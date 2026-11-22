// SPDX-License-Identifier: GPL-3.0-or-later
//! Trust-on-first-use pins for other users' public keys (utils/e2eeTrust.ts).
//!
//! The server hands out everyone's public key, so without pinning a compromised server could swap
//! in its own key and have honest clients wrap room keys for it. This device remembers the first
//! key it sees for each user and won't wrap for a different one until the user accepts the change
//! (like Signal's safety numbers). Pins are per device and per account.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::crypto::fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTrust {
    New,
    Trusted,
    Verified,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// Fingerprint of the currently trusted key.
    pub fp: String,
    /// Every fingerprint ever accepted (old envelopes stay openable).
    pub history: Vec<String>,
    /// The user compared fingerprints out of band.
    pub verified: bool,
    pub pinned_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    pub fp: String,
    pub seen_at: i64,
}

/// The web's TrustState, same JSON shape (`{pins, changed}`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrustState {
    pub pins: HashMap<String, Pin>,
    pub changed: HashMap<String, PendingChange>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl TrustState {
    /// Checks a user's current key against the pin, pinning it on first sight. `Changed` (and a
    /// recorded pending change) when it differs from the pinned key — then don't wrap for it.
    pub fn check_and_pin(&mut self, user_id: i64, public_jwk: &str) -> KeyTrust {
        let fp = fingerprint(public_jwk);
        let id = user_id.to_string();
        match self.pins.get(&id) {
            None => {
                self.pins.insert(id, Pin { fp: fp.clone(), history: vec![fp], verified: false, pinned_at: now() });
                KeyTrust::New
            }
            Some(pin) if pin.fp == fp => {
                self.changed.remove(&id);
                if pin.verified { KeyTrust::Verified } else { KeyTrust::Trusted }
            }
            Some(_) => {
                if self.changed.get(&id).map(|c| &c.fp) != Some(&fp) {
                    self.changed.insert(id, PendingChange { fp, seen_at: now() });
                }
                KeyTrust::Changed
            }
        }
    }

    /// For opening an existing envelope: any key this user was ever trusted with is fine (it may
    /// predate a legitimate change). Unknown users are pinned on first sight.
    pub fn is_known_key(&mut self, user_id: i64, public_jwk: &str) -> bool {
        let fp = fingerprint(public_jwk);
        let id = user_id.to_string();
        let Some(pin) = self.pins.get(&id) else {
            self.check_and_pin(user_id, public_jwk);
            return true;
        };
        if pin.history.contains(&fp) {
            return true;
        }
        if self.changed.get(&id).map(|c| &c.fp) != Some(&fp) {
            self.changed.insert(id, PendingChange { fp, seen_at: now() });
        }
        false
    }

    /// The user reviewed a key change and accepts the new key (no longer "verified": a new key).
    pub fn accept_change(&mut self, user_id: i64) {
        let id = user_id.to_string();
        let Some(change) = self.changed.remove(&id) else { return };
        let mut history = self.pins.get(&id).map(|p| p.history.clone()).unwrap_or_default();
        if !history.contains(&change.fp) {
            history.push(change.fp.clone());
        }
        if history.len() > 10 {
            history.drain(..history.len() - 10);
        }
        self.pins.insert(id, Pin { fp: change.fp, history, verified: false, pinned_at: now() });
    }

    pub fn set_verified(&mut self, user_id: i64, verified: bool) {
        if let Some(pin) = self.pins.get_mut(&user_id.to_string()) {
            pin.verified = verified;
        }
    }
}
