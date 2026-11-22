// SPDX-License-Identifier: GPL-3.0-or-later
//! The part of Accordo a phone needs without any UI toolkit: the end-to-end encryption primitives
//! (byte-compatible with the web's WebCrypto) behind a Kotlin-friendly API.
//!
//! `crypto.rs` is the desktop client's `src/e2ee/crypto.rs`, unchanged — it has no UI dependencies
//! at all (aes-gcm, p256, sha2, serde). Everything above it (rooms, envelopes, the vault) builds on
//! these functions, so this is the slice worth shipping to Android first; the API client comes next.
//!
//! Bindings:
//!   cargo build
//!   cargo run --bin uniffi-bindgen -- generate --library target/debug/libaccordo_core.so \
//!       --language kotlin --out-dir ../kotlin-out

uniffi::setup_scaffolding!();

pub mod api;
pub mod crypto;
pub mod keystore;
pub mod log;
pub mod mobile;
pub mod rooms;
pub mod secrets;
pub mod trust;
pub mod vault;
pub mod wallet_session;

use std::sync::Arc;

/// What a decrypted payload turned out to be (`Opened` in the desktop client, flattened for Kotlin).
#[derive(uniffi::Enum)]
pub enum Decrypted {
    /// Ciphertext, opened with one of the keys we were given.
    Text(String),
    /// Ciphertext none of the keys opens (the room's key isn't on this device yet).
    Locked,
    /// Not sealed at all — public rooms and old messages.
    Plain(String),
}

impl From<crypto::Opened> for Decrypted {
    fn from(opened: crypto::Opened) -> Self {
        match opened {
            crypto::Opened::Text(text) => Decrypted::Text(text),
            crypto::Opened::Plain(text) => Decrypted::Plain(text),
            crypto::Opened::Locked => Decrypted::Locked,
        }
    }
}

/// A device's long-term chat key (ECDH P-256): it opens the envelopes that carry room keys.
#[derive(uniffi::Object)]
pub struct DeviceKey {
    inner: crypto::DeviceKeypair,
}

#[uniffi::export]
impl DeviceKey {
    /// A new key pair. The private half never leaves Rust; Kotlin can only ask for the public JWK.
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self { inner: crypto::DeviceKeypair::generate() })
    }

    /// The public half, as the JSON the server stores and other clients pin.
    pub fn public_jwk(&self) -> String {
        self.inner.public_jwk.clone()
    }

    /// The private half, as JWK — for the device's own key backup only.
    pub fn private_jwk(&self) -> String {
        self.inner.private_jwk.clone()
    }

    /// Does the private half really belong to the public one? (Run it after restoring a backup.)
    pub fn self_test(&self) -> bool {
        self.inner.self_test()
    }

    /// Wrap a room key for one member: `{"iv","data"}`, sealed with our key and their public key.
    pub fn create_envelope(&self, room_key: String, recipient_public_jwk: String) -> Option<String> {
        self.inner.create_envelope(&room_key, &recipient_public_jwk)
    }

    /// Open an envelope someone wrapped for us, using their public key.
    pub fn open_envelope(&self, envelope: String, wrapper_public_jwk: String) -> Option<String> {
        self.inner.open_envelope(&envelope, &wrapper_public_jwk)
    }
}

/// Rebuild a device key from a stored public/private JWK pair (restoring a backup).
#[uniffi::export]
pub fn device_key_from_jwk(public_jwk: String, private_jwk: String) -> Option<Arc<DeviceKey>> {
    crypto::DeviceKeypair::from_jwk(&public_jwk, &private_jwk).map(|inner| Arc::new(DeviceKey { inner }))
}

impl DeviceKey {
    /// Wrapping a key pair the core itself loaded (the keystore) — not exported to Kotlin.
    pub(crate) fn from_pair(pair: crypto::DeviceKeypair) -> Arc<Self> {
        Arc::new(Self { inner: pair })
    }

    /// The raw pair, for the keystore to store.
    pub(crate) fn pair(&self) -> &crypto::DeviceKeypair {
        &self.inner
    }
}

/// A new random room key (base64url): the symmetric key a chat's messages are sealed with.
#[uniffi::export]
pub fn generate_room_key() -> String {
    crypto::generate_room_key()
}

/// Seal text for a room — AES-256-GCM with a random nonce, the same bytes the web app produces.
#[uniffi::export]
pub fn encrypt_text(plaintext: String, room_key: String) -> Option<String> {
    crypto::encrypt_symmetric(&plaintext, &room_key)
}

/// Open a sealed payload with the room's keys (current first, archived after it).
#[uniffi::export]
pub fn decrypt_text(payload: String, keys: Vec<String>) -> Decrypted {
    crypto::decrypt_symmetric(&payload, &keys).into()
}

/// Whether a string looks like sealed content — a cheap check that needs no keys.
#[uniffi::export]
pub fn looks_encrypted(text: String) -> bool {
    crypto::looks_encrypted(&text)
}

/// A stable fingerprint of a public key ("this is really @alina").
#[uniffi::export]
pub fn fingerprint_of(public_jwk: String) -> String {
    crypto::fingerprint(&public_jwk)
}
