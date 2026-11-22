// SPDX-License-Identifier: GPL-3.0-or-later
//! Sealing text with the master password (PBKDF2 + AES-GCM) and the sealed-wallet types — now in the shared core, so the phone works the same way. Re-exported here so every
//! `crate::e2ee::vault::…` path keeps working.

pub use accordo_core::vault::*;
