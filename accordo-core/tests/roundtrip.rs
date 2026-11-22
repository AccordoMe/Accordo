// SPDX-License-Identifier: GPL-3.0-or-later
//! The mobile core's own tests: the same round trips the phone will do, without Kotlin.

use accordo_core::*;

fn text_of(d: Decrypted) -> Option<String> {
    match d {
        Decrypted::Text(t) | Decrypted::Plain(t) => Some(t),
        Decrypted::Locked => None,
    }
}

#[test]
fn a_message_round_trips_through_the_room_key() {
    let key = generate_room_key();
    let sealed = encrypt_text("привет, это шифр".into(), key.clone()).expect("seal");
    assert!(looks_encrypted(sealed.clone()));
    assert_eq!(text_of(decrypt_text(sealed.clone(), vec![key.clone()])).as_deref(), Some("привет, это шифр"));
    // a wrong key leaves it locked
    assert!(text_of(decrypt_text(sealed, vec![generate_room_key()])).is_none());
}

#[test]
fn a_room_key_travels_in_an_envelope_between_two_devices() {
    let alice = DeviceKey::generate();
    let bob = DeviceKey::generate();
    let room_key = generate_room_key();

    let envelope = alice.create_envelope(room_key.clone(), bob.public_jwk()).expect("wrap");
    assert_eq!(bob.open_envelope(envelope, alice.public_jwk()), Some(room_key.clone()));

    // Carol's key opens nothing: the envelope is bound to Bob's
    let carol = DeviceKey::generate();
    assert_eq!(carol.open_envelope(alice.create_envelope(room_key, bob.public_jwk()).unwrap(), alice.public_jwk()), None);
}

#[test]
fn a_restored_backup_still_works_and_is_self_tested() {
    let original = DeviceKey::generate();
    let restored = device_key_from_jwk(original.public_jwk(), original.private_jwk()).expect("restore");
    assert!(restored.self_test());
    assert_eq!(restored.public_jwk(), original.public_jwk());
    assert_eq!(fingerprint_of(restored.public_jwk()), fingerprint_of(original.public_jwk()));
}

#[test]
fn the_same_plaintext_seals_differently_every_time() {
    let key = generate_room_key();
    let a = encrypt_text("same".into(), key.clone()).unwrap();
    let b = encrypt_text("same".into(), key).unwrap();
    assert_ne!(a, b, "random nonce per message");
}
