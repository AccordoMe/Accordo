// SPDX-License-Identifier: GPL-3.0-or-later
//! Room keys are resolved in the shared core (`accordo_core::rooms`), so the phone opens the same
//! envelopes with the same trust rules. Re-exported here so `crate::e2ee::rooms::…` keeps working.

pub use accordo_core::rooms::*;
