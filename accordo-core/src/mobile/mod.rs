// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Android app calls: the API client and the socket behind a Kotlin-friendly surface.
//!
//! Payloads cross as JSON strings, not as uniffi records: the shapes are exactly the server's (the
//! same ones the desktop client deserializes), so the phone's UI can mirror the desktop screen for
//! screen without a second model layer, and a new field needs no binding change.
//!
//! Sealing happens here, in Rust: `send_text(…, room_key)` seals with the room key before the
//! request leaves, and `messages()` returns what the server sent — the UI opens payloads with
//! `decrypt_text` from the crate root.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::StreamExt;
use serde::Serialize;
use serde_json::json;

use crate::api::client::Api;
use crate::api::{account, auth, chat, data, socket};
use crate::crypto;

/// Everything that can go wrong on the way to the screen. `status` is None when the server wasn't
/// reached (no network, wrong address, TLS).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{detail}")]
    Api { status: Option<u16>, detail: String },
    #[error("the message could not be sealed for this room")]
    SealFailed,
    #[error("the request could not be put together: {detail}")]
    Json { detail: String },
    #[error("{detail}")]
    Store { detail: String },
    #[error("{detail}")]
    Wallet { detail: String },
    /// A sealed room whose key isn't available here (the sentence is shown to the user).
    #[error("{detail}")]
    Keys { detail: String },
}

impl From<String> for MobileError {
    fn from(message: String) -> Self {
        MobileError::Store { detail: message }
    }
}

impl From<crate::api::client::ApiError> for MobileError {
    fn from(e: crate::api::client::ApiError) -> Self {
        MobileError::Api { status: e.status, detail: e.message }
    }
}

impl From<serde_json::Error> for MobileError {
    fn from(e: serde_json::Error) -> Self {
        MobileError::Json { detail: e.to_string() }
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<String, MobileError> {
    Ok(serde_json::to_string(value)?)
}

/// A signed-in (or signing-in) connection to one server. Hold one per account.
#[derive(uniffi::Object)]
pub struct Client {
    /// Swapped after a successful sign-in: every later call carries the token.
    api: RwLock<Api>,
}

impl Client {
    /// The server this client talks to (not exported: `Api` is not a bindable type).
    fn current(&self) -> Api {
        self.api.read().map(|api| api.clone()).unwrap_or_else(|_| Api::guest("https://accordo.me"))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// `token` is None before sign-in (public endpoints only).
    #[uniffi::constructor]
    pub fn new(base: String, token: Option<String>) -> Arc<Self> {
        Arc::new(Self { api: RwLock::new(Api::new(&base, token.as_deref())) })
    }

    #[uniffi::constructor]
    pub fn remembering(base: String, token: String) -> Arc<Self> {
        Self::new(base, Some(token))
    }

    pub fn base(&self) -> String {
        self.current().base().to_string()
    }

    /// Signs in and keeps the token: `{"token": "…", "user": {…}}`. `otp` for 2FA accounts.
    pub async fn login(&self, email_or_username: String, password: String, otp: Option<String>) -> Result<String, MobileError> {
        let api = self.current();
        let answer = auth::login(api.clone(), email_or_username, password, otp).await?;
        if let Ok(mut slot) = self.api.write() {
            *slot = api.with_token(&answer.access_token);
        }
        to_json(&json!({
            "token": answer.access_token,
            "user": answer.user,
            "recovery_codes": answer.recovery_codes,
        }))
    }

    /// The signed-in user, fresh from the server.
    pub async fn me(&self) -> Result<String, MobileError> {
        let user = auth::me(self.current()).await?;
        to_json(&user)
    }

    /// Rooms and DMs, as the desktop's sidebar shows them.
    pub async fn rooms(&self) -> Result<String, MobileError> {
        let rooms = chat::get_rooms(self.current()).await?;
        to_json(&rooms)
    }

    pub async fn conversations(&self) -> Result<String, MobileError> {
        let rooms = chat::get_conversations(self.current()).await?;
        to_json(&rooms)
    }

    /// A page of a room's messages, newest last. `before_id` pages backwards.
    pub async fn messages(&self, room_id: String, before_id: Option<i64>, limit: Option<i64>) -> Result<String, MobileError> {
        let query = chat::MessageQuery {
            room_id: Some(room_id),
            recipient_id: None,
            recipient_username: None,
            limit,
            before_id,
            include_thread_replies: false,
        };
        let messages = chat::get_messages(self.current(), query).await?;
        to_json(&messages)
    }

    /// Sends text into a room. With `room_key`, the text is sealed here, before it leaves the device.
    pub async fn send_text(&self, room_id: String, body: String, room_key: Option<String>, reply_to_id: Option<i64>) -> Result<String, MobileError> {
        let content = match room_key.as_deref() {
            Some(key) => crypto::encrypt_symmetric(&body, key).ok_or(MobileError::SealFailed)?,
            None => body,
        };
        let message = chat::SendMessage {
            room_id: Some(room_id),
            content: Some(content),
            reply_to_id,
            ..Default::default()
        };
        let sent = chat::send_message(self.current(), message).await?;
        to_json(&sent)
    }

    /// Anything richer than text (files, polls, drops…): the JSON of a `SendMessage`, which is the
    /// same shape the web's chat sends. `content` is sealed when `room_key` is given.
    pub async fn send_raw(&self, message_json: String, room_key: Option<String>) -> Result<String, MobileError> {
        let mut message: chat::SendMessage = serde_json::from_str(&message_json)?;
        if let (Some(key), Some(content)) = (room_key.as_deref(), message.content.as_deref()) {
            message.content = Some(crypto::encrypt_symmetric(content, key).ok_or(MobileError::SealFailed)?);
        }
        let sent = chat::send_message(self.current(), message).await?;
        to_json(&sent)
    }

    /// Toggles an emoji on a message (`POST …/reactions`).
    pub async fn react(&self, message_id: i64, emoji: String) -> Result<String, MobileError> {
        let reply = chat::toggle_reaction(self.current(), message_id, emoji).await?;
        to_json(&reply)
    }

    /// Pins or unpins a message.
    pub async fn pin(&self, message_id: i64) -> Result<String, MobileError> {
        let reply = chat::toggle_pin(self.current(), message_id).await?;
        to_json(&reply)
    }

    /// The replies of a thread (root excluded), oldest first: the feed hides them unless asked.
    pub async fn thread(&self, room_id: String, root_id: i64, limit: Option<i64>) -> Result<String, MobileError> {
        let query = chat::MessageQuery {
            room_id: Some(room_id),
            recipient_id: None,
            recipient_username: None,
            limit,
            before_id: None,
            include_thread_replies: true,
        };
        let all = chat::get_messages(self.current(), query).await?;
        let replies: Vec<_> = all.into_iter().filter(|m| m.thread_root_id == Some(root_id)).collect();
        to_json(&replies)
    }

    /// Changes the profile (display name now; the avatar needs an upload, later).
    pub async fn update_profile(&self, display_name: String) -> Result<String, MobileError> {
        let trimmed = display_name.trim().to_string();
        let update = auth::ProfileUpdate {
            display_name: (!trimmed.is_empty()).then_some(trimmed),
            ..Default::default()
        };
        let user = auth::update_profile(self.current(), update).await?;
        to_json(&user)
    }

    /// The networks the server offers (`/api/data/chains`), in its order — the desktop's picker.
    pub async fn chains(&self) -> Result<String, MobileError> {
        let chains = data::chains(self.current()).await?;
        to_json(&chains)
    }

    /// The network picked when there is no pick yet (or the server dropped it): Base, as on the
    /// desktop.
    pub fn default_chain_id(&self) -> String {
        data::DEFAULT_CHAIN_ID.to_string()
    }

    /// Where the account is signed in, each with the desktop's wording ready: `signed_in`
    /// ("Nov 14, 2026") and `active` ("5 h ago").
    pub async fn sessions(&self) -> Result<String, MobileError> {
        let list = account::sessions(self.current()).await?;
        let rows: Vec<_> = list
            .iter()
            .map(|s| {
                let signed_in = s.signed_in_at.as_deref().and_then(account::parse_time).map(|t| t.format("%b %d, %Y").to_string());
                let active = s.last_active_at.as_deref().map(account::ago);
                json!({ "session": s, "signed_in": signed_in, "active": active })
            })
            .collect();
        to_json(&rows)
    }

    /// Signs one other device out.
    pub async fn end_session(&self, id: String) -> Result<(), MobileError> {
        account::end_session(self.current(), id).await?;
        Ok(())
    }

    /// Signs out every other device. This one gets a fresh token (the old one stops working): it
    /// is swapped in here and returned, for the saved session.
    pub async fn end_other_sessions(&self) -> Result<String, MobileError> {
        let api = self.current();
        let token = account::end_other_sessions(api.clone()).await?;
        if let Ok(mut slot) = self.api.write() {
            *slot = api.with_token(&token);
        }
        Ok(token)
    }

    /// The live connection: events arrive as JSON on a background thread, in order.
    pub fn subscribe(&self, user_id: i64, username: String, listener: Box<dyn SocketListener>) -> Arc<SocketSubscription> {
        let api = self.current();
        let token = api.token().unwrap_or_default();
        let key = socket::SocketKey { base: api.base().to_string(), token, user_id, username };
        let stop = Arc::new(AtomicBool::new(false));
        let told_to_stop = stop.clone();
        let listener: Arc<dyn SocketListener> = Arc::from(listener);
        std::thread::Builder::new()
            .name("accordo-socket".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
                runtime.block_on(async move {
                    let mut events = Box::pin(socket::connect(&key));
                    loop {
                        tokio::select! {
                            _ = async {
                                while !told_to_stop.load(Ordering::Relaxed) {
                                    tokio::time::sleep(Duration::from_millis(200)).await;
                                }
                            } => break,
                            event = events.next() => match event {
                                Some(event) => listener.on_event(event_json(&event)),
                                None => break,
                            },
                        }
                    }
                });
            })
            .ok();
        Arc::new(SocketSubscription { stop })
    }
}

/// Handed back by `subscribe`: dropping it, or `close()`, stops the connection.
#[derive(uniffi::Object)]
pub struct SocketSubscription {
    stop: Arc<AtomicBool>,
}

#[uniffi::export]
impl SocketSubscription {
    /// Stops the connection. Named `stop`, not `close`: the generated class already implements
    /// `AutoCloseable.close()`.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Where the live events go. Called on the socket's own thread: hop to the UI thread there.
#[uniffi::export(callback_interface)]
pub trait SocketListener: Send + Sync {
    /// `{"kind":"authenticated","reconnected":false}`,
    /// `{"kind":"event","event":{…}}` (the server's payload untouched),
    /// `{"kind":"disconnected"}`.
    fn on_event(&self, event: String);
}

pub mod keys;
pub mod session;
pub mod signup;
pub mod wallet;

fn event_json(event: &socket::SocketEvent) -> String {
    match event {
        socket::SocketEvent::Authenticated { reconnected } => {
            json!({ "kind": "authenticated", "reconnected": reconnected }).to_string()
        }
        socket::SocketEvent::Event(payload) => json!({ "kind": "event", "event": payload }).to_string(),
        socket::SocketEvent::Disconnected => json!({ "kind": "disconnected" }).to_string(),
    }
}

// ── Where secrets live on this device ──

/// Kotlin's side of secret storage: the Android Keystore behind `EncryptedSharedPreferences`.
/// Installing it makes saved sessions and chat keys survive a restart — the same trait the
/// desktop backs with its OS credential store.
#[uniffi::export(callback_interface)]
pub trait SecretStoreCallback: Send + Sync {
    /// None when there is no such entry.
    fn get(&self, key: String) -> Option<String>;
    /// False when the device refused to store the value.
    fn set(&self, key: String, value: String) -> bool;
    fn remove(&self, key: String);
}

struct Bridged(Arc<dyn SecretStoreCallback>);

impl crate::secrets::SecretStore for Bridged {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key.to_string())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        if self.0.set(key.to_string(), value.to_string()) {
            Ok(())
        } else {
            Err("the device refused to store the secret".into())
        }
    }

    fn remove(&self, key: &str) {
        self.0.remove(key.to_string());
    }
}

/// Call once at startup, before the first session is read.
#[uniffi::export]
pub fn install_secret_store(store: Box<dyn SecretStoreCallback>) {
    let store: Arc<dyn SecretStoreCallback> = Arc::from(store);
    crate::secrets::install(Arc::new(Bridged(store)));
}

/// A server path as a full URL (`/api/avatars/x` → `https://accordo.me/api/avatars/x`), for the
/// image loader: the server hands out relative paths.
#[uniffi::export]
pub fn absolute_url(base: String, path: String) -> String {
    crate::api::client::Api::new(&base, None).url(&path)
}

/// The key an account's secrets are filed under: `1:alina@https://accordo.me` — the same scheme
/// the desktop uses, so a backup moved between devices finds its entries.
#[uniffi::export]
pub fn account_key(id: i64, username: String, base: String) -> String {
    crate::api::session_store::account_key(id, &username, &base)
}

/// The earlier form of an account key (`1@https://accordo.me`): entries under it are moved when
/// the chat key is first read on this device.
#[uniffi::export]
pub fn previous_account_key(id: i64, base: String) -> String {
    crate::api::session_store::previous_account_key(id, &base)
}

/// Whether sessions and chat keys can be stored (false: a forgotten `install_secret_store`).
#[uniffi::export]
pub fn secrets_available() -> bool {
    crate::secrets::installed()
}

// ── This device's chat key ──

/// The account's chat key from the previous launch, if this device has one.
/// `server_key` is the account's public key on the server: proof that an old shared entry belongs
/// to this account and not to the same user id on another server.
#[uniffi::export]
pub fn load_chat_key(account: String, previous: String, user_id: i64, server_key: Option<String>) -> Result<Option<Arc<crate::DeviceKey>>, MobileError> {
    let found = crate::keystore::load(&account, &previous, user_id, server_key.as_deref())?;
    Ok(found.map(crate::DeviceKey::from_pair))
}

#[uniffi::export]
pub fn save_chat_key(account: String, key: Arc<crate::DeviceKey>) -> Result<(), MobileError> {
    crate::keystore::save(&account, key.pair())?;
    Ok(())
}

#[uniffi::export]
pub fn delete_chat_key(account: String) -> Result<(), MobileError> {
    crate::keystore::delete(&account)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// What Kotlin will implement over the Keystore.
    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);

    impl SecretStoreCallback for Memory {
        fn get(&self, key: String) -> Option<String> {
            self.0.lock().ok()?.get(&key).cloned()
        }
        fn set(&self, key: String, value: String) -> bool {
            match self.0.lock() {
                Ok(mut map) => {
                    map.insert(key, value);
                    true
                }
                Err(_) => false,
            }
        }
        fn remove(&self, key: String) {
            if let Ok(mut map) = self.0.lock() {
                map.remove(&key);
            }
        }
    }

    #[test]
    fn a_chat_key_survives_a_restart_through_the_phone_side_store() {
        let _guard = crate::secrets::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        install_secret_store(Box::new(Memory::default()));
        assert!(secrets_available());

        let account = crate::api::session_store::account_key(1, "alina", "https://accordo.me");
        let previous = crate::api::session_store::previous_account_key(1, "https://accordo.me");
        let key = crate::DeviceKey::generate();

        save_chat_key(account.clone(), key.clone()).expect("save");
        let restored = load_chat_key(account.clone(), previous, 1, Some(key.public_jwk()))
            .expect("load")
            .expect("found");
        assert_eq!(restored.public_jwk(), key.public_jwk());
        assert_eq!(crate::fingerprint_of(restored.public_jwk()), crate::fingerprint_of(key.public_jwk()));

        delete_chat_key(account.clone()).expect("delete");
        assert!(load_chat_key(account, String::new(), 1, None).expect("load").is_none());
    }
}
