// SPDX-License-Identifier: GPL-3.0-or-later
//! The E2EE primitives now live in the shared core (`accordo_core::crypto`), so the desktop
//! client and the Android app encrypt with literally the same code. Re-exported here so every
//! `crate::e2ee::crypto::…` path keeps working.
//!
//! Byte-compatible with the web app's WebCrypto: AES-256-GCM payloads, base64url envelopes,
//! ECDH P-256 device keys.

pub use accordo_core::crypto::*;
