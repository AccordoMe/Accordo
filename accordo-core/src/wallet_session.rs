// SPDX-License-Identifier: GPL-3.0-or-later
//! The wallet unlock session (utils/walletSession.ts + vaultUpgrade.ts).
//!
//! Unlocking derives the AES key of every vault (each has its own salt) from the master password
//! and keeps those keys in this process's memory, wiped when dropped. The master password itself
//! is kept (in memory only) just to open a wallet added on another device meanwhile, whose vault
//! has a new salt. At the lock deadline, on "Lock now" and on sign-out everything is dropped.
//!
//! Unlike the web, nothing is written to disk: closing the app always locks the wallets.

use std::collections::HashMap;
use std::sync::Arc;

use zeroize::{Zeroize, Zeroizing};

use crate::vault::{decrypt_vault, decrypt_with_key, derive_key, encrypt_vault, encrypt_with_key, iterations, needs_upgrade, usable, SecureWallet, Vault, VaultError, WalletVault};

/// What the master-password check vault holds.
/// What the master-password check vault holds (the web writes the same string).
pub const CHECK: &str = "VALID_PASSWORD";

fn id(salt: &str, iter: u32) -> String {
    format!("{salt}:{iter}")
}

struct Key {
    salt: String,
    iter: u32,
    bytes: Zeroizing<[u8; 32]>,
}

/// The outcome of `unlock`, applied by the app. Its Debug never shows key material.
pub struct Unlocked {
    keys: Vec<Key>,
    password: Zeroizing<String>,
    /// The check vault to keep (new, upgraded or the same).
    pub check: Vault,
    /// Wallets with legacy vaults re-sealed; Some when anything changed (sync to the account).
    pub upgraded: Option<Vec<SecureWallet>>,
}

impl std::fmt::Debug for Unlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unlocked {{ keys: {}, .. }}", self.keys.len())
    }
}

/// Checks `password` and works out the session (slow: PBKDF2 per distinct salt — run it off the
/// UI thread). Mirrors unlockWallets: the check vault must open, the password must open at least
/// one of the account's wallets if it has any, legacy vaults are re-sealed, then a key is derived
/// for every vault it opens.
pub fn unlock(password: &str, check: Option<&Vault>, wallets: &[SecureWallet]) -> Result<Unlocked, String> {
    // Every distinct (salt, iterations) once, all at the same time (the web's Promise.all):
    // the check vault and each wallet vault usually share a few salts at most.
    let mut wanted: Vec<(String, u32)> = Vec::new();
    for v in check.into_iter().cloned().chain(wallets.iter().filter_map(|w| w.vault.parsed())) {
        let k = (v.salt.clone(), iterations(&v));
        if !wanted.contains(&k) {
            wanted.push(k);
        }
    }
    let mut derived: HashMap<String, Zeroizing<[u8; 32]>> = std::thread::scope(|sc| {
        let jobs: Vec<_> = wanted.iter().map(|(salt, iter)| sc.spawn(move || (id(salt, *iter), derive_key(password, salt, *iter).map(Zeroizing::new)))).collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).filter_map(|(k, key)| key.map(|key| (k, key))).collect()
    });

    if let Some(check) = check {
        let key = derived.get(&id(&check.salt, iterations(check))).map(|k| **k);
        if !key.is_some_and(|k| matches!(decrypt_with_key(check, &k).as_deref(), Ok(CHECK))) {
            return Err("Invalid master password".into());
        }
    }

    // A vault not seen above (re-sealed ones below) is derived on demand.
    let mut key_for = |v: &Vault| -> Option<[u8; 32]> {
        let k = id(&v.salt, iterations(v));
        if let Some(found) = derived.get(&k) {
            return Some(**found);
        }
        let key = derive_key(password, &v.salt, iterations(v))?;
        derived.insert(k, Zeroizing::new(key));
        Some(key)
    };
    let opens = |v: &Vault, key: Option<[u8; 32]>| key.is_some_and(|k| decrypt_with_key(v, &k).is_ok());

    let vaults: Vec<Vault> = wallets.iter().filter_map(|w| w.vault.parsed()).collect();
    if !vaults.is_empty() && !vaults.iter().any(|v| opens(v, key_for(v))) {
        return Err("This password doesn't open your wallets — they were created under a different master password".into());
    }

    // Re-seal what's still on the old iteration count (vaults of other passwords stay as they are).
    let mut check = match check {
        Some(c) if needs_upgrade(c) => encrypt_vault(CHECK, password),
        Some(c) => c.clone(),
        None => encrypt_vault(CHECK, password),
    };
    if !usable(&check) {
        check = encrypt_vault(CHECK, password);
    }
    let mut changed = false;
    let wallets: Vec<SecureWallet> = wallets
        .iter()
        .map(|w| {
            let Some(v) = w.vault.parsed().filter(needs_upgrade) else { return w.clone() };
            match decrypt_vault(&v, password) {
                Ok(mut plain) => {
                    changed = true;
                    let next = encrypt_vault(&plain, password);
                    plain.zeroize();
                    SecureWallet { vault: WalletVault::Sealed(next), ..w.clone() }
                }
                Err(_) => w.clone(),
            }
        })
        .collect();

    let mut keys: Vec<Key> = Vec::new();
    for v in std::iter::once(&check).chain(wallets.iter().filter_map(|w| w.vault.parsed()).collect::<Vec<_>>().iter()) {
        let iter = iterations(v);
        if keys.iter().any(|k| k.salt == v.salt && k.iter == iter) {
            continue;
        }
        if let Some(key) = key_for(v) {
            if decrypt_with_key(v, &key).is_ok() {
                keys.push(Key { salt: v.salt.clone(), iter, bytes: Zeroizing::new(key) });
            }
        }
    }

    Ok(Unlocked { keys, password: Zeroizing::new(password.to_string()), check, upgraded: changed.then_some(wallets) })
}

/// Off the UI thread.
pub async fn unlock_async(password: String, check: Option<Vault>, wallets: Vec<SecureWallet>) -> Result<Arc<Unlocked>, String> {
    let password = Zeroizing::new(password);
    tokio::task::spawn_blocking(move || unlock(&password, check.as_ref(), &wallets).map(Arc::new)).await.map_err(|e| e.to_string())?
}

#[derive(Default)]
pub struct WalletSession {
    keys: HashMap<String, Key>,
    password: Option<Zeroizing<String>>,
    /// When it locks again (ms since the epoch); None: until restart.
    pub lock_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// Locked: unlock with the master password first.
    Locked,
    Vault(VaultError),
    NoKey,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OpenError::Locked => "Your wallets are locked — unlock them with your master password",
            OpenError::NoKey => "This wallet has no key stored",
            OpenError::Vault(VaultError::WrongPassword) => {
                "This wallet is locked with a different master password than the one you unlocked with. Unlock with the password you created the wallet under."
            }
            OpenError::Vault(VaultError::Malformed) => "This wallet's key is damaged",
        })
    }
}

impl WalletSession {
    pub fn unlocked(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Takes the keys `unlock` worked out; `ttl_minutes` 0: until restart.
    pub fn start(&mut self, done: &Unlocked, ttl_minutes: u32, now_ms: i64) {
        self.keys = done.keys.iter().map(|k| (id(&k.salt, k.iter), Key { salt: k.salt.clone(), iter: k.iter, bytes: k.bytes.clone() })).collect();
        self.password = Some(done.password.clone());
        self.lock_at = (ttl_minutes > 0).then(|| now_ms + i64::from(ttl_minutes) * 60_000);
    }

    /// Forgets the keys and the password (wiped from memory).
    pub fn lock(&mut self) {
        self.keys.clear();
        self.password = None;
        self.lock_at = None;
    }

    /// The deadline passed?
    pub fn expired(&self, now_ms: i64) -> bool {
        self.lock_at.is_some_and(|t| now_ms >= t)
    }

    /// Opens a wallet's vault (decryptWalletVault). A wallet added on another device meanwhile
    /// (new salt) is opened with the password typed this session. Zeroize the result after use.
    pub fn open(&mut self, vault: &WalletVault) -> Result<Zeroizing<String>, OpenError> {
        let v = vault.parsed().ok_or(OpenError::NoKey)?;
        let k = id(&v.salt, iterations(&v));
        if !self.keys.contains_key(&k) {
            let password = self.password.as_ref().ok_or(OpenError::Locked)?;
            let key = derive_key(password, &v.salt, iterations(&v)).ok_or(OpenError::Vault(VaultError::Malformed))?;
            decrypt_with_key(&v, &key).map_err(OpenError::Vault)?;
            self.keys.insert(k.clone(), Key { salt: v.salt.clone(), iter: iterations(&v), bytes: Zeroizing::new(key) });
        }
        let key = &self.keys[&k];
        decrypt_with_key(&v, &key.bytes).map(Zeroizing::new).map_err(OpenError::Vault)
    }

    /// Seals a new wallet key with the session (sealWithSession): the check vault's key if it
    /// is unlocked, otherwise any.
    pub fn seal(&self, data: &str, check: Option<&Vault>) -> Result<Vault, OpenError> {
        let key = check.and_then(|c| self.keys.get(&id(&c.salt, iterations(c)))).or_else(|| self.keys.values().next()).ok_or(OpenError::Locked)?;
        Ok(encrypt_with_key(data, &key.bytes, &key.salt, key.iter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::LEGACY_ITERATIONS;

    fn wallet(vault: Vault) -> SecureWallet {
        SecureWallet { id: "w".into(), name: "W".into(), address: "0x1".into(), vault: WalletVault::Sealed(vault), chain: Some("EVM".into()) }
    }

    #[test]
    fn first_password_then_unlock_and_open() {
        let done = unlock("correct horse battery", None, &[]).unwrap();
        assert_eq!(decrypt_vault(&done.check, "correct horse battery").unwrap(), CHECK);
        let mut s = WalletSession::default();
        s.start(&done, 15, 1_000);
        assert!(s.unlocked() && !s.expired(1_000) && s.expired(1_000 + 15 * 60_000));

        // A new wallet sealed with the session opens with the same password.
        let sealed = s.seal("0xsecret", Some(&done.check)).unwrap();
        assert_eq!(decrypt_vault(&sealed, "correct horse battery").unwrap(), "0xsecret");
        assert_eq!(s.open(&WalletVault::Sealed(sealed.clone())).unwrap().as_str(), "0xsecret");

        // Wrong password against the check, and against the wallets.
        assert!(unlock("wrong password!", Some(&done.check), &[]).is_err());
        assert!(unlock("another password", None, &[wallet(sealed)]).is_err());
        s.lock();
        assert!(!s.unlocked());
    }

    #[test]
    fn legacy_vaults_are_resealed() {
        // A vault from before the iteration count was recorded.
        let key = derive_key("old password 1", &crate::crypto::b64(&[7u8; 16]), LEGACY_ITERATIONS).unwrap();
        let mut old = encrypt_with_key("0xkey", &key, &crate::crypto::b64(&[7u8; 16]), LEGACY_ITERATIONS);
        old.iter = None;
        let done = unlock("old password 1", None, &[wallet(old)]).unwrap();
        let up = done.upgraded.as_ref().unwrap();
        let v = up[0].vault.parsed().unwrap();
        assert_eq!(v.iter, Some(crate::vault::ITERATIONS));
        assert_eq!(decrypt_vault(&v, "old password 1").unwrap(), "0xkey");
    }
}
