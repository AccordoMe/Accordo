// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the core says what it did: one sink, installed by the host.
//!
//! The desktop client shows these records in its devtools panel; a phone logs them to logcat, or
//! drops them. The core knows nothing about either — it only fills in `Net` / `SocketEntry` and
//! calls the trait. With no sink installed every call is a no-op, so a silent build costs nothing.

use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

/// Bodies longer than this are cut (a token list can be megabytes).
const MAX_BODY: usize = 64 * 1024;

/// Severity of a logged line, the same scale as `tracing`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// One HTTP exchange, as the panel shows it.
#[derive(Debug, Clone)]
pub struct Net {
    pub method: String,
    /// Path and query, without the server (shown in the list).
    pub path: String,
    pub url: String,
    /// None: the server wasn't reached.
    pub status: Option<u16>,
    pub millis: u128,
    pub size: usize,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub error: Option<String>,
}

/// What the host does with the records. Called from tokio tasks: keep it cheap and thread-safe.
pub trait Sink: Send + Sync + 'static {
    /// Whether recording is on — the core skips building bodies when it isn't.
    fn enabled(&self) -> bool;
    fn record_net(&self, net: Net);
    fn record_socket(&self, label: String, incoming: bool, body: Option<String>);
    fn record_log(&self, level: Level, target: String, message: String);
}

static SINK: OnceLock<RwLock<Option<Arc<dyn Sink>>>> = OnceLock::new();

fn cell() -> &'static RwLock<Option<Arc<dyn Sink>>> {
    SINK.get_or_init(|| RwLock::new(None))
}

/// Installs (or replaces) the sink. Called once at startup by the host.
pub fn install(sink: Arc<dyn Sink>) {
    if let Ok(mut slot) = cell().write() {
        *slot = Some(sink);
    }
}

fn with(f: impl FnOnce(&dyn Sink)) {
    let guarded = cell().read().ok();
    if let Some(slot) = guarded.as_ref().and_then(|s| s.as_ref()) {
        f(slot.as_ref());
    }
}

/// Whether recording is on (false when no sink is installed).
pub fn enabled() -> bool {
    let mut on = false;
    with(|sink| on = sink.enabled());
    on
}

pub fn record_net(net: Net) {
    with(|sink| sink.record_net(net));
}

pub fn record_socket(label: impl Into<String>, incoming: bool, body: Option<&Value>) {
    let label = label.into();
    let body = body.map(json_text);
    with(|sink| sink.record_socket(label, incoming, body));
}

pub fn record_log(level: Level, target: &str, message: String) {
    let target = target.to_string();
    with(|sink| sink.record_log(level, target, message));
}

/// Fields whose values never leave the process.
const SECRET_KEYS: &[&str] = &[
    "password", "new_password", "current_password", "passphrase", "otp", "code", "recovery_code", "recovery_codes",
    "access_token", "token", "api_key", "key", "secret", "private_key", "priv", "d", "ct", "cipher", "salt", "auth_tag",
];

/// Masks secrets anywhere in a JSON value.
pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    if SECRET_KEYS.contains(&k.as_str()) && !v.is_null() {
                        (k, Value::String("•••".into()))
                    } else {
                        (k, redact(v))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        other => other,
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A body as shown: JSON pretty-printed and masked; anything else as text; cut when huge.
pub fn body_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let text = match serde_json::from_slice::<Value>(bytes) {
        Ok(v) => pretty(&redact(v)),
        Err(_) => match std::str::from_utf8(bytes) {
            Ok(t) => t.to_string(),
            Err(_) => return Some(format!("<{} bytes of binary data>", bytes.len())),
        },
    };
    Some(if text.len() > MAX_BODY {
        let mut cut = MAX_BODY;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n… cut, {} bytes in all", &text[..cut], text.len())
    } else {
        text
    })
}

/// A request body as shown.
pub fn json_text(value: &Value) -> String {
    pretty(&redact(value.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(AtomicUsize);

    impl Sink for Counting {
        fn enabled(&self) -> bool {
            true
        }
        fn record_net(&self, _net: Net) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn record_socket(&self, _label: String, _incoming: bool, _body: Option<String>) {}
        fn record_log(&self, _level: Level, _target: String, _message: String) {}
    }

    #[test]
    fn secrets_are_masked_and_bodies_cut() {
        let masked = redact(serde_json::json!({ "token": "abc", "nested": [{ "password": "x" }], "ok": 1 }));
        assert_eq!(masked["token"], "•••");
        assert_eq!(masked["nested"][0]["password"], "•••");
        assert_eq!(masked["ok"], 1);
        assert_eq!(body_text(b""), None);
        assert!(body_text(&[0xff, 0xfe]).unwrap().contains("binary data"));
    }

    #[test]
    fn a_sink_receives_what_the_core_records() {
        let counter = Arc::new(Counting(AtomicUsize::new(0)));
        install(counter.clone());
        record_net(Net {
            method: "GET".into(),
            path: "/api/me".into(),
            url: "https://accordo.me/api/me".into(),
            status: Some(200),
            millis: 3,
            size: 2,
            request_body: None,
            response_body: None,
            error: None,
        });
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
    }
}
