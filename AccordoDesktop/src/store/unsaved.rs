// SPDX-License-Identifier: GPL-3.0-or-later
//! Records that are live on-chain but that Accordo couldn't save (utils/unsavedRecords.ts,
//! UnsavedRecordsBanner.tsx). A cheque's record holds its claim secret, which exists nowhere
//! else — so it's kept in a file next to the app's state (0600, like state.json) until the
//! backend has it. Retried quietly after sign-in.
//!
//! Records belong to the account that made them: only that account retries them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::client::{config_dir, Api};

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    /// "cheque" | "drop" | "invoice" | "campaign" | "nft_collection" | "nft_drop"
    pub kind: String,
    /// Where it's posted ("/api/cheques").
    pub path: String,
    pub payload: Value,
    #[serde(rename = "txHash", default)]
    pub tx_hash: Option<String>,
    #[serde(rename = "savedAt")]
    pub saved_at: String,
    /// The username that created it.
    #[serde(default)]
    pub account: String,
    /// The server it belongs to (usernames repeat across servers; a record must never be posted
    /// to another one — a cheque's has its secret). Empty in records from older versions.
    #[serde(default)]
    pub server: String,
}

/// Never shows the payload (a cheque secret is in it).
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Record").field("kind", &self.kind).field("path", &self.path).field("tx_hash", &self.tx_hash).finish_non_exhaustive()
    }
}

fn path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("unsaved-records.json"))
}

fn same(a: &Record, b: &Record) -> bool {
    a.saved_at == b.saved_at && a.path == b.path && a.tx_hash == b.tx_hash
}

pub fn list() -> Vec<Record> {
    path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write(list: &[Record]) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no config directory"))?;
    if list.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp, path)
}

/// Keeps one more record (`account`: the username, `server`: its server).
pub fn keep(kind: &str, path: &str, payload: Value, tx_hash: Option<String>, account: &str, server: &str) -> std::io::Result<()> {
    let mut all = list();
    all.push(Record { kind: kind.into(), path: path.into(), payload, tx_hash, saved_at: chrono::Utc::now().to_rfc3339(), account: account.into(), server: crate::api::client::normalize_base(server) });
    write(&all)
}

/// Whether a record is this account's: its username on its server (records from before the server
/// was kept: by username alone).
fn is_mine(r: &Record, account: &str, server: &str) -> bool {
    r.account == account && (r.server.is_empty() || r.server == crate::api::client::normalize_base(server))
}

fn remove(r: &Record) -> std::io::Result<()> {
    let all: Vec<Record> = list().into_iter().filter(|x| !same(x, r)).collect();
    write(&all)
}

/// How many records of this account are still waiting.
pub fn pending(account: &str, server: &str) -> usize {
    list().iter().filter(|r| is_mine(r, account, server)).count()
}

/// Posts every record of `account` again; (saved now, still waiting). A 409 means the backend
/// has it already (registered meanwhile).
pub async fn retry_all(api: Api, account: String) -> (usize, usize) {
    let server = api.base().to_string();
    let mine: Vec<Record> = list().into_iter().filter(|r| is_mine(r, &account, &server)).collect();
    let mut saved = 0;
    for r in &mine {
        let ok = match api.record_on_chain_tx::<_, Value>(&r.path, &r.payload).await {
            Ok(_) => true,
            Err(e) => e.status == Some(409),
        };
        if ok && remove(r).is_ok() {
            saved += 1;
        }
    }
    (saved, mine.len() - saved)
}

/// The kinds of this account's waiting records, oldest first ("cheque", "drop"…).
pub fn kinds(account: &str, server: &str) -> Vec<String> {
    list().into_iter().filter(|r| is_mine(r, account, server)).map(|r| r.kind).collect()
}

/// How a kind reads in the banner (UnsavedRecordsBanner.tsx KIND_LABEL).
pub fn label(kind: &str) -> &str {
    match kind {
        "nft_collection" => "NFT collection",
        "nft_drop" => "NFT drop",
        "cheque" | "drop" | "invoice" | "campaign" => kind,
        _ => "record",
    }
}

/// The banner's line: "1 cheque is" / "3 records (cheque, invoice) are" …
pub fn summary(kinds: &[String]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for k in kinds {
        let l = label(k);
        if !names.contains(&l) {
            names.push(l);
        }
    }
    match kinds.len() {
        1 => format!("1 {} is", names[0]),
        n => format!("{n} records ({}) are", names.join(", ")),
    }
}

/// Backup (the web's download): a copy of this account's records in the Downloads folder (or the
/// home folder), private to the user — cheque claim secrets are in it.
pub fn save_copy(account: &str, server: &str) -> std::io::Result<PathBuf> {
    let mine: Vec<Record> = list().into_iter().filter(|r| is_mine(r, account, server)).collect();
    let downloads = crate::components::file_picker::downloads();
    let dir = if downloads.is_dir() { downloads } else { crate::components::file_picker::home() };
    let path = dir.join(format!("accordo-unsaved-records-{}.json", chrono::Local::now().format("%Y-%m-%d")));
    let bytes = serde_json::to_vec_pretty(&mine).map_err(std::io::Error::other)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(&bytes)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_line_reads_like_the_web() {
        assert_eq!(summary(&["cheque".into()]), "1 cheque is");
        assert_eq!(summary(&["cheque".into(), "nft_drop".into(), "cheque".into()]), "3 records (cheque, NFT drop) are");
    }
}
