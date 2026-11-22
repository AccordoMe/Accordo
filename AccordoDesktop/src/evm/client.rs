// SPDX-License-Identifier: GPL-3.0-or-later
//! The EVM plumbing every contract uses (utils/evmClient.ts + txFlow.ts): JSON-RPC over a list of
//! endpoints with failover, reads (eth_call), and sending a transaction from an Accordo wallet —
//! nonce, gas estimate +20%, EIP-1559 fees (legacy gasPrice where the chain has no base fee),
//! signed here with the wallet's key, then waiting for the receipt and failing on a revert, so a
//! caller never reports success for something that didn't happen.
//!
//! The key: `Wallet` takes it from WalletSession::open for one operation and drops it (the signer
//! zeroizes on drop). It is never logged, never sent anywhere — only the signed transaction is.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy_consensus::{SignableTransaction, TxEip1559, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;
use serde_json::{json, Value};

use super::errors::TxError;

const TIMEOUT: Duration = Duration::from_secs(12);
/// How long to wait for a transaction to be mined before giving the hash back as "pending".
const RECEIPT_WAIT: Duration = Duration::from_secs(300);
/// Gas limit safety buffer (evmClient.ts: gasBufferPercentage = 20).
const GAS_BUFFER_PCT: u64 = 20;

/// Where a transaction is (the web's TxStage): the UI shows "Approving token…" etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// An ERC-20 approve before the real transaction.
    Approving,
    /// Signing and sending.
    Signing,
    /// Sent, waiting for the block.
    Confirming,
}

/// Progress callback: (stage, the tx hash once known).
pub type Progress = Arc<dyn Fn(Stage, Option<B256>) + Send + Sync>;

/// JSON-RPC to one network over its endpoints: the last one that answered goes first, a dead one
/// is skipped (the web ranks them with viem's fallback transport).
#[derive(Clone)]
pub struct Rpc {
    urls: Arc<Vec<String>>,
    good: Arc<AtomicUsize>,
    http: reqwest::Client,
    /// The network's id when known: an endpoint serving another chain is refused before signing.
    pub expect_chain: Option<u64>,
}

impl Rpc {
    pub fn new(urls: Vec<String>, expect_chain: Option<u64>) -> Result<Self, TxError> {
        let urls: Vec<String> = urls.into_iter().filter(|u| u.starts_with("http://") || u.starts_with("https://")).collect();
        if urls.is_empty() {
            return Err(TxError::NoRpc);
        }
        let http = reqwest::Client::builder().timeout(TIMEOUT).build().map_err(|e| TxError::Network(e.to_string()))?;
        Ok(Self { urls: Arc::new(urls), good: Arc::new(AtomicUsize::new(0)), http, expect_chain })
    }

    /// One call; transport failures move on to the next endpoint, RPC errors (a revert, a bad
    /// nonce) are the answer and come back as they are.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, TxError> {
        let n = self.urls.len();
        let start = self.good.load(Ordering::Relaxed) % n;
        let mut last = TxError::Network("no endpoint answered".into());
        for i in 0..n {
            let at = (start + i) % n;
            let url = &self.urls[at];
            let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
            let res = match self.http.post(url).json(&body).send().await {
                Ok(r) if r.status().is_success() || r.status().as_u16() == 400 => r,
                Ok(r) => {
                    last = TxError::Network(format!("HTTP {}", r.status().as_u16()));
                    continue;
                }
                Err(e) => {
                    last = TxError::Network(if e.is_timeout() { "the node didn't answer in time".into() } else { "can't reach the node".into() });
                    continue;
                }
            };
            let Ok(v) = res.json::<Value>().await else {
                last = TxError::Network("not a JSON-RPC answer".into());
                continue;
            };
            self.good.store(at, Ordering::Relaxed);
            if let Some(err) = v.get("error") {
                return Err(TxError::Rpc {
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: err.get("message").and_then(Value::as_str).unwrap_or("RPC error").to_string(),
                    data: revert_data(err),
                });
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(last)
    }

    pub async fn chain_id(&self) -> Result<u64, TxError> {
        hex_u64(&self.call("eth_chainId", json!([])).await?).ok_or_else(|| TxError::Network("odd chain id".into()))
    }

    /// eth_call → the raw return data.
    pub async fn eth_call(&self, to: Address, data: Bytes) -> Result<Bytes, TxError> {
        let v = self.call("eth_call", json!([{"to": to, "data": data}, "latest"])).await?;
        parse_bytes(&v).ok_or_else(|| TxError::Network("odd eth_call answer".into()))
    }

    /// A typed view call: encodes, calls, decodes (`C::Return`).
    pub async fn read<C: SolCall>(&self, to: Address, call: &C) -> Result<C::Return, TxError> {
        let out = self.eth_call(to, call.abi_encode().into()).await?;
        if out.is_empty() {
            return Err(TxError::Empty);
        }
        C::abi_decode_returns(&out).map_err(|_| TxError::Decode)
    }

    pub async fn code(&self, at: Address) -> Result<Bytes, TxError> {
        let v = self.call("eth_getCode", json!([at, "latest"])).await?;
        parse_bytes(&v).ok_or(TxError::Decode)
    }

    pub async fn balance(&self, of: Address) -> Result<U256, TxError> {
        let v = self.call("eth_getBalance", json!([of, "latest"])).await?;
        hex_u256(&v).ok_or(TxError::Decode)
    }

    /// Waits for the receipt; Ok(hash) only if the transaction succeeded.
    pub async fn wait(&self, hash: B256) -> Result<B256, TxError> {
        let started = std::time::Instant::now();
        let mut delay = Duration::from_millis(250);
        loop {
            let r = self.call("eth_getTransactionReceipt", json!([hash])).await;
            if let Ok(receipt) = &r {
                if !receipt.is_null() {
                    return match receipt.get("status").and_then(Value::as_str) {
                        Some("0x1") => Ok(hash),
                        _ => Err(TxError::Reverted(hash)),
                    };
                }
            }
            if started.elapsed() > RECEIPT_WAIT {
                return Err(TxError::Pending(hash));
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(2));
        }
    }
}

/// A revert's data from an RPC error ({"data": "0x…"} or {"data": {"data": "0x…"}}).
fn revert_data(err: &Value) -> Option<Bytes> {
    let d = err.get("data")?;
    let s = d.as_str().or_else(|| d.get("data").and_then(Value::as_str))?;
    parse_bytes(&Value::String(s.to_string())).filter(|b| b.len() >= 4)
}

fn parse_bytes(v: &Value) -> Option<Bytes> {
    let s = v.as_str()?.strip_prefix("0x")?;
    alloy_primitives::hex::decode(s).ok().map(Bytes::from)
}

pub fn hex_u64(v: &Value) -> Option<u64> {
    u64::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

fn hex_u128(v: &Value) -> Option<u128> {
    u128::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

pub fn hex_u256(v: &Value) -> Option<U256> {
    U256::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// One transaction to send.
#[derive(Debug, Clone)]
pub struct Call {
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
}

impl Call {
    pub fn new<C: SolCall>(to: Address, call: &C) -> Self {
        Self { to, data: call.abi_encode().into(), value: U256::ZERO }
    }

    pub fn with_value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }
}

/// An Accordo wallet able to send on one network.
pub struct Wallet {
    signer: PrivateKeySigner,
    pub rpc: Rpc,
    pub progress: Option<Progress>,
}

impl std::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wallet").field("address", &self.signer.address()).finish_non_exhaustive()
    }
}

impl Wallet {
    /// `key`: "0x" + 64 hex, as kept in the vault.
    pub fn new(key: &str, rpc: Rpc) -> Result<Self, TxError> {
        let signer = crate::wallets::evm::signer(key).ok_or(TxError::BadKey)?;
        Ok(Self { signer, rpc, progress: None })
    }

    pub fn with_progress(mut self, p: Option<Progress>) -> Self {
        self.progress = p;
        self
    }

    pub fn address(&self) -> Address {
        self.signer.address()
    }

    fn stage(&self, s: Stage, hash: Option<B256>) {
        if let Some(p) = &self.progress {
            p(s, hash);
        }
    }

    /// Builds, signs, sends and waits (buildUniversalTransaction + sendTransaction + receipt).
    pub async fn send(&self, call: Call, stage: Stage) -> Result<B256, TxError> {
        self.stage(stage, None);
        let rpc = &self.rpc;
        let from = self.address();
        let chain_id = rpc.chain_id().await?;
        if let Some(want) = rpc.expect_chain {
            if want != chain_id {
                return Err(TxError::WrongChain { want, got: chain_id });
            }
        }
        let tx = json!({"from": from, "to": call.to, "data": call.data, "value": format!("{:#x}", call.value)});
        let nonce = hex_u64(&rpc.call("eth_getTransactionCount", json!([from, "pending"])).await?).ok_or(TxError::Decode)?;
        let estimate = hex_u64(&rpc.call("eth_estimateGas", json!([tx])).await?).ok_or(TxError::Decode)?;
        let gas_limit = estimate.saturating_mul(100 + GAS_BUFFER_PCT) / 100;
        let block = rpc.call("eth_getBlockByNumber", json!(["latest", false])).await?;
        let raw = match block.get("baseFeePerGas").and_then(hex_u128) {
            Some(base) => {
                // ethers' rule: 2 × base + tip — the unused part isn't charged.
                let tip = match rpc.call("eth_maxPriorityFeePerGas", json!([])).await {
                    Ok(v) => hex_u128(&v).unwrap_or(1_000_000_000),
                    Err(_) => 1_000_000_000,
                };
                let tx = TxEip1559 {
                    chain_id,
                    nonce,
                    gas_limit,
                    max_fee_per_gas: base.saturating_mul(2).saturating_add(tip),
                    max_priority_fee_per_gas: tip,
                    to: TxKind::Call(call.to),
                    value: call.value,
                    access_list: Default::default(),
                    input: call.data,
                };
                let sig = self.signer.sign_hash_sync(&tx.signature_hash()).map_err(|_| TxError::BadKey)?;
                tx.into_signed(sig).encoded_2718()
            }
            None => {
                let price = hex_u128(&rpc.call("eth_gasPrice", json!([])).await?).ok_or(TxError::Decode)?;
                let tx = TxLegacy { chain_id: Some(chain_id), nonce, gas_price: price.saturating_mul(11) / 10, gas_limit, to: TxKind::Call(call.to), value: call.value, input: call.data };
                let sig = self.signer.sign_hash_sync(&tx.signature_hash()).map_err(|_| TxError::BadKey)?;
                tx.into_signed(sig).encoded_2718()
            }
        };
        let sent = rpc.call("eth_sendRawTransaction", json!([Bytes::from(raw)])).await?;
        let hash: B256 = sent.as_str().and_then(|s| s.parse().ok()).ok_or(TxError::Decode)?;
        if stage == Stage::Signing {
            self.stage(Stage::Confirming, Some(hash));
        }
        rpc.wait(hash).await
    }

    /// ERC-20 allowance check, and an approve of exactly `amount` when it's too low.
    pub async fn ensure_allowance(&self, token: Address, spender: Address, amount: U256) -> Result<(), TxError> {
        if token == Address::ZERO {
            return Ok(());
        }
        let current = self.rpc.read(token, &super::abi::IERC20::allowanceCall { owner: self.address(), spender }).await.unwrap_or(U256::ZERO);
        if current < amount {
            self.send(Call::new(token, &super::abi::IERC20::approveCall { spender, amount }), Stage::Approving).await?;
        }
        Ok(())
    }
}

/// 32 random bytes (secrets, salts, ids).
pub fn random_b256() -> B256 {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("system randomness");
    B256::from(b)
}

/// "0x…" → an address, as ethers' getAddress: all-lowercase / all-uppercase is taken as it is,
/// mixed case must be a valid checksum (a mistyped character is caught, not sent money to).
pub fn address(s: &str) -> Result<Address, TxError> {
    let s = s.trim();
    let bad = || TxError::BadAddress(s.to_string());
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    let mixed = hex.chars().any(|c| c.is_ascii_lowercase()) && hex.chars().any(|c| c.is_ascii_uppercase());
    if mixed {
        Address::parse_checksummed(format!("0x{hex}"), None).map_err(|_| bad())
    } else {
        format!("0x{hex}").parse::<Address>().map_err(|_| bad())
    }
}

/// A token address, "" / None / 0x0 → the native coin.
pub fn token_or_native(t: Option<&str>) -> Result<Address, TxError> {
    match t.map(str::trim).filter(|t| !t.is_empty()) {
        None => Ok(Address::ZERO),
        Some(t) => address(t),
    }
}

/// "1.5" with 18 decimals → 1500000000000000000 (parseUnits); too many decimals is an error.
pub fn parse_units(amount: &str, decimals: u8) -> Result<U256, TxError> {
    let a = amount.trim();
    let bad = || TxError::BadAmount(a.to_string());
    let (whole, frac) = a.split_once('.').unwrap_or((a, ""));
    if whole.is_empty() && frac.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > decimals as usize {
        return Err(bad());
    }
    let digits = format!("{whole}{frac:0<width$}", width = decimals as usize);
    U256::from_str_radix(if digits.is_empty() { "0" } else { &digits }, 10).map_err(|_| bad())
}

/// 1500000000000000000 with 18 decimals → "1.5" (formatUnits, trailing zeros dropped).
pub fn format_units(v: U256, decimals: u8) -> String {
    let s = v.to_string();
    let d = decimals as usize;
    if d == 0 {
        return s;
    }
    let padded = format!("{s:0>width$}", width = d + 1);
    let (whole, frac) = padded.split_at(padded.len() - d);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() { whole.to_string() } else { format!("{whole}.{frac}") }
}

#[cfg(test)]
mod tests {
    #[test]
    fn checksums() {
        use super::address;
        let good = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
        assert!(address(good).is_ok());
        assert!(address(&good.to_lowercase()).is_ok());
        assert!(address("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02914").is_err()); // one char off, mixed case
        assert!(address("833589fcd6edb6e08f4c7c32d4f71b54bda02913").is_ok()); // ethers takes it without 0x too
        assert!(address("0x1234").is_err());
    }

    use super::*;

    #[test]
    fn units_round_trip() {
        assert_eq!(parse_units("1.5", 18).unwrap(), U256::from(1_500_000_000_000_000_000u128));
        assert_eq!(parse_units("100000", 6).unwrap(), U256::from(100_000_000_000u64));
        assert_eq!(parse_units(".5", 6).unwrap(), U256::from(500_000u64));
        assert!(parse_units("1.0000001", 6).is_err());
        assert!(parse_units("1e5", 18).is_err());
        assert!(parse_units("", 18).is_err());
        assert_eq!(format_units(U256::from(1_500_000_000_000_000_000u128), 18), "1.5");
        assert_eq!(format_units(U256::from(5u8), 6), "0.000005");
        assert_eq!(format_units(U256::from(7_000_000u64), 6), "7");
    }
}
