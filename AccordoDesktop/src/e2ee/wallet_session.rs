// SPDX-License-Identifier: GPL-3.0-or-later
//! The wallet unlock session (derived keys held in memory) — now in the shared core, so the phone works the same way. Re-exported here so every
//! `crate::e2ee::wallet_session::…` path keeps working.

pub use accordo_core::wallet_session::*;
