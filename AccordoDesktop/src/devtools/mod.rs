// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's own developer tools (F12): what the browser's DevTools give the web app — every API
//! request (Network), the live connection (Socket) and the app's log (Console).
//!
//! Kept in memory only (the last 500 entries; nothing is written to disk). Secrets never get in:
//! the Authorization header isn't recorded, and passwords, codes, tokens, private keys and sealed
//! backups in bodies are masked. The record types, the masks and the sink protocol live in
//! `accordo_core::log`, so the desktop panel and a phone's logcat receive the same records.
//!
//! Switched by Developer mode in App settings (remembered); ACCORDO_DEVTOOLS=1 forces it on.

pub mod log_layer;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::{DateTime, Local};

// The records the core fills in, and the masks that protect them: one implementation both use.
pub use accordo_core::log::{Level, Net};

const CAPACITY: usize = 500;

static ENABLED: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(forced_on()));
static VERSION: AtomicU64 = AtomicU64::new(0);
static ENTRIES: LazyLock<Mutex<VecDeque<Entry>>> = LazyLock::new(|| Mutex::new(VecDeque::with_capacity(CAPACITY)));

/// Whether recording (and F12) is on.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// ACCORDO_DEVTOOLS=1: on whatever Developer mode says (Developer mode can't switch it off).
pub fn forced_on() -> bool {
    std::env::var_os("ACCORDO_DEVTOOLS").is_some()
}

/// Developer mode on / off. Off forgets what was recorded: nothing lingers in memory.
pub fn set_enabled(on: bool) {
    let on = on || forced_on();
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        clear();
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub at: DateTime<Local>,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Kind {
    Net(Net),
    Socket(SocketEntry),
    Log(LogEntry),
}

#[derive(Debug, Clone)]
pub struct SocketEntry {
    /// "connected", "authenticated", "disconnected", or the event name ("chat_message"…).
    pub label: String,
    pub incoming: bool,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: tracing::Level,
    pub target: String,
    pub message: String,
}

fn push(kind: Kind) {
    if !enabled() {
        return;
    }
    let id = VERSION.fetch_add(1, Ordering::Relaxed) + 1;
    if let Ok(mut list) = ENTRIES.lock() {
        if list.len() == CAPACITY {
            list.pop_front();
        }
        list.push_back(Entry { id, at: Local::now(), kind });
    }
}

/// Changes whenever something was recorded (the panel redraws only then).
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

pub fn snapshot() -> Vec<Entry> {
    ENTRIES.lock().map(|l| l.iter().cloned().collect()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut l) = ENTRIES.lock() {
        l.clear();
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// The sink the core writes into: this module's ring buffer.
pub struct Panel;

impl accordo_core::log::Sink for Panel {
    fn enabled(&self) -> bool {
        enabled()
    }

    fn record_net(&self, net: Net) {
        push(Kind::Net(net));
    }

    fn record_socket(&self, label: String, incoming: bool, body: Option<String>) {
        push(Kind::Socket(SocketEntry { label, incoming, body }));
    }

    fn record_log(&self, level: Level, target: String, message: String) {
        push(Kind::Log(LogEntry { level: from_core(level), target, message }));
    }
}

/// Connects the core to the panel: call once, before anything talks to the server.
pub fn install() {
    accordo_core::log::install(Arc::new(Panel));
}

pub fn record_log(level: tracing::Level, target: &str, message: String) {
    accordo_core::log::record_log(to_core(level), target, message);
}

fn to_core(level: tracing::Level) -> Level {
    match level {
        tracing::Level::TRACE => Level::Trace,
        tracing::Level::DEBUG => Level::Debug,
        tracing::Level::INFO => Level::Info,
        tracing::Level::WARN => Level::Warn,
        tracing::Level::ERROR => Level::Error,
    }
}

fn from_core(level: Level) -> tracing::Level {
    match level {
        Level::Trace => tracing::Level::TRACE,
        Level::Debug => tracing::Level::DEBUG,
        Level::Info => tracing::Level::INFO,
        Level::Warn => tracing::Level::WARN,
        Level::Error => tracing::Level::ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accordo_core::log::redact;
    use serde_json::json;

    #[test]
    fn the_core_reports_into_the_panel() {
        install();
        set_enabled(true);
        clear();
        accordo_core::log::record_socket("test-frame", true, Some(&json!({ "token": "secret" })));
        let entries = snapshot();
        assert!(
            entries.iter().any(|e| matches!(&e.kind, Kind::Socket(s)
                if s.label == "test-frame" && s.body.as_deref().is_some_and(|b| b.contains("•••")))),
            "the core's frame arrived in the panel, masked"
        );
        clear();
        set_enabled(false);
    }

    #[test]
    fn secrets_are_masked_everywhere() {
        let v = redact(json!({
            "email_or_username": "dazay", "password": "hunter2", "otp": "123456",
            "user": { "access_token": "eyJ…", "e2ee_public_key": "{…}" },
            "backup": [{ "ct": "sealed", "iv": "x" }],
        }));
        assert_eq!(v["password"], "•••");
        assert_eq!(v["otp"], "•••");
        assert_eq!(v["user"]["access_token"], "•••");
        assert_eq!(v["user"]["e2ee_public_key"], "{…}");
        assert_eq!(v["backup"][0]["ct"], "•••");
        assert_eq!(v["email_or_username"], "dazay");
    }
}
