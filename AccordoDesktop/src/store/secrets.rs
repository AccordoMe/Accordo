// SPDX-License-Identifier: GPL-3.0-or-later
//! The desktop's secret store: the OS credential store, via the `keyring` crate.
//!
//! The core only knows the `accordo_core::secrets::SecretStore` trait; this is the Linux / Windows /
//! macOS implementation it installs at startup (Secret Service — GNOME Keyring or KWallet;
//! Credential Manager; Keychain). One entry per key, small (Windows takes ~2.5 KB each), and
//! losing one never touches the others.

use std::sync::Arc;

/// The service name entries are filed under in the OS store.
const SERVICE: &str = "accordo-desktop";

struct CredentialStore;

impl accordo_core::secrets::SecretStore for CredentialStore {
    fn get(&self, key: &str) -> Option<String> {
        let entry = match keyring::Entry::new(SERVICE, key) {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("accordo: no credential store: {e}");
                return None;
            }
        };
        match entry.get_password() {
            Ok(value) => Some(value),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                eprintln!("accordo: can't read from the credential store: {e}");
                None
            }
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, key)
            .map_err(|e| format!("no credential store: {e}"))?
            .set_password(value)
            .map_err(|e| e.to_string())
    }

    fn remove(&self, key: &str) {
        match keyring::Entry::new(SERVICE, key).map(|e| e.delete_credential()) {
            Ok(Ok(())) | Ok(Err(keyring::Error::NoEntry)) => {}
            Ok(Err(e)) | Err(e) => eprintln!("accordo: can't remove from the credential store: {e}"),
        }
    }
}

/// Connects the core to the OS store. Call once, before the first session is read.
pub fn install() {
    accordo_core::secrets::install(Arc::new(CredentialStore));
}

#[cfg(test)]
mod tests {
    /// Touches the real OS store (own throwaway entry): `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn credential_store_round_trip() {
        let entry = keyring::Entry::new("accordo-desktop-selftest", "probe").expect("credential store");
        entry.set_password("{\"base\":\"x\",\"token\":\"y\"}").expect("write");
        assert_eq!(entry.get_password().expect("read"), "{\"base\":\"x\",\"token\":\"y\"}");
        entry.delete_credential().expect("delete");
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
