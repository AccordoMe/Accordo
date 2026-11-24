// SPDX-License-Identifier: GPL-3.0-or-later
//! Wallet unlock for Kotlin: the master password, the sealed keys, and the session that opens them.
//!
//! The web's `unlockWallets` / `walletSession.ts` behaviour, unchanged: PBKDF2-HMAC-SHA256 with
//! 600 000 iterations per vault, AES-256-GCM over each key, keys held in memory only and wiped when
//! the session is locked. The app never sees a key it didn't ask for by name: `reveal` opens one
//! wallet, `seal` wraps a new key with the session's key.
//!
//! Unlocking is slow on purpose — call it as a suspend function, never on the UI thread. Nothing is
//! written to disk by this module: the app stores `check_vault` and re-sealed wallets through its
//! normal account sync.

use std::sync::{Arc, Mutex};

use crate::mobile::MobileError;
use crate::vault::{SecureWallet, Vault, WalletVault};
use crate::wallet_session::{self, WalletSession};

/// What unlocking produced: the session plus what the account has to store afterwards.
#[derive(uniffi::Record)]
pub struct UnlockedWallets {
    /// The open session. Keep it; lock it when the app goes to the background.
    pub session: Arc<WalletUnlock>,
    /// The password-check vault to store (new, or re-sealed on the current iteration count).
    pub check_vault: String,
    /// Wallets re-sealed to 600 000 iterations, as a JSON array — empty when nothing changed.
    pub upgraded: String,
    /// When the session locks by itself (ms since the epoch); None: until the app locks it.
    pub lock_at: Option<i64>,
}

/// An unlocked (or locked) wallet session. `unlocked()` tells which.
#[derive(uniffi::Object)]
pub struct WalletUnlock {
    session: Mutex<WalletSession>,
}

/// Checks the master password and derives the keys: one PBKDF2 pass per distinct vault salt, so
/// this takes a noticeable moment by design — a suspend function on the Kotlin side.
/// `ttl_minutes` 0: stay unlocked until `lock()`.
#[uniffi::export(async_runtime = "tokio")]
pub async fn open_wallet_session(password: String, check_vault: Option<String>, wallets_json: String, ttl_minutes: u32) -> Result<UnlockedWallets, MobileError> {
    {
        let check: Option<Vault> = check_vault.as_deref().map(serde_json::from_str).transpose()?;
        let wallets: Vec<SecureWallet> = serde_json::from_str(&wallets_json)?;
        let done = wallet_session::unlock_async(password, check, wallets)
            .await
            .map_err(|detail| MobileError::Wallet { detail })?;

        let mut session = WalletSession::default();
        session.start(&done, ttl_minutes, chrono::Utc::now().timestamp_millis());
        let lock_at = session.lock_at;
        let answer = UnlockedWallets {
            check_vault: serde_json::to_string(&done.check)?,
            upgraded: serde_json::to_string(&done.upgraded.clone().unwrap_or_default())?,
            lock_at,
            session: Arc::new(WalletUnlock { session: Mutex::new(session) }),
        };
        Ok(answer)
    }
}

#[uniffi::export]
impl WalletUnlock {
    /// A session with no keys: `reveal` will say it's locked. Used after a restart.
    #[uniffi::constructor]
    pub fn locked() -> Arc<Self> {
        Arc::new(WalletUnlock { session: Mutex::new(WalletSession::default()) })
    }

    pub fn unlocked(&self) -> bool {
        self.session.lock().map(|s| s.unlocked()).unwrap_or(false)
    }

    /// Forgets the keys (wiped from memory; the password is never kept).
    pub fn lock(&self) {
        if let Ok(mut session) = self.session.lock() {
            session.lock();
        }
    }

    /// The deadline, for scheduling a re-lock (ms since the epoch).
    pub fn lock_at(&self) -> Option<i64> {
        self.session.lock().ok().and_then(|s| s.lock_at)
    }

    /// Whether the deadline has passed — the app should lock on `true`.
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        self.session.lock().map(|s| s.expired(now)).unwrap_or(false)
    }

    /// The private key of one wallet (hex), given the wallet as the account stores it
    /// (`{"id","name","address","vault":…}`). Prefer signing in Rust when the call lands there.
    pub fn reveal(&self, wallet_json: String) -> Result<String, MobileError> {
        let wallet: SecureWallet = serde_json::from_str(&wallet_json)?;
        let session = self.session.lock().map_err(|_| MobileError::Wallet { detail: "the wallet session is poisoned".into() })?;
        let key = session.open(&wallet.vault).map_err(|e| MobileError::Wallet { detail: e.to_string() })?;
        Ok(key.to_string())
    }

    /// The private key from a bare vault (wallet JSON's `vault` field, sealed or as text).
    pub fn reveal_vault(&self, vault_json: String) -> Result<String, MobileError> {
        let vault: WalletVault = serde_json::from_str(&vault_json)?;
        let session = self.session.lock().map_err(|_| MobileError::Wallet { detail: "the wallet session is poisoned".into() })?;
        let key = session.open(&vault).map_err(|e| MobileError::Wallet { detail: e.to_string() })?;
        Ok(key.to_string())
    }

    /// Seals a new wallet's key with this session (adding a wallet): a vault to store.
    pub fn seal(&self, private_key: String, check_vault: Option<String>) -> Result<String, MobileError> {
        let check: Option<Vault> = check_vault.as_deref().map(serde_json::from_str).transpose()?;
        let session = self.session.lock().map_err(|_| MobileError::Wallet { detail: "the wallet session is poisoned".into() })?;
        let vault = session.seal(&private_key, check.as_ref()).map_err(|e| MobileError::Wallet { detail: e.to_string() })?;
        Ok(serde_json::to_string(&vault)?)
    }

}

/// The password-check vault for a brand-new account (first launch: there is none yet).
/// Slow: 600 000 PBKDF2 iterations — call it as a suspend function.
#[uniffi::export(async_runtime = "tokio")]
pub async fn new_check_vault(password: String) -> Result<String, MobileError> {
    let vault = tokio::task::spawn_blocking(move || crate::vault::encrypt_vault(crate::wallet_session::CHECK, &password))
        .await
        .map_err(|e| MobileError::Wallet { detail: e.to_string() })?;
    Ok(serde_json::to_string(&vault)?)
}

/// The password-check vault this device keeps (a JSON `Vault`), so unlocking doesn't have to make a
/// new one every time — the desktop keeps the same in its store.
const CHECK_KEY: &str = "wallet:check";

#[uniffi::export]
pub fn saved_check_vault() -> Option<String> {
    crate::secrets::get(CHECK_KEY)
}

#[uniffi::export]
pub fn save_check_vault(json: String) -> bool {
    crate::secrets::set(CHECK_KEY, &json).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::encrypt_vault;

    fn wallet(key: &str, password: &str) -> String {
        let sealed = encrypt_vault(key, password);
        serde_json::to_string(&SecureWallet {
            id: "w1".into(),
            name: "Main".into(),
            address: "0xabc".into(),
            vault: WalletVault::Sealed(sealed),
            chain: Some("EVM".into()),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn a_wallet_key_opens_with_the_master_password_and_not_with_another() {
        let check = super::new_check_vault("correct horse".into()).await.unwrap();
        let main = wallet("0xsecret", "correct horse");
        let opened = super::open_wallet_session("correct horse".into(), Some(check.clone()), format!("[{main}]"), 0).await.expect("unlock");
        assert!(opened.session.unlocked());
        assert_eq!(opened.session.reveal(main.clone()).unwrap(), "0xsecret");

        // Sealed since on another device (a new salt): the password isn't kept, so it's asked again.
        assert!(opened.session.reveal(wallet("0xsecret", "correct horse")).is_err());

        let wrong = super::open_wallet_session("wrong".into(), Some(check), format!("[{main}]"), 0).await;
        assert!(wrong.is_err(), "a wrong password must not open the session");
    }

    #[tokio::test]
    async fn a_new_wallet_sealed_in_the_session_opens_later() {
        let check = super::new_check_vault("pw".into()).await.unwrap();
        let opened = super::open_wallet_session("pw".into(), Some(check.clone()), "[]".into(), 30).await.expect("unlock");
        assert!(opened.lock_at.is_some(), "ttl 30 → a deadline");

        let sealed = opened.session.seal("0xfeed".into(), Some(check)).expect("seal");
        assert_eq!(opened.session.reveal_vault(sealed).unwrap(), "0xfeed");

        opened.session.lock();
        assert!(!opened.session.unlocked());
        assert!(opened.session.reveal_vault(serde_json::to_string(&encrypt_vault("0xfeed", "pw")).unwrap()).is_err());
    }
}
