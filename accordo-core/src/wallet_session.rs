// SPDX-License-Identifier: GPL-3.0-or-later
//! The wallet unlock session (utils/walletSession.ts + vaultUpgrade.ts).
//!
//! Unlocking derives the AES key of every vault (each has its own salt) from the master password
//! and keeps those keys in this process's memory: each on a page of its own, locked in RAM (never
//! swapped out, left out of core dumps) and wiped when dropped (`LockedKey`). The master password
//! itself is not kept: a wallet added on another device meanwhile (a new salt) needs it typed again
//! (`OpenError::NeedsPassword`). At the lock deadline, on "Lock now" and on sign-out everything is
//! dropped.
//!
//! With a lock timer the derived keys are also kept in the host's secret store until the deadline
//! (`persisted` → `save_persisted`; `load_persisted` → `resume`), like the web keeps them in
//! IndexedDB: restarting the app inside the timer doesn't lock the wallets. "Until restart" stays
//! in memory only.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::vault::{decrypt_vault, decrypt_with_key, derive_key, encrypt_vault, encrypt_with_key, iterations, needs_upgrade, usable, SecureWallet, Vault, VaultError, WalletVault};

/// What the master-password check vault holds.
/// What the master-password check vault holds (the web writes the same string).
pub const CHECK: &str = "VALID_PASSWORD";

fn id(salt: &str, iter: u32) -> String {
    format!("{salt}:{iter}")
}

#[derive(Clone)]
struct Key {
    salt: String,
    iter: u32,
    bytes: LockedKey,
}

/// A memory page: the unit RAM is locked in (a larger real page is locked whole).
const PAGE: usize = 4096;

/// 32 key bytes alone on a page of their own, locked in RAM — never written to swap — and left out
/// of core dumps (Unix; best effort: if the system refuses the lock, the key still works and is
/// still wiped). Wiped, unlocked and freed when dropped.
struct LockedKey(std::ptr::NonNull<[u8; 32]>);

// SAFETY: the page belongs to this value alone; nothing else points into it.
unsafe impl Send for LockedKey {}
unsafe impl Sync for LockedKey {}

impl LockedKey {
    fn layout() -> std::alloc::Layout {
        std::alloc::Layout::from_size_align(PAGE, PAGE).expect("a page is a valid layout")
    }

    fn new(bytes: &[u8; 32]) -> Self {
        let layout = Self::layout();
        // SAFETY: the layout's size isn't zero; a null result is handled below.
        let page = unsafe { std::alloc::alloc_zeroed(layout) };
        let Some(page) = std::ptr::NonNull::new(page) else { std::alloc::handle_alloc_error(layout) };
        #[cfg(unix)]
        // SAFETY: `page` is a live allocation of PAGE bytes, aligned to PAGE.
        unsafe {
            if libc::mlock(page.as_ptr().cast(), PAGE) != 0 {
                tracing::debug!("wallet keys: the system didn't lock a key's page in RAM");
            }
            #[cfg(any(target_os = "linux", target_os = "android"))]
            libc::madvise(page.as_ptr().cast(), PAGE, libc::MADV_DONTDUMP);
        }
        let key = page.cast::<[u8; 32]>();
        // SAFETY: the page holds at least 32 bytes, and [u8; 32] needs no alignment.
        unsafe { key.as_ptr().write(*bytes) };
        Self(key)
    }

    fn get(&self) -> &[u8; 32] {
        // SAFETY: written in `new`, alive until `drop`.
        unsafe { self.0.as_ref() }
    }
}

impl Clone for LockedKey {
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        // SAFETY: the page is ours since `new` and freed with the layout it was allocated with;
        // the key is wiped before the page goes back.
        unsafe {
            self.0.as_mut().zeroize();
            #[cfg(unix)]
            libc::munlock(self.0.as_ptr().cast(), PAGE);
            std::alloc::dealloc(self.0.as_ptr().cast(), Self::layout());
        }
    }
}

/// The outcome of `unlock`, applied by the app. Its Debug never shows key material.
pub struct Unlocked {
    keys: Vec<Key>,
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
                keys.push(Key { salt: v.salt.clone(), iter, bytes: LockedKey::new(&key) });
            }
        }
    }

    Ok(Unlocked { keys, check, upgraded: changed.then_some(wallets) })
}

/// Off the UI thread.
pub async fn unlock_async(password: String, check: Option<Vault>, wallets: Vec<SecureWallet>) -> Result<Arc<Unlocked>, String> {
    let password = Zeroizing::new(password);
    tokio::task::spawn_blocking(move || unlock(&password, check.as_ref(), &wallets).map(Arc::new)).await.map_err(|e| e.to_string())?
}

#[derive(Default)]
pub struct WalletSession {
    keys: HashMap<String, Key>,
    /// When it locks again (ms since the epoch); None: until restart.
    pub lock_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// Locked: unlock with the master password first.
    Locked,
    /// Unlocked, but this wallet came after (sealed on another device, a new salt): the password
    /// isn't kept, so unlocking again with it is the way to its key.
    NeedsPassword,
    Vault(VaultError),
    NoKey,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OpenError::Locked => "Your wallets are locked — unlock them with your master password",
            OpenError::NeedsPassword => "This wallet was added on another device since you unlocked — unlock again with your master password to use it",
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
        self.keys = done.keys.iter().map(|k| (id(&k.salt, k.iter), k.clone())).collect();
        self.lock_at = (ttl_minutes > 0).then(|| now_ms + i64::from(ttl_minutes) * 60_000);
    }

    /// Forgets the keys (wiped from memory).
    pub fn lock(&mut self) {
        self.keys.clear();
        self.lock_at = None;
    }

    /// The deadline passed?
    pub fn expired(&self, now_ms: i64) -> bool {
        self.lock_at.is_some_and(|t| now_ms >= t)
    }

    /// Opens a wallet's vault (decryptWalletVault) with the key unlocked for its salt. A wallet
    /// added on another device meanwhile (a new salt) has none: `NeedsPassword`. Zeroize the
    /// result after use.
    pub fn open(&self, vault: &WalletVault) -> Result<Zeroizing<String>, OpenError> {
        let v = vault.parsed().ok_or(OpenError::NoKey)?;
        let Some(key) = self.keys.get(&id(&v.salt, iterations(&v))) else {
            return Err(if self.unlocked() { OpenError::NeedsPassword } else { OpenError::Locked });
        };
        decrypt_with_key(&v, key.bytes.get()).map(Zeroizing::new).map_err(OpenError::Vault)
    }

    /// Seals a new wallet key with the session (sealWithSession): the check vault's key if it
    /// is unlocked, otherwise any.
    pub fn seal(&self, data: &str, check: Option<&Vault>) -> Result<Vault, OpenError> {
        let key = check.and_then(|c| self.keys.get(&id(&c.salt, iterations(c)))).or_else(|| self.keys.values().next()).ok_or(OpenError::Locked)?;
        Ok(encrypt_with_key(data, key.bytes.get(), &key.salt, key.iter))
    }

    /// The unlock as the secret store keeps it between launches: the derived keys and the
    /// deadline (the password isn't kept anywhere). None when locked or "until restart" (memory
    /// only).
    pub fn persisted(&self) -> Option<Zeroizing<String>> {
        let lock_at = self.lock_at.filter(|_| self.unlocked())?;
        let keys = self.keys.values().map(|k| StoredKey { salt: k.salt.clone(), iter: k.iter, key: crate::crypto::b64(k.bytes.get()) }).collect();
        serde_json::to_string(&Stored { v: 1, lock_at, keys }).ok().map(Zeroizing::new)
    }

    /// Takes the keys of an unlock kept from an earlier launch (`load_persisted`).
    pub fn resume(&mut self, r: &Restored) {
        self.keys = r.keys.iter().map(|k| (id(&k.salt, k.iter), k.clone())).collect();
        self.lock_at = Some(r.lock_at);
    }
}

// ── Between launches ──

/// The longest lock timer (24 h) and a minute of clock slack: a deadline further away than that
/// wasn't written by `persisted` on this clock.
const MAX_KEEP_MS: i64 = 24 * 60 * 60_000 + 60_000;

/// The secret-store entry of an account's unlock (`account`: the app's account key, like the
/// chat key's `e2ee:<account>`).
fn entry(account: &str) -> String {
    format!("wallet-session:{account}")
}

#[derive(Serialize, Deserialize)]
struct StoredKey {
    salt: String,
    iter: u32,
    key: String,
}

impl Drop for StoredKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct Stored {
    v: u8,
    lock_at: i64,
    keys: Vec<StoredKey>,
}

/// An unlock read back from the secret store, for `WalletSession::resume`. Its Debug never shows
/// key material.
pub struct Restored {
    keys: Vec<Key>,
    /// When it locks (ms since the epoch).
    pub lock_at: i64,
}

impl std::fmt::Debug for Restored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Restored {{ keys: {}, lock_at: {} }}", self.keys.len(), self.lock_at)
    }
}

/// Keeps `json` (from `WalletSession::persisted`) for the account. Blocking: off the UI thread.
pub fn save_persisted(account: &str, json: &str) -> Result<(), String> {
    crate::secrets::set(&entry(account), json)
}

/// The account's kept unlock, if its deadline is still ahead. An entry that ran out or can't be
/// read is removed. Blocking: off the UI thread.
pub fn load_persisted(account: &str, now_ms: i64) -> Option<Restored> {
    let json = Zeroizing::new(crate::secrets::get(&entry(account))?);
    let restored = serde_json::from_str::<Stored>(&json).ok().filter(|s| s.v == 1 && s.lock_at > now_ms && s.lock_at - now_ms <= MAX_KEEP_MS).and_then(|s| {
        let keys = s
            .keys
            .iter()
            .map(|k| {
                let raw = Zeroizing::new(crate::crypto::unb64(&k.key)?);
                let bytes: &[u8; 32] = raw.as_slice().try_into().ok()?;
                Some(Key { salt: k.salt.clone(), iter: k.iter, bytes: LockedKey::new(bytes) })
            })
            .collect::<Option<Vec<Key>>>()
            .filter(|keys| !keys.is_empty())?;
        Some(Restored { keys, lock_at: s.lock_at })
    });
    if restored.is_none() {
        forget_persisted(account);
    }
    restored
}

/// Removes the account's kept unlock (nothing kept: nothing happens). Blocking: off the UI thread.
pub fn forget_persisted(account: &str) {
    crate::secrets::remove(&entry(account));
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

        // A wallet sealed on another device since (a new salt): the password isn't kept, so it
        // asks for it — and unlocking again takes that wallet in.
        let elsewhere = encrypt_vault("0xother", "correct horse battery");
        assert!(matches!(s.open(&WalletVault::Sealed(elsewhere.clone())), Err(OpenError::NeedsPassword)));
        let again = unlock("correct horse battery", Some(&done.check), &[wallet(elsewhere.clone())]).unwrap();
        s.start(&again, 15, 1_000);
        assert_eq!(s.open(&WalletVault::Sealed(elsewhere.clone())).unwrap().as_str(), "0xother");

        s.lock();
        assert!(!s.unlocked());
        assert!(matches!(s.open(&WalletVault::Sealed(elsewhere)), Err(OpenError::Locked)));
    }

    #[test]
    fn a_locked_key_is_a_copy_on_its_own_page() {
        let k = LockedKey::new(&[7u8; 32]);
        let c = k.clone();
        assert_eq!(c.get(), &[7u8; 32]);
        assert_ne!(k.0, c.0, "each copy has its own page");
        assert_eq!(k.0.as_ptr() as usize % PAGE, 0, "page-aligned");
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

    #[derive(Default)]
    struct Memory(std::sync::Mutex<HashMap<String, String>>);

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
    fn a_timed_unlock_survives_a_restart_until_its_deadline() {
        let _guard = crate::secrets::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::secrets::install(Arc::new(Memory::default()));
        let account = "1:dazay@https://accordo.me";

        let done = unlock("correct horse battery", None, &[]).unwrap();
        let mut s = WalletSession::default();
        s.start(&done, 15, 1_000);
        let sealed = s.seal("0xsecret", Some(&done.check)).unwrap();
        let json = s.persisted().expect("a timed unlock is kept");
        assert!(!json.contains("correct horse battery"), "the password is never kept");
        save_persisted(account, &json).unwrap();

        // "Restart": a fresh session takes the kept keys and opens the wallet without the password.
        let r = load_persisted(account, 2_000).expect("inside the timer");
        assert_eq!(r.lock_at, 1_000 + 15 * 60_000);
        assert!(!format!("{r:?}").contains(&crate::crypto::b64(r.keys[0].bytes.get())), "Debug hides keys");
        let mut again = WalletSession::default();
        again.resume(&r);
        assert!(again.unlocked());
        assert_eq!(again.open(&WalletVault::Sealed(sealed)).unwrap().as_str(), "0xsecret");
        // A wallet under a salt it has no key for: the password isn't kept, so it asks for it.
        let other = encrypt_vault("0xother", "correct horse battery");
        assert!(matches!(again.open(&WalletVault::Sealed(other)), Err(OpenError::NeedsPassword)));

        // At the deadline it's gone, and the entry with it.
        assert!(load_persisted(account, 1_000 + 15 * 60_000).is_none());
        assert!(crate::secrets::get(&entry(account)).is_none());

        // "Until restart" keeps nothing; a damaged entry is removed; forget removes.
        s.start(&done, 0, 1_000);
        assert!(s.persisted().is_none());
        crate::secrets::set(&entry(account), "{not json").unwrap();
        assert!(load_persisted(account, 2_000).is_none());
        assert!(crate::secrets::get(&entry(account)).is_none());
        s.start(&done, 60, 1_000);
        save_persisted(account, &s.persisted().unwrap()).unwrap();
        // A deadline further than the longest timer (a clock set back) isn't trusted.
        assert!(load_persisted(account, 1_000 - MAX_KEEP_MS).is_none());
        save_persisted(account, &s.persisted().unwrap()).unwrap();
        forget_persisted(account);
        assert!(load_persisted(account, 2_000).is_none());
    }
}
