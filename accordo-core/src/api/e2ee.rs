// SPDX-License-Identifier: GPL-3.0-or-later
//! Chat-key endpoints (utils/e2ee.ts calls these directly): the account's public key and the
//! passphrase-sealed key backup (backend routers/chat.py, /api/chat/e2ee/*).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupStatus {
    pub exists: bool,
    pub created_at: Option<String>,
    /// The public key the backup holds.
    pub public_key: Option<String>,
}

/// The sealed backup, as stored: `{v, kdf, iter, salt, iv, ct, pub}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBackup {
    pub v: i64,
    pub kdf: String,
    pub iter: Option<u32>,
    pub salt: String,
    pub iv: String,
    pub ct: String,
    #[serde(rename = "pub")]
    pub public_key: String,
}

pub async fn backup_status(api: Api) -> ApiResult<BackupStatus> {
    api.get_data("/api/chat/e2ee/key-backup/status").await
}

/// The sealed key (owner only; 10 downloads an hour — each one is an offline guess).
pub async fn get_backup(api: Api) -> ApiResult<KeyBackup> {
    api.get_data("/api/chat/e2ee/key-backup").await
}

/// Stores / replaces the backup. It must hold the account's current public key.
pub async fn put_backup(api: Api, backup: KeyBackup) -> ApiResult<Value> {
    api.put("/api/chat/e2ee/key-backup", &backup).await
}

/// Publishes this device's public key as the account's. A *new* key makes the server drop every
/// envelope wrapped for the old one — only for an account without a key, or an explicit reset.
pub async fn publish_public_key(api: Api, public_key: String) -> ApiResult<Value> {
    api.post("/api/chat/e2ee/public-key", &json!({ "public_key": public_key })).await
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserKey {
    pub id: i64,
    pub username: String,
    pub e2ee_public_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PublicKeys {
    /// By lower-cased username.
    pub keys: std::collections::HashMap<String, UserKey>,
}

/// Several users' public keys at once (for new rooms).
pub async fn public_keys(api: Api, usernames: Vec<String>) -> ApiResult<PublicKeys> {
    let list = usernames.iter().map(|u| super::client::enc(u.trim_start_matches('@'))).collect::<Vec<_>>().join(",");
    api.get(&format!("/api/chat/e2ee/public-keys?usernames={list}")).await
}
