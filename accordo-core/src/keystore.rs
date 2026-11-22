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
    let json = serde_json::to_string(&Stored { public_jwk: keypair.public_jwk.clone(), private_jwk: keypair.private_jwk.clone() }).map_err(|e| e.to_string())?;
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
    #[test]
    fn one_file_per_account_and_server() {
        assert_eq!(super::file_safe("12:dazay@http://127.0.0.1:8000"), "12_dazay@http___127.0.0.1_8000");
        assert_ne!(super::file_safe("1@https://accordo.me"), super::file_safe("1@http://127.0.0.1:8000"));
    }
}
