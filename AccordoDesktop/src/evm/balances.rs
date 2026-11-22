// SPDX-License-Identifier: GPL-3.0-or-later
//! A wallet's balance of every token in a list (useWalletTokenBalances.ts): the gas coin with one
//! call, the ERC-20s through Multicall3 in big batches. Without Multicall3 (some local chains) the
//! first few tokens one by one. Also a token's own name / symbol / decimals (adding a custom one).

use std::collections::HashMap;

use alloy_primitives::{address, Address, U256};
use alloy_sol_types::{SolCall, SolValue};

use super::abi::{Multicall3, IERC20};
use super::client::Rpc;
use super::errors::TxError;

const MULTICALL3: Address = address!("cA11bde05977b3631167028862bE2a173976CA11");
/// balanceOf calls per eth_call (~700 in 48 KB, like the web).
const BATCH: usize = 700;
const FALLBACK_LIMIT: usize = 50;

/// Balances by token: `Address::ZERO` = the gas coin. Tokens that couldn't be read are absent.
pub async fn of(rpc: &Rpc, owner: Address, tokens: &[Address]) -> HashMap<Address, U256> {
    let mut out = HashMap::new();
    if let Ok(b) = rpc.balance(owner).await {
        out.insert(Address::ZERO, b);
    }
    let mut list: Vec<Address> = tokens.iter().copied().filter(|a| *a != Address::ZERO).collect();
    list.sort();
    list.dedup();
    if list.is_empty() {
        return out;
    }
    let call_data = IERC20::balanceOfCall { account: owner }.abi_encode();
    let mut multicall_ok = true;
    for chunk in list.chunks(BATCH) {
        let calls: Vec<Multicall3::Call3> = chunk.iter().map(|t| Multicall3::Call3 { target: *t, allowFailure: true, callData: call_data.clone().into() }).collect();
        match rpc.read(MULTICALL3, &Multicall3::aggregate3Call { calls }).await {
            Ok(results) => {
                for (t, r) in chunk.iter().zip(results) {
                    if r.success {
                        if let Ok(v) = U256::abi_decode(&r.returnData) {
                            out.insert(*t, v);
                        }
                    }
                }
            }
            Err(_) => {
                multicall_ok = false;
                break;
            }
        }
    }
    if !multicall_ok {
        for t in list.iter().take(FALLBACK_LIMIT) {
            if let Ok(v) = rpc.read(*t, &IERC20::balanceOfCall { account: owner }).await {
                out.insert(*t, v);
            }
        }
    }
    out
}

/// (name, symbol, decimals) read from the token contract itself.
pub async fn token_info(rpc: &Rpc, token: Address) -> Result<(String, String, u8), TxError> {
    let symbol = rpc.read(token, &IERC20::symbolCall {}).await?;
    if symbol.trim().is_empty() {
        return Err(TxError::Other("The contract has no token symbol".into()));
    }
    let name = rpc.read(token, &IERC20::nameCall {}).await.unwrap_or_default();
    let decimals = rpc.read(token, &IERC20::decimalsCall {}).await.unwrap_or(18);
    Ok((name, symbol, decimals))
}

/// Gas a typical Accordo write uses at most (useTokenBalance.ts: TYPICAL_GAS), doubled for fee
/// spikes and L2 data fees.
const TYPICAL_GAS: u64 = 400_000;
const RESERVE_MULTIPLIER: u64 = 2;
/// Floor for the reserve (0.0001 of the gas coin): on L2s the L1 data fee isn't in the gas price.
const MIN_RESERVE: u128 = 100_000_000_000_000;

/// What can be put into a payment (useTokenBalance.ts `spendable`, for 25 % … MAX): a token's whole
/// balance; the gas coin minus a reserve for the transaction's own fee.
pub async fn spendable(rpc: &Rpc, owner: Address, token: Address) -> Result<U256, TxError> {
    if token != Address::ZERO {
        return rpc.read(token, &IERC20::balanceOfCall { account: owner }).await;
    }
    let raw = rpc.balance(owner).await?;
    let per_gas = rpc.call("eth_gasPrice", serde_json::json!([])).await.ok().and_then(|v| super::client::hex_u256(&v)).unwrap_or_default();
    Ok(raw.saturating_sub(gas_reserve(per_gas)))
}

fn gas_reserve(per_gas: U256) -> U256 {
    (per_gas * U256::from(TYPICAL_GAS) * U256::from(RESERVE_MULTIPLIER)).max(U256::from(MIN_RESERVE))
}

/// `pct` % of `amount` (100 → all of it).
pub fn share(amount: U256, pct: u8) -> U256 {
    amount * U256::from(pct.min(100)) / U256::from(100u8)
}

#[cfg(test)]
mod spend_tests {
    use super::*;

    #[test]
    fn the_reserve_has_a_floor_and_follows_gas_price() {
        assert_eq!(gas_reserve(U256::ZERO), U256::from(MIN_RESERVE));
        // 1 gwei × 400k × 2 = 0.0008 ETH, over the floor.
        assert_eq!(gas_reserve(U256::from(1_000_000_000u64)), U256::from(800_000_000_000_000u128));
    }

    #[test]
    fn shares_round_down() {
        assert_eq!(share(U256::from(999u32), 25), U256::from(249u32));
        assert_eq!(share(U256::from(999u32), 100), U256::from(999u32));
        assert_eq!(share(U256::from(999u32), 200), U256::from(999u32));
    }
}
