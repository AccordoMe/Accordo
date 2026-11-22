// SPDX-License-Identifier: GPL-3.0-or-later
//! Secrets as a password or a 12-word phrase (utils/secretPhrase.ts). The phrase is just a
//! password made of BIP39 words — it doesn't derive any wallet. 12 words = 128 bits.

use bip39::{Language, Mnemonic};

use super::crypto::random_bytes;

/// No look-alikes (0/O, 1/l/I) and no '_' (it reads as a space when written down).
const PASSWORD_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-=+";

/// A random password (20 characters ≈ 120 bits), every character equally likely.
pub fn generate_password(length: usize) -> String {
    let limit = 256 - 256 % PASSWORD_CHARS.len();
    let mut out = String::with_capacity(length);
    while out.len() < length {
        for b in random_bytes::<64>() {
            if (b as usize) < limit {
                out.push(PASSWORD_CHARS[b as usize % PASSWORD_CHARS.len()] as char);
                if out.len() == length {
                    break;
                }
            }
        }
    }
    out
}

/// A random 12-word BIP39 phrase (128 bits + checksum).
pub fn generate_phrase() -> String {
    Mnemonic::from_entropy_in(Language::English, &random_bytes::<16>()).expect("16 bytes of entropy").to_string()
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_lowercase).collect()
}

fn is_word(w: &str) -> bool {
    Language::English.find_word(w).is_some()
}

/// What a typed phrase is missing, or None for a valid 12-word phrase.
pub fn phrase_problem(input: &str) -> Option<String> {
    let w = words(input);
    if w.len() != 12 {
        return Some(format!("{} of 12 words", w.len()));
    }
    let unknown: Vec<&str> = w.iter().filter(|x| !is_word(x)).map(String::as_str).take(3).collect();
    if !unknown.is_empty() {
        return Some(format!("Not in the word list: {}", unknown.join(", ")));
    }
    if Mnemonic::parse_in_normalized(Language::English, &w.join(" ")).is_err() {
        return Some("These words don't form a valid phrase — check for a typo".into());
    }
    None
}

/// The exact secret keys are derived from: a 12-word phrase is normalised (case, extra spaces,
/// line breaks) so it opens the same keys however it's typed; anything else is used as-is.
pub fn normalize_secret(input: &str) -> String {
    let w = words(input);
    if w.len() == 12 && w.iter().all(|x| is_word(x)) {
        w.join(" ")
    } else {
        input.to_string()
    }
}
