// SPDX-License-Identifier: GPL-3.0-or-later
//! Remembers signed-in sessions between launches, through the host's secret store
//! (`crate::secrets`): on the desktop the OS credential store (Linux: Secret Service — GNOME
//! Keyring / KWallet; Windows: Credential Manager; macOS: Keychain), on a phone the Android
//! Keystore. The web app keeps the same token in localStorage (AuthContext.tsx).
//!
//! One entry per account, `session:<id>:<nick>@<server>` (the list of accounts, which isn't
//! secret, is in state.json): an entry is small (Windows takes ~2.5 KB each), and losing or
//! removing one doesn't touch the others. Older names are read once more and moved: the single
//! `session` entry from before several accounts, and `session:<id>@<server>` from before the nick
//! was in the key.

use serde::{Deserialize, Serialize};

/// The single entry from before several accounts.
const LEGACY: &str = "session";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub base: String,
    pub token: String,
    /// Read from an older entry (its name): saved under the account's key, then that one goes.
    #[serde(skip)]
    pub moved_from: Option<String>,
}

fn name_of(key: &str) -> String {
    format!("session:{key}")
}

fn read(name: &str) -> Option<Saved> {
    let stored = crate::secrets::get(name)?;
    serde_json::from_str(&stored).ok()
}

/// An account's session: under its key, else under its key's earlier form (`previous`).
fn read_account(key: &str, previous: Option<&str>) -> Option<Saved> {
    if let Some(s) = read(&name_of(key)) {
        return Some(s);
    }
    let old = name_of(previous?);
    read(&old).map(|s| Saved { moved_from: Some(old), ..s })
}

/// At start-up, before the window opens: the open account's session, else the old single one.
pub fn load(active: Option<&str>, previous: Option<&str>) -> Option<Saved> {
    if let Some(s) = active.and_then(|k| read_account(k, previous)) {
        return Some(s);
    }
    read(LEGACY).map(|s| Saved { moved_from: Some(LEGACY.into()), ..s })
}

/// One account's session (switching to it), off the UI thread.
pub async fn load_account(key: String, previous: Option<String>) -> Option<Saved> {
    tokio::task::spawn_blocking(move || read_account(&key, previous.as_deref())).await.ok().flatten()
}

/// Saves an account's session (off the UI thread: the store may take a moment or ask to unlock).
/// A session read from an older entry is moved: saved under its account, the old one removed.
pub async fn save(key: String, saved: Saved) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let json = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
        let name = name_of(&key);
        crate::secrets::set(&name, &json)?;
        if let Some(old) = saved.moved_from.filter(|o| *o != name) {
            remove(&old);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn remove(name: &str) {
    crate::secrets::remove(name);
}

/// Forgets an account's session (sign-out, or the server no longer accepts the token) — and the
/// old single entry, if it's still there.
pub async fn clear(key: Option<String>) {
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(k) = key {
            remove(&name_of(&k));
        }
        remove(LEGACY);
    })
    .await;
}

/// The key an account's entries live under: `1:alina@https://accordo.me`.
pub fn account_key(id: i64, username: &str, base: &str) -> String {
    format!("{id}:{username}@{}", super::client::normalize_base(base))
}

/// The key's earlier form, `1@https://accordo.me`: entries under it move to `account_key`.
pub fn previous_account_key(id: i64, base: &str) -> String {
    format!("{id}@{}", super::client::normalize_base(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// A store that keeps everything in memory: the session code doesn't care where secrets live.
    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);

    impl crate::secrets::SecretStore for Memory {
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
    fn a_session_survives_a_round_trip_and_moves_from_the_old_key() {
        // A blocking runtime inside a sync test: the process-wide store is shared, so the guard
        // must not live across an await point.
        let _guard = crate::secrets::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
        runtime.block_on(async {
        crate::secrets::install(Arc::new(Memory::default()));
        let key = account_key(1, "alina", "https://accordo.me");
        let previous = previous_account_key(1, "https://accordo.me");
        let saved = Saved { base: "https://accordo.me".into(), token: "t".into(), moved_from: None };

        save(previous.clone(), saved).await.expect("save under the old key");
        let found = load_account(key.clone(), Some(previous.clone())).await.expect("read");
        assert_eq!(found.token, "t");
        assert_eq!(found.moved_from.as_deref(), Some(format!("session:{previous}").as_str()));

        // Saving under the new key removes the old entry.
        let moved = Saved { base: "https://accordo.me".into(), token: "t2".into(), moved_from: Some(format!("session:{previous}")) };
        save(key.clone(), moved).await.expect("save");
        assert!(crate::secrets::get(&format!("session:{previous}")).is_none());
        let again = load_account(key, None).await.expect("read");
        assert_eq!(again.token, "t2");
        });
    }
}
