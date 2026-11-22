// SPDX-License-Identifier: GPL-3.0-or-later
//! The live connection (context/SocketContext.tsx): `/api/ws/notifications`, authenticated with
//! the session token. Chat messages, edits, reactions, room changes, payment events arrive here.
//!
//! Pinged every 25 s; if nothing (not even a pong) came back for two rounds the connection is
//! half-open (sleep, Wi-Fi switch, proxy drop) and is replaced. Reconnects with a growing pause.
//! After a reconnect, events from the gap are lost: `Authenticated { reconnected: true }` tells
//! the app to re-fetch what it shows.

use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use futures::{SinkExt, Stream, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;

const PING_EVERY: Duration = Duration::from_secs(25);
const STALE_AFTER: Duration = Duration::from_secs(60);

/// What the app sends over the open connection ("typing"…): the connection loop writes it out.
/// None while disconnected — those sends are dropped (they're only live signals).
static OUTBOX: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Value>>> = std::sync::Mutex::new(None);

/// Sends `{"action": …}` over the live connection, if there is one. False: not connected.
pub fn send(message: Value) -> bool {
    OUTBOX.lock().ok().and_then(|o| o.as_ref().map(|tx| tx.send(message).is_ok())).unwrap_or(false)
}

#[derive(Debug, Clone)]
pub enum SocketEvent {
    /// The server accepted the token. `reconnected`: this isn't the first time — refresh.
    Authenticated { reconnected: bool },
    /// An event with an `event` field ("chat_message", "room_updated", …), as the server sent it.
    Event(Value),
    Disconnected,
}

/// Who connects (the subscription is restarted when this changes: another account or server).
#[derive(Debug, Clone)]
pub struct SocketKey {
    pub base: String,
    pub token: String,
    pub user_id: i64,
    pub username: String,
}

impl Hash for SocketKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.base.hash(state);
        self.token.hash(state);
        self.user_id.hash(state);
    }
}

/// `https://accordo.me` → `wss://accordo.me/api/ws/notifications`.
fn ws_url(base: &str) -> String {
    let base = super::client::normalize_base(base);
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base
    };
    format!("{base}/api/ws/notifications")
}

/// The event stream: the desktop plugs it into `iced::Subscription::run_with(key, connect)`, a
/// phone polls it from its own loop.
pub fn connect(key: &SocketKey) -> impl Stream<Item = SocketEvent> + use<> {
    let key = key.clone();
    let (sender, events) = futures::channel::mpsc::channel::<SocketEvent>(100);
    let run = async move {
        let mut out = sender;
        let url = ws_url(&key.base);
        let mut authenticated_before = false;
        let mut pause = Duration::from_secs(1);
        loop {
            let connection = tokio_tungstenite::connect_async(url.as_str()).await;
            if let Err(e) = &connection {
                tracing::warn!("socket: can't connect to {url}: {e}");
                crate::log::record_socket(format!("connect failed: {e}"), false, None);
            }
            if let Ok((ws, _)) = connection {
                crate::log::record_socket("connected", false, None);
                tracing::debug!("socket: connected to {url}");
                let (mut write, mut read) = ws.split();
                let auth = json!({ "action": "auth", "token": key.token, "user_id": key.user_id, "username": key.username });
                if write.send(WsMessage::Text(auth.to_string().into())).await.is_ok() {
                    pause = Duration::from_secs(1);
                    let (tx, mut outgoing) = tokio::sync::mpsc::unbounded_channel::<Value>();
                    if let Ok(mut o) = OUTBOX.lock() {
                        *o = Some(tx);
                    }
                    let mut last = Instant::now();
                    let mut next_ping = tokio::time::Instant::now() + PING_EVERY;
                    loop {
                        tokio::select! {
                            item = read.next() => match item {
                                Some(Ok(WsMessage::Text(text))) => {
                                    last = Instant::now();
                                    let Ok(data) = serde_json::from_str::<Value>(&text) else { continue };
                                    if data.get("action").and_then(Value::as_str) == Some("pong") {
                                        continue;
                                    }
                                    if data.get("status").and_then(Value::as_str) == Some("authenticated") {
                                        crate::log::record_socket("authenticated", true, Some(&data));
                                        let _ = out.send(SocketEvent::Authenticated { reconnected: authenticated_before }).await;
                                        authenticated_before = true;
                                        continue;
                                    }
                                    if let Some(name) = data.get("event").and_then(Value::as_str) {
                                        crate::log::record_socket(name.to_string(), true, Some(&data));
                                        let _ = out.send(SocketEvent::Event(data)).await;
                                    }
                                }
                                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                                Some(Ok(_)) => last = Instant::now(), // ping/pong/binary frames
                            },
                            Some(message) = outgoing.recv() => {
                                let label = message.get("action").and_then(Value::as_str).unwrap_or("send").to_string();
                                crate::log::record_socket(label, false, Some(&message));
                                if write.send(WsMessage::Text(message.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                            _ = tokio::time::sleep_until(next_ping) => {
                                if last.elapsed() > STALE_AFTER {
                                    break; // half-open: replace it
                                }
                                if write.send(WsMessage::Text(json!({ "action": "ping" }).to_string().into())).await.is_err() {
                                    break;
                                }
                                next_ping = tokio::time::Instant::now() + PING_EVERY;
                            }
                        }
                    }
                    if let Ok(mut o) = OUTBOX.lock() {
                        *o = None;
                    }
                }
                crate::log::record_socket("disconnected", false, None);
                tracing::debug!("socket: disconnected");
                let _ = out.send(SocketEvent::Disconnected).await;
            }
            tokio::time::sleep(pause).await;
            pause = (pause * 2).min(Duration::from_secs(30));
        }
    };
    // The loop never returns on its own: polling it keeps the connection alive, and everything it
    // sends comes out of the receiver.
    let driver = futures::stream::once(run).flat_map(|_| futures::stream::empty::<SocketEvent>());
    futures::stream::select(events, driver)
}

#[cfg(test)]
mod tests {
    #[test]
    fn socket_url_follows_the_api_base() {
        assert_eq!(super::ws_url("https://accordo.me"), "wss://accordo.me/api/ws/notifications");
        assert_eq!(super::ws_url("127.0.0.1:8000"), "ws://127.0.0.1:8000/api/ws/notifications");
    }
}
