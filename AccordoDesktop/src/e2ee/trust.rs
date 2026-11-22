// SPDX-License-Identifier: GPL-3.0-or-later
//! Trust-on-first-use pins for other users' public keys — now in the shared core, so the phone works the same way. Re-exported here so every
//! `crate::e2ee::trust::…` path keeps working.

pub use accordo_core::trust::*;
