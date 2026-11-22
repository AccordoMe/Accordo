// SPDX-License-Identifier: GPL-3.0-or-later
//! EVM keys (Ethereum, Base, BSC…): `0x` + 64 hex, address checksummed (EIP-55) — what ethers'
//! `Wallet.createRandom()` / `new Wallet(key)` give, so a key moves between web and desktop as is.

use std::str::FromStr;

use alloy_primitives::hex;
use alloy_signer_local::PrivateKeySigner;
use zeroize::Zeroizing;

/// A fresh key: (address, private key). The key string is wiped when dropped.
pub fn generate() -> (String, Zeroizing<String>) {
    let signer = PrivateKeySigner::random();
    let key = Zeroizing::new(hex::encode_prefixed(signer.to_bytes()));
    (signer.address().to_checksum(None), key)
}

/// `0x`-prefixed or bare 64 hex characters, as the web accepts.
fn is_key(key: &str) -> bool {
    let hex = key.strip_prefix("0x").unwrap_or(key);
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The address of a pasted private key, if it is one.
pub fn address_of(key: &str) -> Option<String> {
    let key = key.trim();
    if !is_key(key) {
        return None;
    }
    let signer = PrivateKeySigner::from_str(key.strip_prefix("0x").unwrap_or(key)).ok()?;
    Some(signer.address().to_checksum(None))
}

/// The key as stored: always `0x`-prefixed (ethers keeps it that way).
pub fn normalize_key(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("0x") { key.to_string() } else { format!("0x{key}") }
}

/// A signer from a stored key (for signing — cheques, invoices, transfers; next step).
#[allow(dead_code)]
pub fn signer(key: &str) -> Option<PrivateKeySigner> {
    let key = key.trim();
    PrivateKeySigner::from_str(key.strip_prefix("0x").unwrap_or(key)).ok()
}

#[allow(dead_code)]
pub fn is_address(address: &str) -> bool {
    let a = address.trim();
    a.len() == 42 && a.starts_with("0x") && a[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_key_gives_known_address() {
        // Hardhat / Anvil account #0 — a well-known test key, never holding real funds.
        let key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        assert_eq!(address_of(key).as_deref(), Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"));
        assert_eq!(address_of(&key[2..]).as_deref(), Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"));
        assert!(address_of("0x1234").is_none());
        assert!(address_of(&"0".repeat(64)).is_none()); // zero isn't a valid key
    }

    #[test]
    fn generated_key_round_trips() {
        let (address, key) = generate();
        assert!(key.starts_with("0x") && key.len() == 66);
        assert_eq!(address_of(&key).as_deref(), Some(address.as_str()));
        assert!(is_address(&address));
    }
}
