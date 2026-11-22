// SPDX-License-Identifier: GPL-3.0-or-later
//! Sealing text with a password (utils/vault.ts): PBKDF2-HMAC-SHA256 → AES-256-GCM, fields in
//! base64. Wallet keys and the chat-key backup both use it.

#[cfg(test)]
use pbkdf2::pbkdf2_hmac;
#[cfg(test)]
use sha2::Sha256;

use crate::crypto::{b64, open, random_bytes, seal, unb64};

use serde::{Deserialize, Serialize};

/// A wallet's private key, sealed with the master password (utils/vault.ts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub salt: String,
    pub iv: String,
    pub cipher: String,
    /// PBKDF2 iterations it was sealed with; absent on old vaults (100 000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iter: Option<u32>,
}

/// Wallets saved by older versions may hold the vault as a JSON string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WalletVault {
    Sealed(Vault),
    Text(String),
}

impl WalletVault {
    /// The vault, however it was saved (None when it's damaged or empty).
    pub fn parsed(&self) -> Option<Vault> {
        let v = match self {
            WalletVault::Sealed(v) => v.clone(),
            WalletVault::Text(s) => serde_json::from_str::<Vault>(s).ok()?,
        };
        (!v.salt.is_empty() && !v.iv.is_empty() && !v.cipher.is_empty()).then_some(v)
    }

    /// The same text for the same vault either way (to spot duplicate wallets).
    pub fn canonical(&self) -> String {
        match self {
            WalletVault::Text(s) => serde_json::from_str::<Vault>(s).ok().and_then(|v| serde_json::to_string(&v).ok()).unwrap_or_else(|| s.clone()),
            WalletVault::Sealed(v) => serde_json::to_string(v).unwrap_or_default(),
        }
    }
}

/// What the account keeps about one wallet on the device: the address openly, the key sealed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecureWallet {
    pub id: String,
    pub name: String,
    pub address: String,
    pub vault: WalletVault,
    /// "EVM" | "Solana" | "TON" | "Tron" | "Unknown"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
}


/// OWASP's current recommendation for PBKDF2-HMAC-SHA256 — vaults live on the server, so a
/// leak allows offline guessing and this is what makes each guess expensive.
pub const ITERATIONS: u32 = 600_000;
/// Vaults made before the count was recorded.
pub const LEGACY_ITERATIONS: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// The password (or the data) doesn't match this vault — AES-GCM can't tell which.
    WrongPassword,
    Malformed,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            VaultError::WrongPassword => "Wrong password",
            VaultError::Malformed => "This sealed data is damaged",
        })
    }
}

/// PBKDF2-HMAC-SHA256, as WebCrypto's deriveKey (ring: same bytes, ~25× faster in debug builds).
fn derive(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    let n = std::num::NonZeroU32::new(iterations.max(1)).expect("at least one iteration");
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, n, salt, password.as_bytes(), &mut key);
    key
}

pub fn iterations(vault: &Vault) -> u32 {
    vault.iter.unwrap_or(LEGACY_ITERATIONS)
}

/// Seals `data` (encryptVault). Slow on purpose: ~600k hashes.
pub fn encrypt_vault(data: &str, password: &str) -> Vault {
    let salt = random_bytes::<16>();
    let key = derive(password, &salt, ITERATIONS);
    let (iv, cipher) = seal(&key, data.as_bytes()).expect("AES key of the right size");
    Vault { salt: b64(&salt), iv, cipher, iter: Some(ITERATIONS) }
}

pub fn decrypt_vault(vault: &Vault, password: &str) -> Result<String, VaultError> {
    let salt = unb64(&vault.salt).ok_or(VaultError::Malformed)?;
    let key = derive(password, &salt, iterations(vault));
    let plain = open(&key, &vault.iv, &vault.cipher).ok_or(VaultError::WrongPassword)?;
    String::from_utf8(plain).map_err(|_| VaultError::Malformed)
}

/// Older vaults (fewer iterations) are re-sealed on the next unlock.
pub fn needs_upgrade(vault: &Vault) -> bool {
    iterations(vault) < ITERATIONS
}

// ── With an already-derived key (the wallet unlock session, see wallet_session.rs) ──

/// The AES key that opens vaults with this salt and iteration count.
pub fn derive_key(password: &str, salt_b64: &str, iterations: u32) -> Option<[u8; 32]> {
    Some(derive(password, &unb64(salt_b64)?, iterations))
}

pub fn decrypt_with_key(vault: &Vault, key: &[u8; 32]) -> Result<String, VaultError> {
    let plain = open(key, &vault.iv, &vault.cipher).ok_or(VaultError::WrongPassword)?;
    String::from_utf8(plain).map_err(|_| VaultError::Malformed)
}

/// Seals with a derived key; the vault records its salt and count, so the master password
/// opens it later like one made by `encrypt_vault`. A fresh IV every time.
pub fn encrypt_with_key(data: &str, key: &[u8; 32], salt_b64: &str, iterations: u32) -> Vault {
    let (iv, cipher) = seal(key, data.as_bytes()).expect("AES key of the right size");
    Vault { salt: salt_b64.to_string(), iv, cipher, iter: Some(iterations) }
}

/// A vault worth trying (all three fields present).
pub fn usable(vault: &Vault) -> bool {
    !vault.salt.is_empty() && !vault.iv.is_empty() && !vault.cipher.is_empty()
}

/// ring gives the very bytes the pbkdf2 crate (and WebCrypto) give: old vaults still open.
#[cfg(test)]
#[test]
fn derive_matches_pbkdf2() {
    for (pw, salt, iter) in [("password", &[1u8; 16][..], 1u32), ("пароль ключ", &[7u8; 16][..], 1_000), ("", &[0u8; 8][..], 2)] {
        let mut want = [0u8; 32];
        pbkdf2_hmac::<Sha256>(pw.as_bytes(), salt, iter, &mut want);
        assert_eq!(derive(pw, salt, iter), want);
    }
}

#[cfg(test)]
#[test]
#[ignore]
fn bench_derive() {
    let t = std::time::Instant::now();
    let _ = derive("password", &[1u8; 16], ITERATIONS);
    eprintln!("one 600k derivation: {:?}", t.elapsed());
}
