// SPDX-License-Identifier: GPL-3.0-or-later
//! Where this computer keeps an account's chat key (the web: a non-extractable key in IndexedDB
//! plus copies in localStorage, per account).
//!
//! - the keypair (both JWK halves): in the host's secret store (`crate::secrets`), entry
//!   `e2ee:<id>:<nick>@<server>` — the OS credential store on the desktop, the Keystore on a phone;
//! - contact-key pins (not secret): `<config dir>/e2ee-pins-<id>:<nick>@<server>.json` (made safe
//!   as a file name).
//!
//! `account` is `store::app_store::account_key(id, nick, server)`: user ids repeat across servers,
//! so the key of user 1 on accordo.me and user 1 on a local server must not share an entry.
//!
//! An entry under the key's earlier form (`<id>@<server>`, `previous`) is this account's: it just
//! moves. Before that, the entry was `e2ee:<id>` (and the pins `e2ee-pins-<id>.json`). Such an entry is
//! moved to the account only if its public key is the account's own on the server — otherwise it
//! belongs to the same id on another server, and it's left where it is for that account.

use serde::{Deserialize, Serialize};

use crate::crypto::{same_public_key, DeviceKeypair};
use crate::trust::TrustState;
use crate::api::client::config_dir;

#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(rename = "pub")]
    public_jwk: String,
    #[serde(rename = "priv")]
    private_jwk: String,
    /// When the key was put on this computer (ms since the epoch), for its `Lifetime`. Entries
    /// written before it was recorded start counting at the first `expire`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<i64>,
}

const DAY_MS: i64 = 24 * 60 * 60_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// How long a chat key stays on this computer once it's put here (the desktop's App settings →
/// Storage). When it runs out the key is deleted here and comes back with the chat password —
/// so it only applies to accounts with a backup on Accordo: the app checks that before `expire`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifetime {
    #[default]
    Forever,
    /// Until the app closes (a crash: the next start removes it).
    ThisRun,
    Days(u32),
}

impl Lifetime {
    /// A key put here at `since` has run out (`started_ms`: when this run of the app began).
    pub fn ran_out(self, since: i64, now_ms: i64, started_ms: i64) -> bool {
        match self {
            Lifetime::Forever => false,
            Lifetime::ThisRun => since < started_ms,
            Lifetime::Days(days) => now_ms >= since.saturating_add(i64::from(days) * DAY_MS),
        }
    }
}

/// Applies `lifetime` to the account's key on this computer: deletes it once it ran out (true).
/// Call only for an account with a backup on Accordo. Blocking: call off the UI thread.
pub fn expire(account: &str, lifetime: Lifetime, now_ms: i64, started_ms: i64) -> Result<bool, String> {
    if lifetime == Lifetime::Forever {
        return Ok(false);
    }
    let name = scoped(account);
    let Some(json) = crate::secrets::get(&name) else { return Ok(false) };
    let mut stored: Stored = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    match stored.since {
        Some(since) if lifetime.ran_out(since, now_ms, started_ms) => {
            crate::secrets::remove(&name);
            tracing::info!("chat key: its time on this computer ran out — deleted here");
            Ok(true)
        }
        Some(_) => Ok(false),
        None => {
            // From before `since` was kept: the count starts now.
            stored.since = Some(now_ms);
            crate::secrets::set(&name, &serde_json::to_string(&stored).map_err(|e| e.to_string())?)?;
            Ok(false)
        }
    }
}

fn scoped(account: &str) -> String {
    format!("e2ee:{account}")
}

fn legacy(user_id: i64) -> String {
    format!("e2ee:{user_id}")
}

fn read(name: &str) -> Result<Option<DeviceKeypair>, String> {
    let Some(json) = crate::secrets::get(name) else { return Ok(None) };
    let s: Stored = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(DeviceKeypair::from_jwk(&s.public_jwk, &s.private_jwk))
}

/// This computer's key for the account, if it has one (blocking: call off the UI thread).
/// `server_key`: the account's public key on the server — the only proof an old `e2ee:<id>`
/// entry is this account's and not the same id's on another server.
pub fn load(account: &str, previous: &str, user_id: i64, server_key: Option<&str>) -> Result<Option<DeviceKeypair>, String> {
    if let Some(kp) = read(&scoped(account))? {
        return Ok(Some(kp));
    }
    if let Some(kp) = read(&scoped(previous))? {
        save(account, &kp)?;
        crate::secrets::remove(&scoped(previous));
        if let (Some(from), Some(to)) = (pins_path(previous), pins_path(account)) {
            if from.exists() && !to.exists() {
                let _ = std::fs::rename(from, to);
            }
        }
        return Ok(Some(kp));
    }
    let Some(old) = read(&legacy(user_id))? else { return Ok(None) };
    if !server_key.is_some_and(|k| same_public_key(k, &old.public_jwk)) {
        // Someone else's (same id, another server): not this account's, not touched.
        tracing::info!("chat key: the old shared entry isn't this account's key; left for its owner");
        return Ok(None);
    }
    save(account, &old)?;
    crate::secrets::remove(&legacy(user_id));
    // Its pins go with it.
    if let (Some(from), Some(to)) = (legacy_pins_path(user_id), pins_path(account)) {
        if from.exists() && !to.exists() {
            let _ = std::fs::rename(from, to);
        }
    }
    tracing::info!("chat key: moved to this account's own entry");
    Ok(Some(old))
}

pub fn save(account: &str, keypair: &DeviceKeypair) -> Result<(), String> {
    let json = serde_json::to_string(&Stored { public_jwk: keypair.public_jwk.clone(), private_jwk: keypair.private_jwk.clone(), since: Some(now_ms()) }).map_err(|e| e.to_string())?;
    crate::secrets::set(&scoped(account), &json)
}

/// Forgets the key on this computer (log out & remove keys).
pub fn delete(account: &str) -> Result<(), String> {
    crate::secrets::remove(&scoped(account));
    Ok(())
}

/// "12@http://127.0.0.1:8000" → "12@http___127.0.0.1_8000": a file name on every system.
fn file_safe(account: &str) -> String {
    account.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '-' | '_') { c } else { '_' }).collect()
}

fn pins_path(account: &str) -> Option<std::path::PathBuf> {
    config_dir().map(|d| d.join(format!("e2ee-pins-{}.json", file_safe(account))))
}

fn legacy_pins_path(user_id: i64) -> Option<std::path::PathBuf> {
    config_dir().map(|d| d.join(format!("e2ee-pins-{user_id}.json")))
}

pub fn load_pins(account: &str) -> TrustState {
    pins_path(account).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_pins(account: &str, pins: &TrustState) -> std::io::Result<()> {
    let path = pins_path(account).ok_or_else(|| std::io::Error::other("no config directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec(pins).map_err(std::io::Error::other)?)
}

/// Runs a blocking secret-store call off the UI thread.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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
    fn a_chat_key_leaves_when_its_lifetime_runs_out() {
        let _guard = crate::secrets::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::secrets::install(Arc::new(Memory::default()));
        let account = "1:dazay@https://accordo.me";
        let kp = DeviceKeypair::generate();
        save(account, &kp).unwrap();
        let since = now_ms();
        let here = || crate::secrets::get(&scoped(account)).is_some();

        // Forever: never; 7 days: not before, gone at the mark.
        assert!(!expire(account, Lifetime::Forever, since + 400 * DAY_MS, since).unwrap() && here());
        assert!(!expire(account, Lifetime::Days(7), since + 7 * DAY_MS - 1, since).unwrap() && here());
        assert!(expire(account, Lifetime::Days(7), since + 7 * DAY_MS, since).unwrap() && !here());

        // Until the app closes: kept in the run it came in, gone in the next one.
        save(account, &kp).unwrap();
        assert!(!expire(account, Lifetime::ThisRun, now_ms(), since - 1).unwrap() && here());
        assert!(expire(account, Lifetime::ThisRun, now_ms() + 1, now_ms() + 1).unwrap() && !here());

        // An entry from before `since` was kept starts counting at the first check, and still loads.
        let old = serde_json::to_string(&Stored { public_jwk: kp.public_jwk.clone(), private_jwk: kp.private_jwk.clone(), since: None }).unwrap();
        crate::secrets::set(&scoped(account), &old).unwrap();
        assert!(!expire(account, Lifetime::Days(1), 5_000, 0).unwrap());
        assert!(expire(account, Lifetime::Days(1), 5_000 + DAY_MS, 0).unwrap());
        crate::secrets::set(&scoped(account), &old).unwrap();
        assert!(read(&scoped(account)).unwrap().is_some());
    }

    #[test]
    fn one_file_per_account_and_server() {
        assert_eq!(super::file_safe("12:dazay@http://127.0.0.1:8000"), "12_dazay@http___127.0.0.1_8000");
        assert_ne!(super::file_safe("1@https://accordo.me"), super::file_safe("1@http://127.0.0.1:8000"));
    }
}
