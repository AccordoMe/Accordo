// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end encryption, byte-compatible with the web app (utils/e2ee.ts, vault.ts,
//! e2eeTrust.ts, secretPhrase.ts) — the same keys, envelopes and ciphertext work in both:
//!
//! - device key: ECDH P-256, exchanged as JWK;
//! - room key: 32 random bytes, base64; wrapped per member with ECDH (the raw shared secret is
//!   the AES-256-GCM key — WebCrypto's deriveKey applies no KDF);
//! - messages: AES-256-GCM, `{"iv","data","k"}` (k = which room key);
//! - key backup: `{priv, pub}` sealed with the chat password (PBKDF2-SHA256 + AES-GCM);
//! - trust: the first key seen for a user is pinned (TOFU).
//!
//! Ported ahead of the screens that will use it, hence the allow below.

#![allow(dead_code)]

pub mod account;
pub mod crypto;
pub mod keystore;
pub mod phrase;
pub mod rooms;
pub mod trust;
pub mod vault;
pub mod wallet_session;

#[cfg(test)]
mod tests;
