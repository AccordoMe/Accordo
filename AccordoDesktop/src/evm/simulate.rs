// SPDX-License-Identifier: GPL-3.0-or-later
//! The review window's simulation, as a wallet app does it before you sign: the transactions a
//! flow would send (a dry run, `Wallet::planner`) are run against the chain's current state with
//! `eth_simulateV1` — the permission and the real transaction together, the second seeing the
//! first — and the window gets back whether they go through (or the contract's reason why not),
//! what leaves and enters the wallet, the network fee, and whether the wallet can pay for it all.
//!
//! The wallet is given a large balance for the run, so a short wallet still shows what would
//! happen; whether it can really pay is checked apart: amount + gas limit × max fee against the
//! balance, as the node checks it when the transaction arrives. A node without `eth_simulateV1`
//! gets the older check: each transaction's gas estimated on its own.

use alloy_primitives::{address, b256, Address, Bytes, B256, U256};
use serde_json::{json, Value};

use super::builder_code;
use super::client::{format_units, hex_u256, hex_u64, Planned, Rpc, GAS_BUFFER_PCT};
use super::errors::TxError;

/// ERC-20 Transfer(address,address,uint256) — and the native transfers the node traces.
const TRANSFER: B256 = b256!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
/// Where `traceTransfers` puts native-coin moves (ERC-7528).
const NATIVE: Address = address!("EeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE");

/// One transaction of the plan.
#[derive(Debug, Clone)]
pub struct Step {
    /// The ERC-20 permission before the real transaction.
    pub approve: bool,
    /// Gas it used in the simulation (None: couldn't be worked out).
    pub gas: Option<u64>,
    /// Why it would fail, in words.
    pub error: Option<String>,
}

/// What one asset does in the wallet. The network fee is apart (`Outcome::fee_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Address::ZERO: the network's own coin.
    pub token: Address,
    pub symbol: String,
    pub decimals: u8,
    pub gained: U256,
    pub spent: U256,
}

impl Change {
    /// Leaves the wallet overall.
    pub fn outgoing(&self) -> bool {
        self.spent > self.gained
    }

    /// "−950 USDT", "+0.5 ETH".
    pub fn text(&self) -> String {
        let (sign, n) = if self.outgoing() { ("−", self.spent - self.gained) } else { ("+", self.gained - self.spent) };
        format!("{sign}{} {}", format_units(n, self.decimals), self.symbol)
    }
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub steps: Vec<Step>,
    /// Net per asset, in the order they first moved (nothing that comes back even).
    pub changes: Vec<Change>,
    /// Gas used × base fee + tip: what it most likely costs.
    pub fee_likely: U256,
    /// Gas limit × max fee: the most it can cost (what the wallet must hold).
    pub fee_max: U256,
    /// Native coin sent along with the transactions.
    pub value: U256,
    pub balance: U256,
    /// "ETH", "BNB"…
    pub native: String,
    /// The node can't simulate: gas checked one transaction at a time, no balance changes.
    pub partial: bool,
}

impl Outcome {
    /// Why it would fail, from the first transaction that does.
    pub fn error(&self) -> Option<&str> {
        self.steps.iter().find_map(|s| s.error.as_deref())
    }

    /// How much native coin is missing for the amount plus the most the gas can cost.
    pub fn short_of(&self) -> Option<U256> {
        let need = self.value.saturating_add(self.fee_max);
        (need > self.balance).then(|| need - self.balance)
    }

    /// Confirm would only fail: the contract says no, or the wallet can't pay.
    pub fn blocks(&self) -> bool {
        self.error().is_some() || self.short_of().is_some()
    }

    pub fn fmt_native(&self, v: U256) -> String {
        format!("{} {}", round(&format_units(v, 18), 6), self.native)
    }
}

/// "0.000012345678" → "0.000012" (6 significant decimals after the leading zeros, for fees).
fn round(s: &str, keep: usize) -> String {
    let Some((whole, frac)) = s.split_once('.') else { return s.to_string() };
    let zeros = frac.len() - frac.trim_start_matches('0').len();
    let cut = if whole == "0" { zeros + keep.min(4) } else { keep };
    let frac = &frac[..frac.len().min(cut)];
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() { whole.to_string() } else { format!("{whole}.{frac}") }
}

/// Simulates the plan from `from` on the network `rpc` serves.
pub async fn run(rpc: &Rpc, from: Address, plan: Vec<Planned>, native: String) -> Result<Outcome, TxError> {
    let chain_id = rpc.chain_id().await?;
    if let Some(want) = rpc.expect_chain {
        if want != chain_id {
            return Err(TxError::WrongChain { want, got: chain_id });
        }
    }
    // Exactly what `Wallet::send` would put on the wire, Builder Code included.
    let calls: Vec<Value> = plan
        .iter()
        .map(|p| json!({"from": from, "to": p.call.to, "data": builder_code::tag(chain_id, p.call.data.clone()), "value": format!("{:#x}", p.call.value)}))
        .collect();
    let value = plan.iter().fold(U256::ZERO, |a, p| a.saturating_add(p.call.value));
    let (balance, pricing) = (rpc.balance(from).await?, rpc.pricing().await?);

    let rich = format!("{:#x}", U256::from(10).pow(U256::from(30)));
    let request = json!([{
        "blockStateCalls": [{"stateOverrides": {format!("{from:#x}"): {"balance": rich}}, "calls": calls}],
        "traceTransfers": true,
        "validation": false,
    }, "latest"]);
    let (steps, logs, partial) = match rpc.call("eth_simulateV1", request).await {
        Ok(v) => {
            let (steps, logs) = read_simulation(&v, &plan)?;
            (steps, logs, false)
        }
        Err(e) if unsupported(&e) => (estimate_each(rpc, &calls, &plan).await, Vec::new(), true),
        Err(e) => return Err(e),
    };

    // A transaction that doesn't depend on an earlier one of the plan is estimated the way
    // `send` will estimate it: on some networks (Arbitrum) that includes the L1 part.
    let mut steps = steps;
    if !partial && steps.iter().all(|s| s.error.is_none()) {
        for (i, step) in steps.iter_mut().enumerate() {
            if plan[..i].iter().any(|p| p.approve) {
                break;
            }
            if let Ok(est) = rpc.call("eth_estimateGas", json!([calls[i]])).await {
                if let Some(g) = hex_u64(&est) {
                    step.gas = Some(step.gas.map_or(g, |s| s.max(g)));
                }
            }
        }
    }

    let (mut fee_likely, mut fee_max) = (U256::ZERO, U256::ZERO);
    for gas in steps.iter().filter_map(|s| s.gas) {
        let limit = gas.saturating_mul(100 + GAS_BUFFER_PCT) / 100;
        fee_likely = fee_likely.saturating_add(U256::from(gas) * U256::from(pricing.likely()));
        fee_max = fee_max.saturating_add(U256::from(limit) * U256::from(pricing.max_fee()));
    }
    let changes = name_changes(rpc, net_changes(from, &logs), &native).await;
    Ok(Outcome { steps, changes, fee_likely, fee_max, value, balance, native, partial })
}

/// The node doesn't know eth_simulateV1 (or refuses it).
fn unsupported(e: &TxError) -> bool {
    match e {
        TxError::Rpc { code, message, .. } => {
            let m = message.to_lowercase();
            *code == -32601 || ["not found", "not supported", "does not exist", "unsupported", "not available", "unknown method", "not whitelisted", "not allowed"].iter().any(|w| m.contains(w))
        }
        _ => false,
    }
}

/// One step per planned transaction, up to the first that fails; and every log of the ones
/// that went through.
fn read_simulation(v: &Value, plan: &[Planned]) -> Result<(Vec<Step>, Vec<Value>), TxError> {
    let calls = v.get(0).and_then(|b| b.get("calls")).and_then(Value::as_array).ok_or(TxError::Decode)?;
    let mut steps = Vec::new();
    let mut logs = Vec::new();
    for (p, c) in plan.iter().zip(calls) {
        let gas = ["maxUsedGas", "gasUsed"].iter().filter_map(|k| c.get(*k).and_then(hex_u64)).max();
        let ok = c.get("status").and_then(Value::as_str) == Some("0x1");
        let error = (!ok).then(|| {
            let e = c.get("error").cloned().unwrap_or(Value::Null);
            let data = e.get("data").and_then(Value::as_str).and_then(|s| alloy_primitives::hex::decode(s.trim_start_matches("0x")).ok()).filter(|d| d.len() >= 4).map(Bytes::from);
            let message = e.get("message").and_then(Value::as_str).unwrap_or("execution reverted").to_string();
            TxError::Rpc { code: e.get("code").and_then(Value::as_i64).unwrap_or(3), message, data }.to_string()
        });
        if ok {
            logs.extend(c.get("logs").and_then(Value::as_array).into_iter().flatten().cloned());
        }
        let failed = error.is_some();
        steps.push(Step { approve: p.approve, gas, error });
        if failed {
            break;
        }
    }
    Ok((steps, logs))
}

/// No eth_simulateV1: each transaction's gas on its own. One that needs the permission before it
/// can't be checked until the permission is on-chain — its gas stays unknown.
async fn estimate_each(rpc: &Rpc, calls: &[Value], plan: &[Planned]) -> Vec<Step> {
    let mut steps = Vec::new();
    for (i, p) in plan.iter().enumerate() {
        if plan[..i].iter().any(|q| q.approve) {
            steps.push(Step { approve: p.approve, gas: None, error: None });
            continue;
        }
        match rpc.call("eth_estimateGas", json!([calls[i]])).await {
            Ok(v) => steps.push(Step { approve: p.approve, gas: hex_u64(&v), error: None }),
            // Short of coin is the balance check's to say, with the numbers.
            Err(e) if e.to_string().starts_with("Not enough native coin") => steps.push(Step { approve: p.approve, gas: None, error: None }),
            Err(e) => {
                steps.push(Step { approve: p.approve, gas: None, error: Some(e.to_string()) });
                break;
            }
        }
    }
    steps
}

/// What moved in and out of `wallet`, per asset, from the Transfer logs (ERC-20 ones and the
/// node's traced native transfers). ERC-721 transfers (the id indexed: 4 topics) aren't counted.
fn net_changes(wallet: Address, logs: &[Value]) -> Vec<(Address, U256, U256)> {
    let mut out: Vec<(Address, U256, U256)> = Vec::new();
    for log in logs {
        let topics: Vec<B256> = log.get("topics").and_then(Value::as_array).into_iter().flatten().filter_map(|t| t.as_str()?.parse().ok()).collect();
        if topics.len() != 3 || topics[0] != TRANSFER {
            continue;
        }
        let Some(token) = log.get("address").and_then(Value::as_str).and_then(|a| a.parse::<Address>().ok()) else { continue };
        let token = if token == NATIVE { Address::ZERO } else { token };
        let (src, dst) = (Address::from_word(topics[1]), Address::from_word(topics[2]));
        let amount = log.get("data").and_then(hex_u256).unwrap_or(U256::ZERO);
        if src != wallet && dst != wallet || src == dst {
            continue;
        }
        let i = match out.iter().position(|(t, _, _)| *t == token) {
            Some(i) => i,
            None => {
                out.push((token, U256::ZERO, U256::ZERO));
                out.len() - 1
            }
        };
        if dst == wallet {
            out[i].1 = out[i].1.saturating_add(amount);
        } else {
            out[i].2 = out[i].2.saturating_add(amount);
        }
    }
    out.retain(|(_, gained, spent)| gained != spent);
    out
}

/// The tokens' symbols and decimals, read from them.
async fn name_changes(rpc: &Rpc, raw: Vec<(Address, U256, U256)>, native: &str) -> Vec<Change> {
    let mut out = Vec::new();
    for (token, gained, spent) in raw {
        let (symbol, decimals) = if token == Address::ZERO {
            (native.to_string(), 18)
        } else {
            let symbol = rpc.read(token, &super::abi::IERC20::symbolCall {}).await.ok().filter(|s| !s.trim().is_empty());
            let decimals = rpc.read(token, &super::abi::IERC20::decimalsCall {}).await.ok();
            match decimals {
                Some(d) => (symbol.unwrap_or_else(|| crate::wallets::short_address(&format!("{token:#x}"))), d),
                // Unknown decimals: the raw units, named so.
                None => (format!("units of {}", crate::wallets::short_address(&format!("{token:#x}"))), 0),
            }
        };
        out.push(Change { token, symbol, decimals, gained, spent });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm::client::Call;

    fn word(a: Address) -> String {
        format!("{:#x}", a.into_word())
    }

    #[test]
    fn a_simulation_answer_becomes_steps_and_wallet_changes() {
        let me = address!("3Dd0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7590");
        let usdc = address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
        let cheques = address!("5B94000000000000000000000000000000A60D00");
        let plan = vec![
            Planned { call: Call { to: usdc, data: Bytes::new(), value: U256::ZERO }, approve: true },
            Planned { call: Call { to: cheques, data: Bytes::new(), value: U256::from(5u8) }, approve: false },
        ];
        let answer = json!([{"calls": [
            {"status": "0x1", "gasUsed": "0xb411", "logs": []},
            {"status": "0x1", "gasUsed": "0x1d4c0", "maxUsedGas": "0x1e000", "logs": [
                {"address": format!("{usdc:#x}"), "topics": [format!("{TRANSFER:#x}"), word(me), word(cheques)], "data": format!("{:#066x}", 950_000_000u64)},
                {"address": format!("{NATIVE:#x}"), "topics": [format!("{TRANSFER:#x}"), word(me), word(cheques)], "data": format!("{:#066x}", 5u8)},
                // An NFT (4 topics) and someone else's transfer don't count.
                {"address": format!("{usdc:#x}"), "topics": [format!("{TRANSFER:#x}"), word(me), word(cheques), format!("{:#066x}", 1u8)], "data": "0x"},
                {"address": format!("{usdc:#x}"), "topics": [format!("{TRANSFER:#x}"), word(cheques), word(usdc)], "data": format!("{:#066x}", 7u8)},
            ]},
        ]}]);
        let (steps, logs) = read_simulation(&answer, &plan).unwrap();
        assert_eq!(steps.len(), 2);
        assert!(steps[0].approve && steps[0].gas == Some(0xb411) && steps[0].error.is_none());
        assert_eq!(steps[1].gas, Some(0x1e000), "the higher of maxUsedGas and gasUsed");
        let changes = net_changes(me, &logs);
        assert_eq!(changes, vec![(usdc, U256::ZERO, U256::from(950_000_000u64)), (Address::ZERO, U256::ZERO, U256::from(5u8))]);
        let c = Change { token: usdc, symbol: "USDC".into(), decimals: 6, gained: U256::ZERO, spent: U256::from(950_000_000u64) };
        assert_eq!(c.text(), "−950 USDC");
    }

    #[test]
    fn a_revert_stops_the_plan_with_the_contracts_reason() {
        let plan = vec![Planned { call: Call { to: Address::ZERO, data: Bytes::new(), value: U256::ZERO }, approve: false }];
        // ERC20InsufficientBalance(address,uint256,uint256) as the node returns it.
        let sel = &alloy_primitives::keccak256("ERC20InsufficientBalance(address,uint256,uint256)")[..4];
        let data = format!("0x{}{}", alloy_primitives::hex::encode(sel), "00".repeat(96));
        let answer = json!([{"calls": [{"status": "0x0", "gasUsed": "0x8ccc", "logs": [], "error": {"code": 3, "message": "execution reverted", "data": data}}]}]);
        let (steps, logs) = read_simulation(&answer, &plan).unwrap();
        assert_eq!(steps[0].error.as_deref(), Some("Not enough tokens in this wallet for that amount."));
        assert!(logs.is_empty());
    }

    #[test]
    fn the_wallet_must_hold_the_amount_and_the_most_the_gas_can_cost() {
        let o = |balance: u64| Outcome {
            steps: vec![Step { approve: false, gas: Some(21_000), error: None }],
            changes: vec![],
            fee_likely: U256::from(30u8),
            fee_max: U256::from(50u8),
            value: U256::from(100u8),
            balance: U256::from(balance),
            native: "ETH".into(),
            partial: false,
        };
        assert_eq!(o(149).short_of(), Some(U256::from(1u8)));
        assert!(o(149).blocks());
        assert_eq!(o(150).short_of(), None);
        assert!(!o(150).blocks());
        assert_eq!(round("0.000012345678", 6), "0.00001234");
        assert_eq!(round("1.23456789", 6), "1.234567");
        assert_eq!(round("2", 6), "2");
    }
}

/// Against a real network (cargo test live_simulate -- --ignored): a cheque made dry from an empty
/// wallet — the native one goes through but the wallet can't pay; the USDC one plans the
/// permission first, then the contract refuses for lack of tokens.
/// ACCORDO_TEST_SIM_RPC=https://mainnet.base.org ACCORDO_TEST_SIM_CHEQUES=0x… ACCORDO_TEST_SIM_TOKEN=<USDC>
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn live_simulate() {
    use crate::evm::client::{Rpc, Wallet};
    let (Ok(url), Ok(cheques), Ok(usdc)) = (std::env::var("ACCORDO_TEST_SIM_RPC"), std::env::var("ACCORDO_TEST_SIM_CHEQUES"), std::env::var("ACCORDO_TEST_SIM_TOKEN")) else {
        eprintln!("set ACCORDO_TEST_SIM_RPC, ACCORDO_TEST_SIM_CHEQUES and ACCORDO_TEST_SIM_TOKEN");
        return;
    };
    let rpc = Rpc::new(vec![url], None).unwrap();
    let (contract, token): (Address, Address) = (cheques.parse().unwrap(), usdc.parse().unwrap());
    let empty = Address::repeat_byte(0x5a);
    let to = Address::repeat_byte(0x7b);
    let expires = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 86_400;

    // Native: no permission, one transaction; it would go through, but the wallet has nothing.
    let (w, plan) = Wallet::planner(empty, rpc.clone());
    let r = super::cheques::create(&w, contract, super::cheques::new_v4().1, vec![to], U256::from(10u64.pow(15)), Address::ZERO, expires, 100).await;
    assert!(matches!(r, Err(TxError::Planned)), "{r:?}");
    let plan = plan.lock().unwrap().clone();
    assert_eq!(plan.len(), 1);
    let o = run(&rpc, empty, plan, "ETH".into()).await.unwrap();
    eprintln!("native: steps {:?}\n  changes {:?}\n  fee ≈{} max {} short {:?}", o.steps, o.changes.iter().map(Change::text).collect::<Vec<_>>(), o.fmt_native(o.fee_likely), o.fmt_native(o.fee_max), o.short_of().map(|s| o.fmt_native(s)));
    assert!(o.error().is_none(), "{:?}", o.error());
    assert!(o.changes.iter().any(|c| c.token == Address::ZERO && c.outgoing()), "the locked ETH leaves the wallet");
    assert!(o.short_of().is_some() && o.blocks());

    // USDC: permission + cheque; the cheque reverts (no USDC in the wallet).
    let (w, plan) = Wallet::planner(empty, rpc.clone());
    let r = super::cheques::create(&w, contract, super::cheques::new_v4().1, vec![to], U256::from(1_000_000u64), token, expires, 100).await;
    assert!(matches!(r, Err(TxError::Planned)), "{r:?}");
    let plan = plan.lock().unwrap().clone();
    assert!(plan.len() == 2 && plan[0].approve && !plan[1].approve);
    let o = run(&rpc, empty, plan, "ETH".into()).await.unwrap();
    eprintln!("usdc: steps {:?}\n  error {:?}", o.steps, o.error());
    assert!(o.steps[0].error.is_none(), "the permission goes through");
    assert!(o.steps.get(1).and_then(|s| s.error.as_ref()).is_some(), "the cheque fails: no USDC");
}
