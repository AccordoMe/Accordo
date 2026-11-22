// SPDX-License-Identifier: GPL-3.0-or-later
//! The account's chat key on this computer (restoreKeyBackup, importE2EEBackupFile,
//! createKeyBackup in utils/e2ee.ts). Slow and blocking parts (PBKDF2, the secret store) run off
//! the UI thread. Errors are sentences for the user.

use serde_json::{json, Value};

use super::crypto::{same_public_key, DeviceKeypair};
use super::keystore::{self, blocking};
use super::phrase::normalize_secret;
use super::vault::{decrypt_vault, encrypt_vault, Vault};
use crate::api::client::Api;
use crate::api::e2ee::{self, KeyBackup};

/// Brings the key back from the backup sealed with the chat password, and keeps it here.
pub async fn restore_backup(api: Api, account: String, passphrase: String) -> Result<DeviceKeypair, String> {
    let blob = e2ee::get_backup(api).await.map_err(|e| match e.status {
        Some(404) => "There's no chat key backup on this account yet — set a chat password in the app where your chats open.".to_string(),
        Some(429) => "Too many restore attempts — try again in an hour.".to_string(),
        _ => format!("Couldn't load the backup: {}", e.message),
    })?;
    let vault = Vault { salt: blob.salt, iv: blob.iv, cipher: blob.ct, iter: blob.iter };
    let pass = normalize_secret(&passphrase);
    let opened = blocking(move || decrypt_vault(&vault, &pass).map_err(|_| "That password doesn't open the backup.".to_string())).await?;
    let pair: Value = serde_json::from_str(&opened).map_err(|_| "The backup is damaged.".to_string())?;
    let keypair = keypair_from(&pair["pub"], &pair["priv"]).ok_or("The backup is damaged.")?;
    install(account, keypair).await
}

/// A key file (`{public_key, private_key}`, as the web exports it). Only the account's current
/// key is accepted: the web would publish a different one, replacing the account's key and cutting
/// every chat off — here that file is refused instead.
pub async fn import_key_file(api: Api, account: String, account_key: Option<String>, contents: String) -> Result<DeviceKeypair, String> {
    let keypair = parse_key_file(&contents)?;
    match account_key {
        Some(current) if !same_public_key(&current, &keypair.public_jwk) => {
            return Err("This key file holds a different chat key than your account uses now (an older key, or another account's). Loading it would replace your account's key and lock every chat until members share keys again, so it wasn't loaded.".into());
        }
        Some(_) => {}
        // The account has no key yet: this one becomes it.
        None => {
            e2ee::publish_public_key(api, keypair.public_jwk.clone()).await.map_err(|e| format!("Couldn't publish the key: {}", e.message))?;
        }
    }
    install(account, keypair).await
}

/// Sets / changes the chat password: seals {priv, pub} with it and stores that on Accordo.
pub async fn create_backup(api: Api, keypair: DeviceKeypair, passphrase: String) -> Result<(), String> {
    let pass = normalize_secret(&passphrase);
    // Same JSON as the web: {"priv": "<jwk>", "pub": "<jwk>"}.
    let plain = json!({ "priv": keypair.private_jwk, "pub": keypair.public_jwk }).to_string();
    let vault = blocking(move || Ok(encrypt_vault(&plain, &pass))).await?;
    let backup = KeyBackup { v: 1, kdf: "vault-PBKDF2-SHA256".into(), iter: vault.iter, salt: vault.salt, iv: vault.iv, ct: vault.cipher, public_key: keypair.public_jwk.clone() };
    e2ee::put_backup(api, backup).await.map_err(|e| format!("Couldn't save the backup: {}", e.message))?;
    Ok(())
}

/// Loads this computer's key for the account (`account`: id@server; `server_key`: its public key
/// on the server, to tell an old shared entry apart).
pub async fn load(account: String, previous: String, user_id: i64, server_key: Option<String>) -> Result<Option<DeviceKeypair>, String> {
    blocking(move || keystore::load(&account, &previous, user_id, server_key.as_deref())).await
}

/// Forgets the key on this computer.
#[allow(dead_code)]
pub async fn forget(account: String) -> Result<(), String> {
    blocking(move || keystore::delete(&account)).await
}

/// A key file as the web writes it: `{app, type, public_key, private_key}`, the keys as JWK
/// objects (or JWK text).
pub fn parse_key_file(contents: &str) -> Result<DeviceKeypair, String> {
    let parsed: Value = serde_json::from_str(contents).map_err(|_| "This isn't an Accordo key file (not JSON).".to_string())?;
    if parsed.get("private_key").is_none() || parsed.get("public_key").is_none() {
        return Err("Invalid key file: missing public or private key.".into());
    }
    keypair_from(&parsed["public_key"], &parsed["private_key"]).ok_or_else(|| "The key file is damaged.".into())
}

fn keypair_from(public: &Value, private: &Value) -> Option<DeviceKeypair> {
    let text = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(_) => Some(v.to_string()),
        _ => None,
    };
    let keypair = DeviceKeypair::from_jwk(&text(public)?, &text(private)?)?;
    keypair.self_test().then_some(keypair)
}

async fn install(account: String, keypair: DeviceKeypair) -> Result<DeviceKeypair, String> {
    let to_store = keypair.clone();
    blocking(move || keystore::save(&account, &to_store)).await.map_err(|e| format!("Couldn't keep the key in the system's secret store: {e}"))?;
    Ok(keypair)
}

/// Where this computer stands with the account's chat key.
#[derive(Debug, Clone, Default)]
pub enum ChatKey {
    #[default]
    Loading,
    /// No key here (a new device): restore it from the backup or a key file.
    Missing,
    /// The account's key is here: encrypted chats open.
    Ready(DeviceKeypair),
    /// A key is here, but not the account's (rotated elsewhere, or a reused account id).
    Different(DeviceKeypair),
    /// The system's secret store couldn't be used.
    NoStore(String),
}

impl ChatKey {
    pub fn from_loaded(loaded: Result<Option<DeviceKeypair>, String>, account_key: Option<&str>) -> Self {
        match loaded {
            Err(e) => ChatKey::NoStore(e),
            Ok(None) => ChatKey::Missing,
            Ok(Some(kp)) if account_key.is_some_and(|a| same_public_key(a, &kp.public_jwk)) => ChatKey::Ready(kp),
            Ok(Some(kp)) => ChatKey::Different(kp),
        }
    }

    pub fn keypair(&self) -> Option<&DeviceKeypair> {
        match self {
            ChatKey::Ready(kp) => Some(kp),
            _ => None,
        }
    }
}
