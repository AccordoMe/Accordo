// SPDX-License-Identifier: GPL-3.0-or-later
//! Where secrets live between launches: one trait, installed by the host.
//!
//! On the desktop it is the OS credential store (Linux: Secret Service — GNOME Keyring / KWallet;
//! Windows: Credential Manager; macOS: Keychain). On a phone it is the Android Keystore behind
//! `EncryptedSharedPreferences`. The core only ever sees "key → JSON string", so the same session
//! and chat-key code runs on both, and nothing secret is written to a plain file by accident.
//!
//! With no store installed every read answers `None` and every write fails loudly — a forgotten
//! `install` shows up as "can't save the session", not as silence.

use std::sync::{Arc, OnceLock, RwLock};

pub trait SecretStore: Send + Sync + 'static {
    /// The stored value, or None when there is no such entry.
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn remove(&self, key: &str);
}

static STORE: OnceLock<RwLock<Option<Arc<dyn SecretStore>>>> = OnceLock::new();

/// Tests install their own store into the process-wide slot: take this lock so two of them never
/// run at the same time.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn cell() -> &'static RwLock<Option<Arc<dyn SecretStore>>> {
    STORE.get_or_init(|| RwLock::new(None))
}

/// Installs (or replaces) the store. Called once at startup by the host.
pub fn install(store: Arc<dyn SecretStore>) {
    if let Ok(mut slot) = cell().write() {
        *slot = Some(store);
    }
}

fn with<T>(f: impl FnOnce(&dyn SecretStore) -> T, when_absent: T) -> T {
    let guarded = cell().read().ok();
    match guarded.as_ref().and_then(|s| s.as_ref()) {
        Some(store) => f(store.as_ref()),
        None => when_absent,
    }
}

/// Whether a store is installed (the app can tell the user "sessions won't be remembered").
pub fn installed() -> bool {
    with(|_| true, false)
}

pub fn get(key: &str) -> Option<String> {
    with(|store| store.get(key), None)
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    with(|store| store.set(key, value), Err("no secret store installed".into()))
}

pub fn remove(key: &str) {
    with(|store| store.remove(key), ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);

    impl SecretStore for Memory {
        fn get(&self, key: &str) -> Option<String> {
            self.0.lock().ok()?.get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.lock().map_err(|e| e.to_string())?.insert(key.into(), value.into());
            Ok(())
        }
        fn remove(&self, key: &str) {
            if let Ok(mut map) = self.0.lock() {
                map.remove(key);
            }
        }
    }

    #[test]
    fn the_host_decides_where_secrets_live() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        install(Arc::new(Memory::default()));
        assert!(installed());
        assert_eq!(get("missing"), None);
        set("session:1:alina@x", "{\"t\":1}").expect("write");
        assert_eq!(get("session:1:alina@x").as_deref(), Some("{\"t\":1}"));
        remove("session:1:alina@x");
        assert_eq!(get("session:1:alina@x"), None);
    }
}
