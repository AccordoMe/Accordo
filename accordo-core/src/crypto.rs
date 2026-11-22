// SPDX-License-Identifier: GPL-3.0-or-later
//! The primitives of utils/e2ee.ts, matching WebCrypto byte for byte.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, URL_SAFE_NO_PAD};
use base64::engine::DecodePaddingMode;
use base64::Engine;
use p256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use p256::{EncodedPoint, PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ── base64 as btoa / atob (standard alphabet; padding optional when reading) ──

const B64: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent));

pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn unb64(text: &str) -> Option<Vec<u8>> {
    B64.decode(text.trim()).ok()
}

/// JWK coordinates: base64url without padding.
fn b64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64url(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).ok()
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the system's random source");
    b
}

/// The text WebCrypto's TextDecoder would give (invalid UTF-8 → replacement characters).
fn utf8(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

// ── AES-256-GCM ──

fn aes_encrypt(key: &[u8], iv: &[u8; 12], plaintext: &[u8]) -> Option<Vec<u8>> {
    Aes256Gcm::new_from_slice(key).ok()?.encrypt(&Nonce::from(*iv), plaintext).ok()
}

fn aes_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    let iv: [u8; 12] = iv.try_into().ok()?;
    Aes256Gcm::new_from_slice(key).ok()?.decrypt(&Nonce::from(iv), ciphertext).ok()
}

pub fn seal(key: &[u8], plaintext: &[u8]) -> Option<(String, String)> {
    let iv = random_bytes::<12>();
    let data = aes_encrypt(key, &iv, plaintext)?;
    Some((b64(&iv), b64(&data)))
}

pub fn open(key: &[u8], iv_b64: &str, data_b64: &str) -> Option<Vec<u8>> {
    aes_decrypt(key, &unb64(iv_b64)?, &unb64(data_b64)?)
}

// ── room keys and messages ──

/// A new room key: 32 random bytes, base64.
pub fn generate_room_key() -> String {
    b64(&random_bytes::<32>())
}

/// Which room key sealed a ciphertext: base64(SHA-256(raw key)), first 12 characters.
pub fn key_id(room_key_b64: &str) -> String {
    let raw = unb64(room_key_b64).unwrap_or_default();
    b64(&Sha256::digest(raw))[..12].to_string()
}

/// `{"iv","data","k"}` — how the web encrypts message bodies, attachments and replies.
pub fn encrypt_symmetric(plaintext: &str, room_key_b64: &str) -> Option<String> {
    let key = unb64(room_key_b64)?;
    let (iv, data) = seal(&key, plaintext.as_bytes())?;
    Some(json!({ "iv": iv, "data": data, "k": key_id(room_key_b64) }).to_string())
}

/// What decrypt_symmetric gives back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// The plaintext.
    Text(String),
    /// Not ciphertext at all (public rooms, old plaintext): the input as it was.
    Plain(String),
    /// Ciphertext none of the keys opens.
    Locked,
}

impl Opened {
    pub fn text(self) -> Option<String> {
        match self {
            Opened::Text(t) | Opened::Plain(t) => Some(t),
            Opened::Locked => None,
        }
    }
}

pub fn looks_encrypted(text: &str) -> bool {
    text.starts_with('{') && text.contains("\"iv\"")
}

/// Opens a payload with the room's current key, then its archived keys — the one whose id matches
/// `k` goes first (decryptSymmetric).
pub fn decrypt_symmetric(payload: &str, keys: &[String]) -> Opened {
    if !looks_encrypted(payload) {
        return Opened::Plain(payload.to_string());
    }
    let Ok(parsed) = serde_json::from_str::<Value>(payload) else { return Opened::Locked };
    let (Some(iv), Some(data)) = (parsed.get("iv").and_then(Value::as_str), parsed.get("data").and_then(Value::as_str)) else {
        return Opened::Plain(payload.to_string());
    };
    let mut ordered: Vec<&String> = keys.iter().collect();
    if let Some(k) = parsed.get("k").and_then(Value::as_str) {
        ordered.sort_by_key(|key| key_id(key) != k); // matching id first, the rest in order
    }
    for key in ordered {
        if let Some(plain) = unb64(key).and_then(|raw| open(&raw, iv, data)) {
            return Opened::Text(utf8(plain));
        }
    }
    Opened::Locked
}

/// Attachments / reply data: `{"encrypted": true, "iv", "data", "k"}` → the JSON value inside.
pub fn decrypt_json(value: &Value, keys: &[String]) -> Option<Value> {
    let sealed = value.get("encrypted").and_then(Value::as_bool) == Some(true) || value.get("iv").is_some();
    if !sealed {
        return Some(value.clone());
    }
    serde_json::from_str(&decrypt_symmetric(&value.to_string(), keys).text()?).ok()
}

/// The sending side of decrypt_json.
pub fn encrypt_json(value: &Value, room_key_b64: &str) -> Option<Value> {
    let mut sealed: Value = serde_json::from_str(&encrypt_symmetric(&value.to_string(), room_key_b64)?).ok()?;
    sealed["encrypted"] = json!(true);
    Some(sealed)
}

/// A room's previous keys, kept encrypted under the current one (`e2ee_meta.archived_keys`).
pub fn read_archived_keys(blob: &str, current_key: &str) -> Vec<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(blob) else { return Vec::new() };
    let (Some(iv), Some(data)) = (parsed.get("iv").and_then(Value::as_str), parsed.get("data").and_then(Value::as_str)) else { return Vec::new() };
    let Some(raw) = unb64(current_key).and_then(|k| open(&k, iv, data)) else { return Vec::new() };
    serde_json::from_slice::<Vec<Value>>(&raw).map(|v| v.into_iter().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// Seals the list of previous keys under the new one (the sending side of read_archived_keys).
pub fn seal_archived_keys(keys: &[String], new_key: &str) -> Option<String> {
    let raw = unb64(new_key)?;
    let (iv, data) = seal(&raw, serde_json::to_string(keys).ok()?.as_bytes())?;
    Some(json!({ "iv": iv, "data": data }).to_string())
}

// ── device keypair (ECDH P-256 as JWK) ──

/// A P-256 JWK as WebCrypto exports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Jwk {
    crv: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    d: Option<String>,
    #[serde(default = "yes")]
    ext: bool,
    #[serde(default)]
    key_ops: Vec<String>,
    kty: String,
    x: String,
    y: String,
}

fn yes() -> bool {
    true
}

fn public_from_jwk(jwk: &str) -> Option<PublicKey> {
    let j: Jwk = serde_json::from_str(jwk).ok()?;
    if j.kty != "EC" || j.crv != "P-256" {
        return None;
    }
    let (x, y) = (unb64url(&j.x)?, unb64url(&j.y)?);
    if x.len() != 32 || y.len() != 32 {
        return None;
    }
    let point = EncodedPoint::from_affine_coordinates(x.as_slice().into(), y.as_slice().into(), false);
    Option::from(PublicKey::from_encoded_point(&point))
}

fn secret_from_jwk(jwk: &str) -> Option<SecretKey> {
    let j: Jwk = serde_json::from_str(jwk).ok()?;
    SecretKey::from_slice(&unb64url(j.d.as_deref()?)?).ok()
}

fn public_jwk(public: &PublicKey) -> String {
    let point = public.to_encoded_point(false);
    let jwk = Jwk {
        crv: "P-256".into(),
        d: None,
        ext: true,
        key_ops: Vec::new(),
        kty: "EC".into(),
        x: b64url(point.x().expect("uncompressed point")),
        y: b64url(point.y().expect("uncompressed point")),
    };
    serde_json::to_string(&jwk).expect("JWK")
}

/// This device's chat keypair: both halves as JWK text, the way the web stores and backs them up.
#[derive(Clone)]
pub struct DeviceKeypair {
    pub public_jwk: String,
    pub private_jwk: String,
    secret: SecretKey,
}

impl std::fmt::Debug for DeviceKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeypair").field("public_jwk", &self.public_jwk).finish_non_exhaustive()
    }
}

impl DeviceKeypair {
    pub fn generate() -> Self {
        loop {
            if let Ok(secret) = SecretKey::from_slice(&random_bytes::<32>()) {
                return Self::from_secret(secret);
            }
        }
    }

    fn from_secret(secret: SecretKey) -> Self {
        let public_jwk = public_jwk(&secret.public_key());
        let mut jwk: Jwk = serde_json::from_str(&public_jwk).expect("own JWK");
        jwk.d = Some(b64url(&secret.to_bytes()));
        jwk.key_ops = vec!["deriveKey".into(), "deriveBits".into()];
        Self { public_jwk, private_jwk: serde_json::to_string(&jwk).expect("JWK"), secret }
    }

    /// From a stored / restored pair. The public text is kept as given (it's what the account
    /// publishes and the server compares).
    pub fn from_jwk(public_jwk: &str, private_jwk: &str) -> Option<Self> {
        Some(Self { public_jwk: public_jwk.to_string(), private_jwk: private_jwk.to_string(), secret: secret_from_jwk(private_jwk)? })
    }

    /// Does the private half belong to the public one? (selfTestMyKeypair: otherwise nobody can
    /// open what we wrap for them, while we still open what we wrapped for ourselves.)
    pub fn self_test(&self) -> bool {
        public_from_jwk(&self.public_jwk).is_some_and(|p| p == self.secret.public_key())
    }

    fn shared_key(&self, their_public_jwk: &str) -> Option<[u8; 32]> {
        let theirs = public_from_jwk(their_public_jwk)?;
        let shared = p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), theirs.as_affine());
        let mut key = [0u8; 32];
        key.copy_from_slice(&shared.raw_secret_bytes()[..]);
        Some(key)
    }

    /// The room key wrapped for one member: `{"iv","data"}` (createRoomKeyEnvelope).
    pub fn create_envelope(&self, room_key_b64: &str, recipient_public_jwk: &str) -> Option<String> {
        let key = self.shared_key(recipient_public_jwk)?;
        let (iv, data) = seal(&key, room_key_b64.as_bytes())?;
        Some(json!({ "iv": iv, "data": data }).to_string())
    }

    /// Opens our envelope with the wrapper's public key (unpackRoomKeyEnvelope).
    pub fn open_envelope(&self, envelope: &str, wrapper_public_jwk: &str) -> Option<String> {
        let key = self.shared_key(wrapper_public_jwk)?;
        let parsed: Value = serde_json::from_str(envelope).ok()?;
        let plain = open(&key, parsed.get("iv")?.as_str()?, parsed.get("data")?.as_str()?)?;
        Some(utf8(plain))
    }
}

/// Same EC point? (JWK texts can differ in field order.)
pub fn same_public_key(a: &str, b: &str) -> bool {
    match (serde_json::from_str::<Value>(a), serde_json::from_str::<Value>(b)) {
        (Ok(x), Ok(y)) => x.get("x") == y.get("x") && x.get("y") == y.get("y") && x.get("x").is_some(),
        _ => a == b,
    }
}

/// A public key's fingerprint: SHA-256 of "crv|x|y", 16 groups of 4 hex digits (e2eeTrust.ts).
pub fn fingerprint(public_jwk: &str) -> String {
    let canonical = serde_json::from_str::<Value>(public_jwk)
        .ok()
        .and_then(|j| {
            let (x, y) = (j.get("x")?.as_str()?, j.get("y")?.as_str()?);
            Some(format!("{}|{x}|{y}", j.get("crv").and_then(Value::as_str).unwrap_or("P-256")))
        })
        .unwrap_or_else(|| public_jwk.to_string());
    let hex: String = Sha256::digest(canonical.as_bytes()).iter().map(|b| format!("{b:02X}")).collect();
    hex.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap_or_default()).collect::<Vec<_>>().join(" ")
}

/// The first 4 groups, for compact UI.
pub fn short_fingerprint(fp: &str) -> String {
    fp.split(' ').take(4).collect::<Vec<_>>().join(" ")
}
