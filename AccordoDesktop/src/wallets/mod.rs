// SPDX-License-Identifier: GPL-3.0-or-later
//! Wallet keys (utils/wallet.ts): making a new key, and telling which address a pasted key
//! belongs to. EVM via alloy (the same secp256k1 / keccak / EIP-55 rules as ethers); Solana and
//! TON keys aren't in the desktop client yet.

pub mod evm;

/// Which network an address belongs to (handleAddWallet): 0x… is EVM.
pub fn chain_of(address: &str) -> &'static str {
    if address.trim().starts_with("0x") { "EVM" } else { "Solana" }
}

/// "0x3Dd0…7590": an address or contract shortened the usual way — 4 characters after the
/// prefix, 4 at the end. Anything short (or not an address) stays as it is.
pub fn short_address(address: &str) -> String {
    let a = address.trim();
    let (prefix, body) = match a.get(..2) {
        Some("0x" | "0X") => a.split_at(2),
        _ => ("", a),
    };
    let n = body.chars().count();
    if n <= 10 {
        return a.to_string();
    }
    let head: String = body.chars().take(4).collect();
    let tail: String = body.chars().skip(n - 4).collect();
    format!("{prefix}{head}…{tail}")
}

/// "0x6B2152…b96d9A7" — first 8, last 6 (the web's list).
pub fn short(address: &str) -> String {
    let n = address.chars().count();
    if n <= 16 {
        return address.to_string();
    }
    let head: String = address.chars().take(8).collect();
    let tail: String = address.chars().skip(n - 6).collect();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod short_tests {
    #[test]
    fn short_address() {
        use super::short_address;
        assert_eq!(short_address("0x746Df93c8BaC1645e77a8f6d1234567890abF6d1"), "0x746D…F6d1");
        assert_eq!(short_address("  0x3153eDfaBE7608A97d0Cb0290000000000007590 "), "0x3153…7590");
        assert_eq!(short_address("0x1234"), "0x1234");
        assert_eq!(short_address("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"), "7xKX…gAsU");
    }
}
