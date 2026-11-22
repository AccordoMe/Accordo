// SPDX-License-Identifier: GPL-3.0-or-later
//! Talking to a network's RPC directly: checking an endpoint (Networks window → Test), and the
//! faucet of the developer's local chain (LocalFaucet.tsx). Plain JSON-RPC over HTTP; nothing
//! here signs anything or touches a wallet key.

use std::time::{Duration, Instant};

use alloy_primitives::U256;
use serde_json::{json, Value};

/// Anvil account #0: deployed the local test tokens (so it can mint them) and is unlocked by
/// Anvil itself — eth_sendTransaction from it needs no key. Only meaningful on the local chain.
const ANVIL_DEPLOYER: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
/// mint(address,uint256)
const MINT_SELECTOR: &str = "40c10f19";
const TIMEOUT: Duration = Duration::from_secs(8);

/// What a working endpoint answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub ms: u128,
    /// Latest block (EVM) or slot (Solana).
    pub height: Option<u64>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(TIMEOUT).build().map_err(|e| e.to_string())
}

async fn call(http: &reqwest::Client, url: &str, method: &str, params: Value) -> Result<Value, String> {
    let res = http
        .post(url)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "no answer in 8 s".to_string() } else if e.is_connect() { "can't connect".to_string() } else { "request failed".to_string() })?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    let body: Value = res.json().await.map_err(|_| "not a JSON-RPC answer".to_string())?;
    if let Some(err) = body.get("error") {
        return Err(err.get("message").and_then(Value::as_str).unwrap_or("RPC error").chars().take(120).collect());
    }
    body.get("result").cloned().ok_or_else(|| "no result".to_string())
}

fn hex_u64(v: &Value) -> Option<u64> {
    u64::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// Asks the endpoint which chain it serves and its latest block (Solana: health and slot).
/// `chain_id`: the network's decimal id — an EVM endpoint of another chain is an error.
pub async fn probe(url: String, solana: bool, chain_id: String) -> Result<Probe, String> {
    if url.starts_with("ws://") || url.starts_with("wss://") {
        return Err("WebSocket endpoints aren't tested".into());
    }
    let http = client()?;
    let started = Instant::now();
    if solana {
        call(&http, &url, "getHealth", json!([])).await?;
        let ms = started.elapsed().as_millis();
        let height = call(&http, &url, "getSlot", json!([])).await.ok().and_then(|v| v.as_u64());
        return Ok(Probe { ms, height });
    }
    let id = call(&http, &url, "eth_chainId", json!([])).await?;
    let ms = started.elapsed().as_millis();
    let served = hex_u64(&id).ok_or("odd chain id")?;
    if chain_id.parse::<u64>().is_ok_and(|want| want != served) {
        return Err(format!("serves another chain (ID {served})"));
    }
    let height = call(&http, &url, "eth_blockNumber", json!([])).await.ok().as_ref().and_then(hex_u64);
    Ok(Probe { ms, height })
}

/// What the faucet hands out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drip {
    /// +1000 of the chain's coin (Solana: +100 SOL airdrop).
    Native { symbol: String },
    /// +100,000 of a test token deployed by the Anvil deployer.
    Token { symbol: String, address: String, decimals: u8 },
}

impl Drip {
    pub fn label(&self, solana: bool) -> String {
        match self {
            Drip::Native { symbol } => format!("+{} {symbol}", if solana { 100 } else { 1000 }),
            Drip::Token { symbol, .. } => format!("+100,000 {symbol}"),
        }
    }
}

fn units(whole: u64, decimals: u8) -> U256 {
    U256::from(whole) * U256::from(10u8).pow(U256::from(decimals))
}

/// mint(to, amount) calldata.
fn mint_data(to: &str, amount: U256) -> Result<String, String> {
    let addr = to.trim_start_matches("0x");
    if addr.len() != 40 || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("not an EVM address".into());
    }
    Ok(format!("0x{MINT_SELECTOR}{:0>64}{:064x}", addr.to_lowercase(), amount))
}

/// Tops up `to` on the local chain at `url`. Ok: the label of what arrived ("+1000 ETH").
pub async fn drip(url: String, solana: bool, to: String, what: Drip) -> Result<String, String> {
    let http = client()?;
    let label = what.label(solana);
    if solana {
        let sig = call(&http, &url, "requestAirdrop", json!([to, 100u64 * 1_000_000_000])).await?;
        for _ in 0..40 {
            let st = call(&http, &url, "getSignatureStatuses", json!([[sig]])).await?;
            if st.pointer("/value/0/confirmationStatus").is_some_and(|v| !v.is_null()) {
                return Ok(label);
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        return Err("the airdrop wasn't confirmed".into());
    }
    match what {
        Drip::Native { .. } => {
            let now = call(&http, &url, "eth_getBalance", json!([to, "latest"])).await?;
            let current = U256::from_str_radix(now.as_str().unwrap_or("0x0").trim_start_matches("0x"), 16).map_err(|_| "odd balance")?;
            let next = current + units(1000, 18);
            call(&http, &url, "anvil_setBalance", json!([to, format!("{next:#x}")])).await?;
            // setBalance makes no block, and wallets re-read balances on a new one.
            call(&http, &url, "evm_mine", json!([])).await?;
        }
        Drip::Token { address, decimals, .. } => {
            let data = mint_data(&to, units(100_000, decimals))?;
            let hash = call(&http, &url, "eth_sendTransaction", json!([{"from": ANVIL_DEPLOYER, "to": address, "data": data}])).await?;
            let receipt = call(&http, &url, "eth_getTransactionReceipt", json!([hash])).await?;
            if receipt.get("status").and_then(Value::as_str).is_some_and(|s| s != "0x1") {
                return Err("mint reverted".into());
            }
        }
    }
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_calldata() {
        let d = mint_data("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266", units(1, 6)).unwrap();
        assert_eq!(d, "0x40c10f19000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb9226600000000000000000000000000000000000000000000000000000000000f4240");
        assert!(mint_data("0x123", U256::ZERO).is_err());
    }

    #[test]
    fn selector_is_mint() {
        let h = alloy_primitives::keccak256(b"mint(address,uint256)");
        assert_eq!(&alloy_primitives::hex::encode(&h[..4]), MINT_SELECTOR);
    }
}
