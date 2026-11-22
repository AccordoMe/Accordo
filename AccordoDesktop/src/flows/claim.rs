// SPDX-License-Identifier: GPL-3.0-or-later
//! Claiming a cheque (ChequesPage.handleDirectClaim, ChequeDetailsCard's in-app claim):
//! - AccordoTask: Accordo signs with the cheque's claim key and pays the gas (claim-for-me); nothing is
//!   signed here, only where the money goes is chosen.
//! - AccordoCheques: CashOutCheque(secret), signed by the wallet the slot is for (or the active one for an
//!   open slot), then recorded with /api/cheques/{id}/claim.
//! - A swap on AccordoCheques: CashOutSwapCheque(secret) pays the asked amount of the other token (a
//!   permission first for a token) and takes the locked one, in one transaction.
//!
//! Unlike the web, the calldata kept in the cheque's record is never signed as it is: only the
//! secret is taken from it — checked against the cheque id (id = keccak256(secret)) and against
//! our contract — and the transaction is built here. What the review window shows (amount,
//! token, recipients, expiry) is read from the contract, not from the record.

use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_sol_types::SolCall;
use iced::futures::channel::mpsc;
use serde_json::Value;

use super::{failed, Ready};
use crate::api::client::Api;
use crate::api::data::Chain;
use crate::components::tx_review::{Failed, Finished, Item, Review, TxMsg};
use crate::evm::abi::AccordoCheques;
use crate::evm::client::{address, format_units, Rpc, Wallet};
use crate::store::app_store::WalletVault;
use crate::wallets::short_address;

/// One of the person's EVM wallets that can sign.
#[derive(Clone)]
pub struct MyWallet {
    pub address: String,
    pub name: String,
    pub vault: WalletVault,
}

/// What the app hands over to work it out.
pub struct Ask {
    pub cheque: Value,
    /// The slot clicked in the chat card ("0x…" / "slot2"), and its recipient.
    pub recipient: Option<String>,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub explorer: Option<String>,
    pub wallets: Vec<MyWallet>,
    /// The active wallet's address (an open slot is claimed with it).
    pub active: Option<String>,
    pub username: String,
}

pub enum Job {
    Cheques(Box<ChequesClaim>),
    Task { cheque_id: String, recipient: String },
}

pub struct ChequesClaim {
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub cheque_id: B256,
    pub secret: B256,
    pub slot_key: String,
    pub wallet: MyWallet,
    pub username: String,
    /// A swap: (the token paid for it, how much) — read from the contract.
    pub swap: Option<(Address, U256)>,
}

fn s<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(str::trim).filter(|x| !x.is_empty())
}

pub fn is_task_cheque(c: &Value) -> bool {
    s(c, &["cheque_type"]) == Some("ChequeV6") || c.pointer("/detail/version").and_then(Value::as_i64) == Some(6)
}

fn parse_b256(x: &str) -> Option<B256> {
    x.trim().parse().ok()
}

/// The secret of an AccordoCheques cheque, from wherever the record has it; only one that hashes to the
/// cheque id counts. A claim call kept in the record gives it too — if it's our contract's
/// CashOutCheque.
pub fn find_secret(c: &Value, id: B256, contract: Address) -> Option<B256> {
    let ok = |x: B256| keccak256(x) == id;
    for p in [&["cheque_secret"][..], &["detail", "cheque_secret"][..]] {
        if let Some(x) = s(c, p).and_then(parse_b256).filter(|x| ok(*x)) {
            return Some(x);
        }
    }
    let claims = c.pointer("/detail/claims").and_then(Value::as_object)?;
    for slot in claims.values() {
        if let Some(x) = slot.get("secret").and_then(Value::as_str).and_then(parse_b256).filter(|x| ok(*x)) {
            return Some(x);
        }
        // {data: {to, data, value}} or {to, data, value}
        let call = slot.get("data").filter(|d| d.get("to").is_some()).unwrap_or(slot);
        let (Some(to), Some(data)) = (call.get("to").and_then(Value::as_str), call.get("data").and_then(Value::as_str)) else { continue };
        if address(to).ok() != Some(contract) {
            continue;
        }
        let Ok(bytes) = alloy_primitives::hex::decode(data.trim_start_matches("0x")) else { continue };
        // CashOutCheque(secret) or, for a swap, CashOutSwapCheque(secret).
        let decoded = AccordoCheques::CashOutChequeCall::abi_decode(&bytes).map(|d| d.secret).or_else(|_| AccordoCheques::CashOutSwapChequeCall::abi_decode(&bytes).map(|d| d.secret));
        if let Some(secret) = decoded.ok().filter(|x| ok(*x)) {
            return Some(secret);
        }
    }
    None
}

/// The record's key for slot `i` of `to` (as cheque_record writes them): the address, or "slotN" for
/// the N-th open slot.
fn slot_key(to: &[Address], i: usize) -> String {
    if to[i] == Address::ZERO {
        format!("slot{}", to[..=i].iter().filter(|a| **a == Address::ZERO).count())
    } else {
        to[i].to_checksum(None)
    }
}

/// The slot to claim: one named for one of my wallets first, else an open one (active wallet).
fn pick_slot(info: &AccordoCheques::ChequeInfo, mine: &[MyWallet], active: Option<&str>, asked: Option<&str>) -> Result<(usize, MyWallet), String> {
    let find = |a: &Address| mine.iter().find(|w| address(&w.address).ok() == Some(*a)).cloned();
    let asked = asked.and_then(|a| address(a).ok());
    let open = info.to.iter().enumerate().find(|(i, a)| **a == Address::ZERO && !info.claimed.get(*i).copied().unwrap_or(true)).map(|(i, _)| i);
    for (i, a) in info.to.iter().enumerate() {
        if info.claimed.get(i).copied().unwrap_or(true) || *a == Address::ZERO {
            continue;
        }
        if asked.is_some_and(|x| x != *a && x != Address::ZERO) {
            continue;
        }
        if let Some(w) = find(a) {
            return Ok((i, w));
        }
    }
    if let Some(i) = open.filter(|_| asked.is_none_or(|x| x == Address::ZERO)) {
        let w = active.and_then(|a| mine.iter().find(|w| w.address.eq_ignore_ascii_case(a)).cloned()).or_else(|| mine.first().cloned());
        return w.map(|w| (i, w)).ok_or_else(|| "Select an EVM wallet to receive it".to_string());
    }
    if info.to.iter().enumerate().all(|(i, _)| info.claimed.get(i).copied().unwrap_or(false)) {
        return Err("This cheque has been claimed already".into());
    }
    Err("None of your wallets is a recipient of this cheque, and it has no open slot left".into())
}

/// Reads the cheque and works out the review (async: the contract is asked first).
pub async fn prepare(ask: Ask) -> Result<Ready, String> {
    let c = &ask.cheque;
    let id_text = s(c, &["cheque_id"]).ok_or("This cheque has no id")?.to_string();
    let amount_shown = crate::components::finance::records::cheque_amount(c);
    let token_shown = crate::components::finance::records::cheque_token(c);

    if is_task_cheque(c) {
        if c.get("one_click_claim").and_then(Value::as_bool) == Some(false) {
            return Err("This cheque is claimed with its link — ask the sender for it".into());
        }
        let asked = ask.recipient.as_deref().filter(|r| address(r).is_ok_and(|a| a != Address::ZERO));
        let claims = c.pointer("/detail/claims").and_then(Value::as_object);
        let named = claims.and_then(|o| ask.wallets.iter().find(|w| o.keys().any(|k| k.eq_ignore_ascii_case(&w.address))));
        let to = asked.map(str::to_string).or_else(|| named.map(|w| w.address.clone())).or_else(|| ask.active.clone()).ok_or("Select an EVM wallet to receive it")?;
        let name = ask.wallets.iter().find(|w| w.address.eq_ignore_ascii_case(&to)).map(|w| w.name.clone()).unwrap_or_else(|| "Wallet".into());
        let review = Review {
            title: "Claim cheque".into(),
            subtitle: "Accordo pays the gas".into(),
            confirm: "Claim",
            items: vec![
                Item::strong("You receive", format!("{amount_shown} {token_shown}")),
                Item::row("From", s(c, &["cheque_owner_identificator"]).unwrap_or("—")),
                Item::Section("Your side"),
                Item::row("To wallet", format!("{name} · {}", short_address(&to))),
                Item::row("Network", ask.chain.name.clone()),
                Item::note("Accordo submits the claim with the cheque's own key and pays the network fee — your wallet signs nothing."),
            ],
            approve: false,
            record: false,
            done_title: "Claimed — no gas paid",
            gasless: true,
        };
        return Ok(Ready { review, explorer: ask.explorer, job: super::Job::Claim(Box::new(Job::Task { cheque_id: id_text, recipient: to })) });
    }

    let swap = c.pointer("/detail/is_swap").and_then(Value::as_bool) == Some(true);
    let contract = match super::own_contract(c.get("contract_address").and_then(Value::as_str)) {
        Some(own) => own,
        None => ask.chain.contract("AccordoCheques").map(address).ok_or_else(|| format!("Accordo cheques aren't on {}", ask.chain.name))?.map_err(|e| e.to_string())?,
    };
    let id = parse_b256(&id_text).ok_or("This cheque's id isn't an on-chain one")?;
    let secret = find_secret(c, id, contract).ok_or("The cheque's secret isn't available to you — open the claim link the sender gave you")?;

    let rpc = Rpc::new(ask.rpc_urls.clone(), ask.chain.evm_id()).map_err(|e| e.to_string())?;
    if swap {
        return prepare_swap(ask, contract, id, secret, rpc).await;
    }
    let info = crate::evm::cheques::info(&rpc, contract, id).await.map_err(|e| format!("Couldn't read the cheque on-chain: {e}"))?;
    if info.from == Address::ZERO {
        return Err(format!("This cheque isn't on {} — is the right network picked?", ask.chain.name));
    }
    if info.refunded {
        return Err("This cheque was refunded to its sender".into());
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if info.expiresAt > 0 && now > info.expiresAt {
        return Err("This cheque has expired".into());
    }
    let (slot, wallet) = pick_slot(&info, &ask.wallets, ask.active.as_deref(), ask.recipient.as_deref())?;
    let (symbol, decimals) = if info.token == Address::ZERO {
        (ask.chain.native_symbol.clone(), 18)
    } else {
        crate::evm::balances::token_info(&rpc, info.token).await.map(|(_, sym, d)| (sym, d)).map_err(|e| format!("Couldn't read the token: {e}"))?
    };
    let fee = info.amountPerRecipient * U256::from(info.feeBps) / U256::from(10_000u32);
    let gets = info.amountPerRecipient - fee;
    let expires = if info.expiresAt == 0 {
        "Never".to_string()
    } else {
        chrono::DateTime::from_timestamp(info.expiresAt as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
    };
    let open = info.to[slot] == Address::ZERO;
    let mut items = vec![
        Item::strong("You receive", format!("{} {symbol}", format_units(gets, decimals))),
        Item::row(format!("Accordo fee, taken now ({}%)", f64::from(info.feeBps) / 100.0), format!("{} {symbol}", format_units(fee, decimals))),
        Item::row("From", format!("{} · {}", s(c, &["cheque_owner_identificator"]).unwrap_or("—"), short_address(&info.from.to_checksum(None)))),
        Item::row("Expires", expires),
        Item::row("Slot", if open { format!("{} (open: any wallet)", slot_key(&info.to, slot)) } else { "Named for your wallet".into() }),
        Item::Section("Your side"),
        Item::row("To wallet", format!("{} · {}", wallet.name, short_address(&wallet.address))),
        Item::row("Network", ask.chain.name.clone()),
    ];
    if open {
        items.push(Item::note("An open slot reveals the secret on-chain when it's claimed — someone watching could try to claim it first. It's yours once the block confirms."));
    }
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo cheques", contract.to_checksum(None)));
    if info.token != Address::ZERO {
        items.push(Item::address(format!("{symbol} token"), info.token.to_checksum(None)));
    }
    let review = Review { title: "Claim cheque".into(), subtitle: format!("{} {symbol} to your wallet", format_units(gets, decimals)), confirm: "Claim", items, approve: false, record: true, done_title: "Cheque claimed", gasless: false };
    let job = ChequesClaim { chain: ask.chain, rpc_urls: ask.rpc_urls, contract, cheque_id: id, secret, slot_key: slot_key(&info.to, slot), wallet, username: ask.username, swap: None };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::Claim(Box::new(Job::Cheques(Box::new(job)))) })
}

/// (symbol, decimals) of a token on this network; the native coin for ZERO.
async fn token_meta(rpc: &Rpc, chain: &Chain, token: Address) -> Result<(String, u8), String> {
    if token == Address::ZERO {
        return Ok((chain.native_symbol.clone(), 18));
    }
    crate::evm::balances::token_info(rpc, token).await.map(|(_, sym, d)| (sym, d)).map_err(|e| format!("Couldn't read the token: {e}"))
}

/// Taking a swap: what it costs and what it gives, as the contract has them (not the record).
async fn prepare_swap(ask: Ask, contract: Address, id: B256, secret: B256, rpc: Rpc) -> Result<Ready, String> {
    let c = &ask.cheque;
    let sw = crate::evm::cheques::swap_detail(&rpc, contract, id).await.map_err(|e| format!("Couldn't read the swap on-chain: {e}"))?;
    if sw.spender == Address::ZERO {
        return Err(format!("This swap isn't on {} — is the right network picked?", ask.chain.name));
    }
    if sw.claimed {
        return Err("This swap has been taken already".into());
    }
    if sw.refunded {
        return Err("This swap was taken back by its sender".into());
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if sw.expiresAt > 0 && now > sw.expiresAt {
        return Err("This swap has expired".into());
    }
    let open = sw.receiver == Address::ZERO;
    let wallet = if open {
        ask.active.as_deref().and_then(|a| ask.wallets.iter().find(|w| w.address.eq_ignore_ascii_case(a)).cloned()).or_else(|| ask.wallets.first().cloned()).ok_or("Select an EVM wallet to take it with")?
    } else {
        ask.wallets.iter().find(|w| address(&w.address).ok() == Some(sw.receiver)).cloned().ok_or("This swap is for another wallet")?
    };
    let (sym_in, dec_in) = token_meta(&rpc, &ask.chain, sw.tokenIn).await?;
    let (sym_out, dec_out) = token_meta(&rpc, &ask.chain, sw.tokenOut).await?;
    let (get, pay) = (format_units(sw.amountIn, dec_in), format_units(sw.amountOut, dec_out));
    let expires = if sw.expiresAt == 0 {
        "Never".to_string()
    } else {
        chrono::DateTime::from_timestamp(sw.expiresAt as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
    };
    let mut items = vec![
        Item::strong("You get", format!("{get} {sym_in}")),
        Item::strong("You pay", format!("{pay} {sym_out}")),
        Item::row("From", format!("{} · {}", s(c, &["cheque_owner_identificator"]).unwrap_or("—"), short_address(&sw.spender.to_checksum(None)))),
        Item::row("Expires", expires),
        Item::note(format!("You pay exactly this. The Accordo fee ({}%) comes out of what the sender gets.", f64::from(sw.feeBps) / 100.0)),
    ];
    if open {
        items.push(Item::note("Anyone with the link can take this swap: whoever's transaction confirms first gets it."));
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("Wallet", format!("{} · {}", wallet.name, short_address(&wallet.address))));
    items.push(Item::row("Network", ask.chain.name.clone()));
    if sw.tokenOut != Address::ZERO {
        items.push(Item::note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {pay} {sym_out}, then the swap.")));
    }
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo cheques", contract.to_checksum(None)));
    for (t, sym) in [(sw.tokenIn, &sym_in), (sw.tokenOut, &sym_out)] {
        if t != Address::ZERO {
            items.push(Item::address(format!("{sym} token"), t.to_checksum(None)));
        }
    }
    let review = Review { title: "Take swap".into(), subtitle: format!("{get} {sym_in} for {pay} {sym_out}"), confirm: "Swap", items, approve: sw.tokenOut != Address::ZERO, record: true, done_title: "Swapped", gasless: false };
    let slot_key = if open { "slot1".to_string() } else { sw.receiver.to_checksum(None) };
    let job = ChequesClaim { chain: ask.chain, rpc_urls: ask.rpc_urls, contract, cheque_id: id, secret, slot_key, wallet, username: ask.username, swap: Some((sw.tokenOut, sw.amountOut)) };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::Claim(Box::new(Job::Cheques(Box::new(job)))) })
}

pub async fn run_cheques(job: ChequesClaim, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let hash = match job.swap {
        Some((token_out, amount_out)) => crate::evm::cheques::take_swap(&wallet, job.contract, job.secret, token_out, amount_out).await,
        None => crate::evm::cheques::claim(&wallet, job.contract, job.secret, false, U256::ZERO).await,
    }
    .map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let id = format!("{:#x}", job.cheque_id);
    // The money is in the wallet now; a failed record is a note, not a failure.
    let warning = crate::api::cheques::record_claim(api, id, job.slot_key, wallet.address().to_checksum(None), format!("{hash:#x}"), job.username)
        .await
        .err()
        .map(|e| format!("Claimed on-chain, but Accordo couldn't record it yet: {}. It shows up once Accordo catches up.", e.message));
    Ok(Finished { hash, saved: true, warning, id: None })
}

pub async fn run_task(cheque_id: String, recipient: String, api: Api) -> Result<Finished, Failed> {
    let r = crate::api::cheques::claim_for_me(api, cheque_id, recipient).await.map_err(|e| Failed { message: e.message, hash: None })?;
    let hash = r.get("tx_hash").and_then(Value::as_str).and_then(parse_b256).unwrap_or(B256::ZERO);
    Ok(Finished { hash, saved: true, warning: None, id: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secret_only_if_it_matches() {
        let contract = Address::repeat_byte(0x22);
        let (secret, id) = crate::evm::cheques::new_v4();
        let call = crate::evm::cheques::call_json(&crate::evm::cheques::claim_call(contract, secret, false, U256::ZERO));
        // From the kept claim call.
        let c = json!({"detail": {"claims": {"slot1": {"data": call, "claimed": false}}}});
        assert_eq!(find_secret(&c, id, contract), Some(secret));
        // The same call aimed at another contract doesn't count.
        assert_eq!(find_secret(&c, id, Address::repeat_byte(0x33)), None);
        // A secret that isn't this cheque's doesn't count.
        let (other, _) = crate::evm::cheques::new_v4();
        assert_eq!(find_secret(&json!({"cheque_secret": format!("{other:#x}")}), id, contract), None);
        assert_eq!(find_secret(&json!({"cheque_secret": format!("{secret:#x}")}), id, contract), Some(secret));
        // A swap's kept call (CashOutSwapCheque, with the price as value) gives it too.
        let swap = crate::evm::cheques::call_json(&crate::evm::cheques::claim_call(contract, secret, true, U256::from(5u8)));
        assert_eq!(find_secret(&json!({"detail": {"claims": {"slot1": {"data": swap}}}}), id, contract), Some(secret));
    }

    #[test]
    fn slots() {
        let me = Address::repeat_byte(0x11);
        let w = MyWallet { address: me.to_checksum(None), name: "Main".into(), vault: WalletVault::Text(String::new()) };
        let info = |to: Vec<Address>, claimed: Vec<bool>| AccordoCheques::ChequeInfo { from: Address::repeat_byte(9), token: Address::ZERO, amountPerRecipient: U256::from(100u8), to, claimed, createdAt: 0, expiresAt: 0, feeBps: 50, refunded: false };
        // Named for me beats an open slot.
        let i = info(vec![Address::ZERO, me], vec![false, false]);
        assert_eq!(pick_slot(&i, std::slice::from_ref(&w), None, None).unwrap().0, 1);
        assert_eq!(slot_key(&i.to, 0), "slot1");
        // Mine is claimed → the open slot, with the active wallet.
        let i = info(vec![Address::ZERO, me], vec![false, true]);
        assert_eq!(pick_slot(&i, std::slice::from_ref(&w), Some(&w.address), None).unwrap().0, 0);
        // Someone else's slot only.
        let i = info(vec![Address::repeat_byte(5)], vec![false]);
        assert!(pick_slot(&i, std::slice::from_ref(&w), None, None).is_err());
        assert_eq!(slot_key(&[Address::ZERO, Address::repeat_byte(5), Address::ZERO], 2), "slot2");
    }
}
