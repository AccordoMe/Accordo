// SPDX-License-Identifier: GPL-3.0-or-later
//! Base Builder Code (ERC-8021): transactions the app sends on Base carry Accordo's code at the end
//! of their calldata, so Base credits the activity to Accordo (base.dev → Builder Codes). The web
//! (src/utils/builderCode.ts) and the relayer (backend/app/builder_code.py) add the same bytes.
//!
//! The suffix, read from the end: a 16-byte marker (0x8021 eight times), a schema id (0: the
//! chain's canonical registry), the length of the codes, the codes as ASCII. Contracts ignore the
//! extra bytes — ABI decoding stops at the arguments, and no Accordo contract reads msg.data —
//! only Base's indexers look at them. It costs about 450 gas.

use alloy_primitives::Bytes;

/// Accordo's Builder Code (base.dev → Settings → Builder Code).
pub const BUILDER_CODE: &str = "bc_97j8rq9h";

/// Base mainnet, and Base Sepolia for trying it out. Other chains get nothing.
const BASE_CHAIN_IDS: [u64; 2] = [8453, 84532];

const MARKER: [u8; 16] = [0x80, 0x21, 0x80, 0x21, 0x80, 0x21, 0x80, 0x21, 0x80, 0x21, 0x80, 0x21, 0x80, 0x21, 0x80, 0x21];

/// The ERC-8021 schema 0 suffix for these codes (ASCII, comma-free, 255 bytes at most together).
pub fn suffix(codes: &[&str]) -> Vec<u8> {
    let joined = codes.join(",");
    debug_assert!(joined.is_ascii() && joined.len() <= 255 && codes.iter().all(|c| !c.is_empty() && !c.contains(',')));
    let mut out = joined.into_bytes();
    out.push(out.len() as u8);
    out.push(0); // schema 0
    out.extend_from_slice(&MARKER);
    out
}

/// The calldata to send on this chain: on Base, with Accordo's code after it (once).
pub fn tag(chain_id: u64, data: Bytes) -> Bytes {
    if !BASE_CHAIN_IDS.contains(&chain_id) {
        return data;
    }
    let tail = suffix(&[BUILDER_CODE]);
    if data.ends_with(&tail) {
        return data;
    }
    let mut out = data.to_vec();
    out.extend_from_slice(&tail);
    out.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn the_suffix_is_the_one_base_documents() {
        // docs.base.org → Builder Codes: "bc_b7k3p9da produces the suffix 0x6263…8021".
        assert_eq!(hex(&suffix(&["bc_b7k3p9da"])), "62635f62376b33703964610b0080218021802180218021802180218021");
        // Accordo's own, as ox's Attribution.toDataSuffix gives it.
        assert_eq!(hex(&suffix(&[BUILDER_CODE])), "62635f39376a38727139680b0080218021802180218021802180218021");
        // Several codes: comma-joined, the length counts the commas (ERC-8021's "baseapp,morpho").
        assert!(hex(&suffix(&["baseapp", "morpho"])).starts_with("626173656170702c6d6f7270686f0e00"));
    }

    #[test]
    fn only_base_gets_it_and_only_once() {
        let call: Bytes = vec![0xa9, 0x05, 0x9c, 0xbb].into();
        let on_base = tag(8453, call.clone());
        assert_eq!(&on_base[..4], &call[..]);
        assert!(hex(&on_base).ends_with("0b0080218021802180218021802180218021"));
        assert_eq!(tag(8453, on_base.clone()), on_base, "a retry doesn't add it twice");
        assert_eq!(tag(84532, call.clone()).len(), on_base.len());
        for other in [1, 97, 42161, 31337] {
            assert_eq!(tag(other, call.clone()), call, "chain {other}");
        }
        // A plain transfer (no calldata) on Base carries just the suffix.
        assert_eq!(tag(8453, Bytes::new()).to_vec(), suffix(&[BUILDER_CODE]));
    }
}
