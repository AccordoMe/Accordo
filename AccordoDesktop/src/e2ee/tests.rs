// SPDX-License-Identifier: GPL-3.0-or-later
//! Byte compatibility with the web app. tests/fixtures/webcrypto.json is made by WebCrypto
//! (scripts/webcrypto_vectors.mjs, the same code as utils/e2ee.ts and vault.ts); the Rust side
//! must open it, and what Rust makes is checked by WebCrypto in the other direction:
//! `ACCORDO_RUST_VECTORS=/tmp/rust.json cargo test -- --ignored rust_vectors`, then
//! `node scripts/webcrypto_vectors.mjs check /tmp/rust.json`.

use serde_json::{json, Value};

use super::crypto::*;
use super::phrase::*;
use super::trust::{KeyTrust, TrustState};
use super::vault::*;

fn fx() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/webcrypto.json")).unwrap()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap()
}

#[test]
fn opens_webcrypto_messages() {
    let f = fx();
    let room = s(&f, "room_key").to_string();
    assert_eq!(key_id(&room), s(&f, "key_id"));
    assert_eq!(decrypt_symmetric(s(&f, "ciphertext"), &[room.clone()]), Opened::Text(s(&f, "text").into()));
    // Not ciphertext: as it is.
    assert_eq!(decrypt_symmetric("plain hello", &[room.clone()]), Opened::Plain("plain hello".into()));
    // A wrong key: locked, never garbage.
    assert_eq!(decrypt_symmetric(s(&f, "ciphertext"), &[generate_room_key()]), Opened::Locked);
    // Attachments.
    assert_eq!(decrypt_json(&f["attachment_sealed"], &[room]).unwrap(), f["attachment"]);
}

#[test]
fn reads_webcrypto_key_archive() {
    let f = fx();
    let room = s(&f, "room_key");
    let old: Vec<String> = serde_json::from_value(f["old_keys"].clone()).unwrap();
    assert_eq!(read_archived_keys(s(&f, "archived_keys"), room), old);
    // A message sealed before the rotation opens with current + archived keys (k picks the right one).
    let mut keys = vec![room.to_string()];
    keys.extend(old);
    assert_eq!(decrypt_symmetric(s(&f, "ciphertext_old_key"), &keys), Opened::Text("from before the rotation".into()));
}

#[test]
fn opens_webcrypto_envelopes_and_keys() {
    let f = fx();
    let bob = DeviceKeypair::from_jwk(s(&f["bob"], "pub"), s(&f["bob"], "priv")).unwrap();
    assert!(bob.self_test());
    assert_eq!(bob.open_envelope(s(&f, "envelope_alice_to_bob"), s(&f["alice"], "pub")).unwrap(), s(&f, "room_key"));
    // The other way round with the same pair of keys: Rust-made envelope, Rust-opened.
    let alice = DeviceKeypair::from_jwk(s(&f["alice"], "pub"), s(&f["alice"], "priv")).unwrap();
    let env = alice.create_envelope("room-key-b64", &bob.public_jwk).unwrap();
    assert_eq!(bob.open_envelope(&env, &alice.public_jwk).unwrap(), "room-key-b64");
    // A mismatched pair fails its self-test.
    let mixed = DeviceKeypair::from_jwk(s(&f["alice"], "pub"), s(&f["bob"], "priv")).unwrap();
    assert!(!mixed.self_test());
    assert_eq!(fingerprint(s(&f["alice"], "pub")), s(&f, "alice_fingerprint"));
    assert!(same_public_key(s(&f["alice"], "pub"), &alice.public_jwk));
}

#[test]
fn opens_webcrypto_vaults() {
    let f = fx();
    let pw = s(&f, "vault_password");
    let v: Vault = serde_json::from_value(f["vault"].clone()).unwrap();
    assert_eq!(decrypt_vault(&v, pw).unwrap(), r#"{"priv":"x","pub":"y"}"#);
    assert_eq!(decrypt_vault(&v, "wrong").unwrap_err(), VaultError::WrongPassword);
    let legacy: Vault = serde_json::from_value(f["vault_legacy"].clone()).unwrap();
    assert!(needs_upgrade(&legacy));
    assert_eq!(decrypt_vault(&legacy, pw).unwrap(), "legacy data");
}

#[test]
fn own_round_trips() {
    let k = generate_room_key();
    let sealed = encrypt_symmetric("hi", &k).unwrap();
    assert!(looks_encrypted(&sealed));
    assert_eq!(decrypt_symmetric(&sealed, &[k.clone()]).text().unwrap(), "hi");
    let a = encrypt_json(&json!({"x": 1}), &k).unwrap();
    assert_eq!(decrypt_json(&a, &[k.clone()]).unwrap(), json!({"x": 1}));
    let kp = DeviceKeypair::generate();
    assert!(kp.self_test());
    let again = DeviceKeypair::from_jwk(&kp.public_jwk, &kp.private_jwk).unwrap();
    assert_eq!(again.open_envelope(&kp.create_envelope(&k, &kp.public_jwk).unwrap(), &kp.public_jwk).unwrap(), k);
}

#[test]
fn secrets_and_trust() {
    let p = generate_phrase();
    assert_eq!(p.split(' ').count(), 12);
    assert_eq!(phrase_problem(&p), None);
    assert_eq!(normalize_secret(&format!("  {}\n", p.to_uppercase())), p);
    assert_eq!(normalize_secret("just a password"), "just a password");
    assert!(phrase_problem("one two").unwrap().contains("2 of 12"));
    assert_eq!(generate_password(20).len(), 20);

    let f = fx();
    let (a, b) = (s(&f["alice"], "pub"), s(&f["bob"], "pub"));
    let mut t = TrustState::default();
    assert_eq!(t.check_and_pin(7, a), KeyTrust::New);
    assert_eq!(t.check_and_pin(7, a), KeyTrust::Trusted);
    assert_eq!(t.check_and_pin(7, b), KeyTrust::Changed); // a swapped key: don't wrap for it
    assert!(!t.is_known_key(7, b));
    t.accept_change(7);
    assert_eq!(t.check_and_pin(7, b), KeyTrust::Trusted);
    assert!(t.is_known_key(7, a)); // old envelopes still open
    // Same JSON as the web's localStorage pins.
    let json = serde_json::to_value(&t).unwrap();
    assert!(json["pins"]["7"]["pinnedAt"].is_i64());
}

/// Writes Rust-made data for `node scripts/webcrypto_vectors.mjs check`.
#[test]
#[ignore]
fn rust_vectors() {
    let f = fx();
    let path = std::env::var("ACCORDO_RUST_VECTORS").expect("ACCORDO_RUST_VECTORS=<file>");
    let rust = DeviceKeypair::generate();
    let room = generate_room_key();
    let old = vec![generate_room_key(), generate_room_key()];
    let vault = encrypt_vault("rust vault text", "pw from rust");
    let out = json!({
        "room_key": room,
        "text": "Rust → WebCrypto ✓",
        "ciphertext": encrypt_symmetric("Rust → WebCrypto ✓", &room).unwrap(),
        "rust": { "pub": rust.public_jwk, "priv": rust.private_jwk },
        "rust_fingerprint": fingerprint(&rust.public_jwk),
        "envelope_rust_to_bob": rust.create_envelope(&room, s(&f["bob"], "pub")).unwrap(),
        "vault": vault, "vault_password": "pw from rust", "vault_plain": "rust vault text",
        "old_keys": old, "archived_keys": seal_archived_keys(&old, &room).unwrap(),
    });
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

#[test]
fn reads_web_key_files() {
    let f = fx();
    // exportE2EEBackupFile: the keys as JWK objects.
    let file = json!({
        "app": "Accordo", "type": "e2ee_keypair_backup", "version": "1.0", "username": "bob",
        "public_key": serde_json::from_str::<Value>(s(&f["bob"], "pub")).unwrap(),
        "private_key": serde_json::from_str::<Value>(s(&f["bob"], "priv")).unwrap(),
    });
    let kp = super::account::parse_key_file(&file.to_string()).unwrap();
    assert!(same_public_key(&kp.public_jwk, s(&f["bob"], "pub")));
    // …and opens what was wrapped for that key.
    assert_eq!(kp.open_envelope(s(&f, "envelope_alice_to_bob"), s(&f["alice"], "pub")).unwrap(), s(&f, "room_key"));
    // A file whose halves don't belong together is refused.
    let mixed = json!({ "public_key": s(&f["alice"], "pub"), "private_key": s(&f["bob"], "priv") });
    assert!(super::account::parse_key_file(&mixed.to_string()).is_err());
    assert!(super::account::parse_key_file("not json").is_err());
}

#[test]
fn owner_tags_match_the_web() {
    let f = fx();
    // Same inputs through Node's HMAC-SHA256 (computeOwnerTag).
    let tag = super::rooms::owner_tag(s(&f, "room_key"), "group:abc", 7, s(&f["alice"], "pub")).unwrap();
    assert_eq!(tag, "5LD0TDYYpy12Xk0MG7NeIfQLUxjrwItJW8unPVdUNqQ=");
}

#[test]
fn room_keys_open_and_seal_messages() {
    use crate::api::chat::{ChatMessageItem, ChatRoomItem, KeyEnvelope, UserInfo};
    let f = fx();
    let bob = DeviceKeypair::from_jwk(s(&f["bob"], "pub"), s(&f["bob"], "priv")).unwrap();
    let mut keys = super::rooms::RoomKeys::default();
    let mut room = ChatRoomItem { room_key: "dm:1_2".into(), chat_type: "dm".into(), ..Default::default() };
    // No device key, then no envelope.
    assert_eq!(keys.resolve(&room), Err(super::rooms::KeyProblem::NoDeviceKey));
    keys.set_keypair(Some(bob));
    assert_eq!(keys.resolve(&room), Err(super::rooms::KeyProblem::NoEnvelope));
    // Alice (id 1) wrapped the room key for us (id 0 = RoomKeys::default's user).
    room.key_envelopes = Some([("0".to_string(), KeyEnvelope { envelope: s(&f, "envelope_alice_to_bob").into(), wrapped_by: 1, wrapper_public_key: None })].into());
    assert_eq!(keys.resolve(&room), Err(super::rooms::KeyProblem::UnknownSharer(1)));
    room.allowed_users = vec![UserInfo { id: 1, e2ee_public_key: Some(s(&f["alice"], "pub").into()), ..Default::default() }];
    let k = keys.resolve(&room).unwrap();
    assert_eq!(k[0], s(&f, "room_key"));
    // A message sealed here opens with the same key; one from the web too.
    let sealed = super::rooms::seal_text("привет", &k).unwrap();
    let m = super::rooms::open_message(ChatMessageItem { content: sealed, ..Default::default() }, Some(&k));
    assert_eq!(m.content, "привет");
}

#[test]
fn admission_tags() {
    use crate::api::chat::{ChatRoomItem, E2eeMeta, MemberAuthEntry, UserInfo};
    use super::rooms::*;
    let f = fx();
    let key = s(&f, "room_key").to_string();
    let alice = UserInfo { id: 7, e2ee_public_key: Some(s(&f["alice"], "pub").into()), ..Default::default() };
    // DM: the room's key names its participants.
    let dm = ChatRoomItem { room_key: "dm:3_7".into(), chat_type: "dm".into(), ..Default::default() };
    assert!(is_admission_verified(&dm, &alice, &[key.clone()]));
    // Group: an owner tag made with the room key; a wrong tag fails.
    let tag = owner_tag(&key, "group:abc", 7, s(&f["alice"], "pub")).unwrap();
    let mut g = ChatRoomItem { room_key: "group:abc".into(), chat_type: "group".into(), ..Default::default() };
    g.member_auth = Some([("7".to_string(), MemberAuthEntry { tag, kind: "owner".into() })].into());
    assert!(is_admission_verified(&g, &alice, &[key.clone()]));
    g.member_auth = Some([("7".to_string(), MemberAuthEntry { tag: "nope".into(), kind: "owner".into() })].into());
    assert!(!is_admission_verified(&g, &alice, &[key.clone()]));
    // Invite tag: the secret stored encrypted under the room key.
    let secret = generate_invite_secret();
    g.e2ee_meta = Some(E2eeMeta { invite_secrets: Some(vec![encrypt_symmetric(&secret, &key).unwrap()]), ..Default::default() });
    g.member_auth = Some([("7".to_string(), MemberAuthEntry { tag: invite_tag(&secret, 7, s(&f["alice"], "pub")), kind: "invite".into() })].into());
    assert!(is_admission_verified(&g, &alice, &[key]));
}

#[test]
fn invite_tags_match_the_web() {
    let f = fx();
    assert_eq!(super::rooms::invite_tag("s3cret", 7, s(&f["alice"], "pub")), "VKSfepWtqc9aEiSGVAqmWWURXT0tmUs4qWB5cDcHHaU=");
}
