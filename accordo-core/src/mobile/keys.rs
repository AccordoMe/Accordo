// SPDX-License-Identifier: GPL-3.0-or-later
//! Room keys for Kotlin: opening the envelope that carries a chat's key, and wrapping a new key
//! for the people in it.
//!
//! The rules are the core's (`accordo_core::rooms`), unchanged from the desktop: our envelope is
//! opened with this device's key and the *wrapper's* key, and only if this device trusts that key
//! (first sight pins it; a changed key stops the envelope from being used). Archived keys come with
//! the current one, so older messages stay readable. No envelope we can open means no key — there is
//! deliberately no fallback anyone could compute.

use std::sync::{Arc, Mutex};

use crate::api::chat::ChatRoomItem;
use crate::mobile::MobileError;
use crate::rooms::RoomKeys;
use crate::DeviceKey;

/// One member whose key a room key should be wrapped for.
#[derive(uniffi::Record)]
pub struct MemberKey {
    pub id: i64,
    pub public_jwk: String,
}

/// Holds this session's room keys and the trust pins. Create it after sign-in, attach the device
/// key, then ask for keys per room as the feed opens.
#[derive(uniffi::Object)]
pub struct RoomKeyring {
    inner: Mutex<RoomKeys>,
}

#[uniffi::export]
impl RoomKeyring {
    /// `account` is the same key the chat key is stored under (`1:alina@https://accordo.me`).
    #[uniffi::constructor]
    pub fn new(user_id: i64, account: String) -> Arc<Self> {
        Arc::new(Self { inner: Mutex::new(RoomKeys::new(user_id, account)) })
    }

    /// This device's chat key. Without it a sealed room stays unreadable (`open` explains).
    pub fn use_device_key(&self, key: Arc<DeviceKey>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.set_keypair(Some(key.pair().clone()));
        }
    }

    /// The room's keys, the current one first and archived ones after it. An empty list means the
    /// room isn't sealed at all; an error carries the sentence the chat shows the user.
    pub fn open(&self, room_json: String) -> Result<Vec<String>, MobileError> {
        let room: ChatRoomItem = serde_json::from_str(&room_json)?;
        if room.key_envelopes.as_ref().is_none_or(|e| e.is_empty()) {
            return Ok(Vec::new());
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| MobileError::Keys { detail: "the room keyring is poisoned".into() })?;
        inner.resolve(&room).map_err(|problem| MobileError::Keys { detail: problem.explain().to_string() })
    }

    /// Wraps `room_key` for these members (us included, when our key is in the list): the JSON of
    /// `KeyEnvelopes`, ready to send with the room. A member whose key changed since it was pinned
    /// is skipped — a swapped key must not receive the room key.
    pub fn wrap_for(&self, room_key: String, members: Vec<MemberKey>) -> Result<String, MobileError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| MobileError::Keys { detail: "the room keyring is poisoned".into() })?;
        let pairs: Vec<(i64, String)> = members.into_iter().map(|m| (m.id, m.public_jwk)).collect();
        let envelopes = inner.wrap_for(&room_key, &pairs);
        Ok(serde_json::to_string(&envelopes)?)
    }

    /// How this device sees a user's key, without pinning anything:
    /// "new" | "trusted" | "verified" | "changed" — or "" when we never saw it.
    pub fn trust_of(&self, user_id: i64, public_jwk: String) -> String {
        let Ok(inner) = self.inner.lock() else { return String::new() };
        match inner.trust_of(user_id, &public_jwk) {
            Some(crate::trust::KeyTrust::New) => "new",
            Some(crate::trust::KeyTrust::Trusted) => "trusted",
            Some(crate::trust::KeyTrust::Verified) => "verified",
            Some(crate::trust::KeyTrust::Changed) => "changed",
            None => "",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chat::{ChatMessageItem, KeyEnvelope, KeyEnvelopes};
    use crate::crypto::{decrypt_symmetric, encrypt_symmetric, Opened};
    use std::collections::HashMap;

    fn room_json(room_key: &str, envelopes: KeyEnvelopes) -> String {
        let room = ChatRoomItem {
            id: 1,
            room_key: room_key.to_string(),
            chat_type: "dm".into(),
            title: "DM".into(),
            key_envelopes: Some(envelopes),
            ..Default::default()
        };
        serde_json::to_string(&room).unwrap()
    }

    /// The sender's side: wrap the room key for Alice (7) and Bob (9).
    fn wrapped(sender: &DeviceKey, alice: &DeviceKey, bob: &DeviceKey, key: &str) -> KeyEnvelopes {
        let mut out = HashMap::new();
        for (id, device) in [(7, alice), (9, bob)] {
            let envelope = sender.create_envelope(key.to_string(), device.public_jwk()).expect("wrap");
            out.insert(
                id.to_string(),
                KeyEnvelope { envelope, wrapped_by: 5, wrapper_public_key: Some(sender.public_jwk()) },
            );
        }
        out
    }

    #[test]
    fn the_feed_of_a_sealed_room_opens_with_the_device_key() {
        let sender = DeviceKey::generate();
        let alice = DeviceKey::generate();
        let bob = DeviceKey::generate();
        let key = crate::generate_room_key();
        let room = room_json("dm:7:9", wrapped(&sender, &alice, &bob, &key));

        let ring = RoomKeyring::new(7, String::new());
        ring.use_device_key(alice.clone());
        let keys = ring.open(room.clone()).expect("open");
        assert_eq!(keys, vec![key.clone()]);

        // The text sealed with that key reads back — and not with another key.
        let sealed = encrypt_symmetric("привет", &key).expect("seal");
        assert!(matches!(decrypt_symmetric(&sealed, &keys), Opened::Text(t) if t == "привет"));
        assert!(matches!(decrypt_symmetric(&sealed, &[crate::generate_room_key()]), Opened::Locked));

        // Bob's device opens its own envelope; a stranger's key opens nothing.
        let bob_ring = RoomKeyring::new(9, String::new());
        bob_ring.use_device_key(bob);
        assert_eq!(bob_ring.open(room.clone()).expect("bob"), vec![key]);
        let stranger = RoomKeyring::new(11, String::new());
        stranger.use_device_key(DeviceKey::generate());
        assert!(stranger.open(room).is_err(), "no envelope for user 11");
    }

    #[test]
    fn a_room_without_envelopes_is_not_sealed() {
        let ring = RoomKeyring::new(7, String::new());
        ring.use_device_key(DeviceKey::generate());
        let room = serde_json::to_string(&ChatRoomItem { room_key: "public:general".into(), ..Default::default() }).unwrap();
        assert!(ring.open(room).expect("plain room").is_empty());
    }

    #[test]
    fn a_key_shared_by_a_changed_sender_key_is_refused() {
        let honest = DeviceKey::generate();
        let impostor = DeviceKey::generate();
        let alice = DeviceKey::generate();
        let key = crate::generate_room_key();

        let ring = RoomKeyring::new(7, String::new());
        ring.use_device_key(alice.clone());

        // First sight pins the honest sender's key.
        let first = room_json("dm:5:7", wrapped(&honest, &alice, &DeviceKey::generate(), &key));
        assert!(ring.open(first).is_ok());

        // Someone else wraps the same room key claiming to be user 5: refused.
        let second = room_json("dm:5:7", wrapped(&impostor, &alice, &DeviceKey::generate(), &key));
        let refused = ring.open(second).expect_err("a changed key must not be trusted");
        assert!(matches!(refused, MobileError::Keys { .. }));
    }

    #[test]
    fn wrapping_for_members_produces_envelopes_others_can_open() {
        let alice = DeviceKey::generate();
        let bob = DeviceKey::generate();
        let key = crate::generate_room_key();

        let ring = RoomKeyring::new(7, String::new());
        ring.use_device_key(alice.clone());
        let json = ring
            .wrap_for(key.clone(), vec![MemberKey { id: 7, public_jwk: alice.public_jwk() }, MemberKey { id: 9, public_jwk: bob.public_jwk() }])
            .expect("wrap");
        let envelopes: KeyEnvelopes = serde_json::from_str(&json).unwrap();
        assert_eq!(envelopes.len(), 2);

        let bob_ring = RoomKeyring::new(9, String::new());
        bob_ring.use_device_key(bob);
        assert_eq!(bob_ring.open(room_json("dm:7:9", envelopes)).expect("bob opens"), vec![key]);
    }

    #[test]
    fn an_archived_key_comes_with_the_current_one() {
        let sender = DeviceKey::generate();
        let alice = DeviceKey::generate();
        let old = crate::generate_room_key();
        let current = crate::generate_room_key();

        let archive = crate::crypto::seal_archived_keys(std::slice::from_ref(&old), &current).expect("archive");
        let mut room: ChatRoomItem =
            serde_json::from_str(&room_json("dm:5:7", wrapped(&sender, &alice, &DeviceKey::generate(), &current))).unwrap();
        room.e2ee_meta = Some(crate::api::chat::E2eeMeta { archived_keys: Some(archive), ..Default::default() });

        let ring = RoomKeyring::new(7, String::new());
        ring.use_device_key(alice);
        let keys = ring.open(serde_json::to_string(&room).unwrap()).expect("open");
        assert_eq!(keys.first().map(String::as_str), Some(current.as_str()));
        assert!(keys.contains(&old), "the archived key stays readable");

        let _ = ChatMessageItem::default();
    }
}
