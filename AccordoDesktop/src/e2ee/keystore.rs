// SPDX-License-Identifier: GPL-3.0-or-later
//! Where this device keeps an account's chat key — now in the shared core, so the phone works the same way. Re-exported here so every
//! `crate::e2ee::keystore::…` path keeps working.

pub use accordo_core::keystore::*;
