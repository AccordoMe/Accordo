// SPDX-License-Identifier: GPL-3.0-or-later
//! Room keys on this computer (resolveRoomKey, wrapRoomKeyForMembers, the message decrypt/encrypt
//! paths of utils/e2ee.ts + useChatMessages.ts, computeOwnerTag of e2eeMembership.ts).
//!
//! A DM's or group's messages are sealed with the room's key; each member gets a copy of that key
//! wrapped for their public key (an envelope). Ours is opened with the device key — only if the
//! one who wrapped it is someone this device trusts for that account (TOFU pins), otherwise a
//! server could plant a key it knows and read everything. No envelope we can open = no key: never
//! a fallback anyone could compute.

use std::collections::HashMap;

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::crypto::{self, b64, decrypt_json, decrypt_symmetric, encrypt_json, encrypt_symmetric, fingerprint, same_public_key, unb64, DeviceKeypair, Opened};
use crate::keystore;
use crate::trust::{KeyTrust, TrustState};
use crate::api::chat::{ChatMessageItem, ChatRoomItem, KeyEnvelope, KeyEnvelopes, UserInfo};

/// Why a room's key isn't available here (diagnoseRoomKey), so the chat can say what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyProblem {
    /// The chat key isn't on this computer (restore it in Settings).
    NoDeviceKey,
    /// Nobody has shared this room's key with us yet.
    NoEnvelope,
    /// Shared by someone whose key this device doesn't trust (their key changed).
    UntrustedSharer(i64),
    /// We don't know the sharer's public key (they're not in the room any more): fetch it.
    UnknownSharer(i64),
    /// The envelope doesn't open with this computer's key.
    WrongDeviceKey,
}

impl KeyProblem {
    pub fn explain(&self) -> &'static str {
        match self {
            KeyProblem::NoDeviceKey => "Your chat key isn't on this computer — restore it in Settings → Chat encryption to read and write here.",
            KeyProblem::NoEnvelope => "Nobody has shared this chat's key with you yet. It arrives when another member opens the chat.",
            KeyProblem::UntrustedSharer(_) => "The key for this chat was shared by someone whose chat key changed. Check it with them in the web app (Room Security).",
            KeyProblem::UnknownSharer(_) => "Looking up who shared this chat's key…",
            KeyProblem::WrongDeviceKey => "This chat's key was shared with a different chat key than the one on this computer.",
        }
    }
}

struct Cached {
    /// Our envelope it came from: when it changes (the key was rotated) the cache is stale.
    envelope: String,
    archive: String,
    /// The current key first, then archived ones.
    keys: Vec<String>,
}

/// This session's room keys. Never written to disk; dropped with the session or a key change.
#[derive(Default)]
pub struct RoomKeys {
    user_id: i64,
    /// id@server: whose pins (keystore).
    account: String,
    keypair: Option<DeviceKeypair>,
    pins: TrustState,
    cache: HashMap<String, Cached>,
    /// Public keys looked up for sharers who aren't in a room's member list.
    known_keys: HashMap<i64, String>,
}

impl RoomKeys {
    pub fn new(user_id: i64, account: String) -> Self {
        let pins = if account.is_empty() { TrustState::default() } else { keystore::load_pins(&account) };
        Self { user_id, account, pins, ..Default::default() }
    }

    /// The pins again from disk (the old shared file may have just moved to this account).
    pub fn reload_pins(&mut self) {
        if !self.account.is_empty() {
            self.pins = keystore::load_pins(&self.account);
        }
    }

    /// The device key changed (restored, imported, gone): every cached room key is suspect.
    pub fn set_keypair(&mut self, keypair: Option<DeviceKeypair>) {
        self.keypair = keypair;
        self.cache.clear();
    }

    pub fn has_device_key(&self) -> bool {
        self.keypair.is_some()
    }

    pub fn remember_public_key(&mut self, user_id: i64, public_jwk: String) {
        self.known_keys.insert(user_id, public_jwk);
    }

    fn save_pins(&self) {
        if self.account.is_empty() {
            return; // no account (tests)
        }
        if let Err(e) = keystore::save_pins(&self.account, &self.pins) {
            tracing::warn!("e2ee: couldn't save key pins: {e}");
        }
    }

    /// The room's keys (current first, then archived), or why there are none.
    pub fn resolve(&mut self, room: &ChatRoomItem) -> Result<Vec<String>, KeyProblem> {
        let Some(keypair) = self.keypair.as_ref() else { return Err(KeyProblem::NoDeviceKey) };
        let mine = room.key_envelopes.as_ref().and_then(|e| e.get(&self.user_id.to_string())).filter(|e| !e.envelope.is_empty());
        let Some(mine) = mine else { return Err(KeyProblem::NoEnvelope) };
        let archive = room.e2ee_meta.as_ref().and_then(|m| m.archived_keys.clone()).unwrap_or_default();
        if let Some(c) = self.cache.get(&room.room_key) {
            if c.envelope == mine.envelope && c.archive == archive {
                return Ok(c.keys.clone());
            }
        }

        // Whose key wrapped it: as recorded at wrap time, else their current one.
        let wrapper_key = mine
            .wrapper_public_key
            .clone()
            .or_else(|| (mine.wrapped_by == self.user_id).then(|| keypair.public_jwk.clone()))
            .or_else(|| room.allowed_users.iter().find(|u| u.id == mine.wrapped_by).and_then(|u| u.e2ee_public_key.clone()))
            .or_else(|| self.known_keys.get(&mine.wrapped_by).cloned());
        let Some(wrapper_key) = wrapper_key else { return Err(KeyProblem::UnknownSharer(mine.wrapped_by)) };

        let trusted = if mine.wrapped_by == self.user_id {
            same_public_key(&wrapper_key, &keypair.public_jwk)
        } else {
            let before = self.pins.clone();
            let known = self.pins.is_known_key(mine.wrapped_by, &wrapper_key);
            if self.pins != before {
                self.save_pins();
            }
            known
        };
        if !trusted {
            tracing::warn!("e2ee: ignoring the key of {} — wrapped by {} with a key this device doesn't trust", room.room_key, mine.wrapped_by);
            return Err(KeyProblem::UntrustedSharer(mine.wrapped_by));
        }

        let Some(current) = keypair.open_envelope(&mine.envelope, &wrapper_key) else { return Err(KeyProblem::WrongDeviceKey) };
        let mut keys = vec![current.clone()];
        if !archive.is_empty() {
            keys.extend(crypto::read_archived_keys(&archive, &current));
        }
        self.cache.insert(room.room_key.clone(), Cached { envelope: mine.envelope.clone(), archive, keys: keys.clone() });
        Ok(keys)
    }

    /// Envelopes of a new room key for these members (us included). A member whose key differs
    /// from the one pinned here is skipped (a key swap must not get the room key), and our own copy
    /// goes only to this computer's real key, whatever the server says ours is.
    pub fn wrap_for(&mut self, room_key: &str, members: &[(i64, String)]) -> KeyEnvelopes {
        let Some(keypair) = self.keypair.as_ref() else { return KeyEnvelopes::new() };
        let mut out = KeyEnvelopes::new();
        let before = self.pins.clone();
        for (id, public) in members {
            if *id == self.user_id {
                if !same_public_key(public, &keypair.public_jwk) {
                    continue;
                }
            } else {
                if self.pins.check_and_pin(*id, public) == KeyTrust::Changed {
                    continue;
                }
            }
            if let Some(envelope) = keypair.create_envelope(room_key, public) {
                out.insert(id.to_string(), KeyEnvelope { envelope, wrapped_by: self.user_id, wrapper_public_key: Some(keypair.public_jwk.clone()) });
            }
        }
        if self.pins != before {
            self.save_pins();
        }
        out
    }

    /// May we vouch for this member's key (owner tags)? Our own only if it's this computer's;
    /// others' as long as it's the key pinned here (first sight pins it).
    pub fn trusts(&mut self, user_id: i64, public_jwk: &str) -> bool {
        if user_id == self.user_id {
            return self.keypair.as_ref().is_some_and(|k| same_public_key(public_jwk, &k.public_jwk));
        }
        let before = self.pins.clone();
        let ok = self.pins.check_and_pin(user_id, public_jwk) != KeyTrust::Changed;
        if self.pins != before {
            self.save_pins();
        }
        ok
    }

    /// How this device sees a user's key, without pinning anything: None = never seen here.
    pub fn trust_of(&self, user_id: i64, public_jwk: &str) -> Option<KeyTrust> {
        if user_id == self.user_id {
            return Some(if self.keypair.as_ref().is_some_and(|k| same_public_key(public_jwk, &k.public_jwk)) { KeyTrust::Verified } else { KeyTrust::Changed });
        }
        let pin = self.pins.pins.get(&user_id.to_string())?;
        let fp = fingerprint(public_jwk);
        Some(if pin.fp != fp { KeyTrust::Changed } else if pin.verified { KeyTrust::Verified } else { KeyTrust::Trusted })
    }

    /// A key change for this user is waiting to be accepted (don't vouch for their key).
    pub fn is_changed(&self, user_id: i64) -> bool {
        self.pins.changed.contains_key(&user_id.to_string())
    }

    pub fn public_key(&self) -> Option<&str> {
        self.keypair.as_ref().map(|k| k.public_jwk.as_str())
    }
}

/// A message as received → as shown: body, attachment and reply opened with the room's keys.
/// With no keys, or ciphertext none of them opens, the body stays ciphertext (shown as locked).
pub fn open_message(mut m: ChatMessageItem, keys: Option<&[String]>) -> ChatMessageItem {
    let Some(keys) = keys else { return m };
    if let Opened::Text(t) = decrypt_symmetric(&m.content, keys) {
        m.content = t;
    }
    if let Some(a) = m.attachment_data.take() {
        m.attachment_data = Some(open_json(a, keys));
    }
    if let Some(r) = m.reply_to_data.take() {
        m.reply_to_data = Some(open_json(r, keys));
    }
    m
}

fn open_json(v: Value, keys: &[String]) -> Value {
    let sealed = v.get("encrypted").and_then(Value::as_bool) == Some(true) || v.get("iv").is_some();
    if !sealed {
        return v;
    }
    decrypt_json(&v, keys).unwrap_or(v)
}

/// The chat list's last line, opened if we can.
pub fn open_preview(text: &str, keys: &[String]) -> Option<String> {
    match decrypt_symmetric(text, keys) {
        Opened::Text(t) => Some(t),
        _ => None,
    }
}

pub fn seal_text(text: &str, keys: &[String]) -> Option<String> {
    encrypt_symmetric(text, keys.first()?)
}

pub fn seal_json(v: &Value, keys: &[String]) -> Option<Value> {
    encrypt_json(v, keys.first()?)
}

fn hmac_b64(key: &[u8], message: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(message.as_bytes());
    b64(&mac.finalize().into_bytes())
}

/// An owner admission tag (computeOwnerTag): HMAC under a sub-key of the room key over the room,
/// the member and their key's fingerprint. Other members' apps check it before sharing the room
/// key with someone — the server, never having the room key, can't make one.
pub fn owner_tag(room_aes_key_b64: &str, room_key: &str, user_id: i64, public_jwk: &str) -> Option<String> {
    let sub = unb64(&hmac_b64(&unb64(room_aes_key_b64)?, "accordo/member-auth/v1"))?;
    Some(hmac_b64(&sub, &format!("owner|{room_key}|{user_id}|{}", fingerprint(public_jwk))))
}

/// Members of a new room, as `(id, public key)`, from their profiles (no key = no envelope yet:
/// they get one when someone opens the room after they set up their key).
pub fn members_with_keys(profiles: &[UserInfo]) -> Vec<(i64, String)> {
    profiles.iter().filter_map(|u| u.e2ee_public_key.clone().filter(|k| !k.is_empty()).map(|k| (u.id, k))).collect()
}

// ── admission: who may be handed the room key (e2eeMembership.ts) ──

/// An invite admission tag (computeInviteTag): HMAC keyed by the invite link's #k secret.
pub fn invite_tag(invite_secret: &str, user_id: i64, public_jwk: &str) -> String {
    hmac_b64(invite_secret.as_bytes(), &format!("invite|{user_id}|{}", fingerprint(public_jwk)))
}

/// A fresh invite secret: 24 random bytes, base64url without padding (generateInviteSecret).
pub fn generate_invite_secret() -> String {
    b64(&crypto::random_bytes::<24>()).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

/// The invite secrets the room keeps encrypted under its key (decryptInviteSecrets).
pub fn invite_secrets(room: &ChatRoomItem, keys: &[String]) -> Vec<String> {
    let blobs = room.e2ee_meta.as_ref().and_then(|m| m.invite_secrets.clone()).unwrap_or_default();
    blobs.iter().filter_map(|b| match decrypt_symmetric(b, keys) {
        Opened::Text(t) => Some(t),
        _ => None,
    }).collect()
}

/// "dm:<lo>_<hi>" → the two participants.
pub fn dm_participants(room_key: &str) -> Option<(i64, i64)> {
    let (a, b) = room_key.strip_prefix("dm:")?.split_once('_')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Can this member's admission be proven without trusting the server (isAdmissionVerified)? A DM
/// participant by the room's own key; a group member by an owner tag (made by someone holding the
/// room key) or an invite tag (made with the link's secret). Anyone else may have been written
/// into the member list directly and waits for a manual approval.
pub fn is_admission_verified(room: &ChatRoomItem, member: &UserInfo, keys: &[String]) -> bool {
    if let Some((a, b)) = dm_participants(&room.room_key) {
        return member.id == a || member.id == b;
    }
    if room.chat_type == "dm" {
        return false;
    }
    let (Some(public), Some(current)) = (member.e2ee_public_key.as_deref(), keys.first()) else { return false };
    let Some(entry) = room.member_auth.as_ref().and_then(|a| a.get(&member.id.to_string())).filter(|e| !e.tag.is_empty()) else { return false };
    match entry.kind.as_str() {
        "owner" => owner_tag(current, &room.room_key, member.id, public).as_deref() == Some(entry.tag.as_str()),
        "invite" => invite_secrets(room, keys).iter().any(|s| invite_tag(s, member.id, public) == entry.tag),
        _ => false,
    }
}

/// Members with a public key but no copy of the room key, split into those whose admission is
/// proven (hand them the key) and those waiting for a manual approval.
pub fn missing_members(room: &ChatRoomItem, me: i64, keys: &[String]) -> (Vec<UserInfo>, Vec<i64>) {
    let existing = room.key_envelopes.clone().unwrap_or_default();
    let mut admitted = vec![];
    let mut pending = vec![];
    for m in &room.allowed_users {
        if m.id == me || m.e2ee_public_key.as_deref().is_none_or(str::is_empty) || existing.contains_key(&m.id.to_string()) || !room.allowed_user_ids.contains(&m.id) {
            continue;
        }
        if is_admission_verified(room, m, keys) { admitted.push(m.clone()) } else { pending.push(m.id) }
    }
    (admitted, pending)
}
