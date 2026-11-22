// SPDX-License-Identifier: GPL-3.0-or-later
//! Cheques on-chain (utils/chequeContract.ts — AccordoCheques, utils/taskCheques.ts — AccordoTask).
//!
//! AccordoCheques: a random 32-byte secret; the cheque id is keccak256(secret). Claiming reveals the secret
//! (CashOutCheque), so the backend keeps it and hands it to the recipient.
//! Swap cheques live there too: the creator locks token A (InitSwapCheque); whoever claims pays the asked
//! amount of token B in the same transaction (CashOutSwapCheque) and gets A whole. The Accordo fee comes
//! out of B, on its way to the creator.
//! AccordoTask: a one-time claim key signs (cheque, recipient) with EIP-712; the money can only go to that
//! recipient, and anyone (the Accordo relayer) can submit the claim and pay the gas.

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{eip712_domain, SolStruct, SolValue};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use super::abi::{AccordoCheques, AccordoTask, Claim};
use super::client::{random_b256, Call, Rpc, Stage, Wallet};
use super::errors::TxError;

/// The web's default when a fee isn't given (feeBps || 50).
pub const DEFAULT_FEE_BPS: u16 = 50;
/// Most recipients an AccordoCheques cheque takes (WriteChequeCard.tsx).
pub const MAX_RECIPIENTS: usize = 15;

/// `expiryDaysOrTimestamp`: 0 = never; under 1e9 = days from now; else a unix time.
pub fn expires_at(days_or_ts: u64, now: u64) -> u64 {
    match days_or_ts {
        0 => 0,
        d if d < 1_000_000_000 => now + d * 86_400,
        ts => ts,
    }
}

// ── AccordoCheques ──

/// A fresh cheque: (secret, id = keccak256(secret)). The secret is what claims it.
pub fn new_v4() -> (B256, B256) {
    let secret = random_b256();
    (secret, keccak256(secret))
}

pub fn init_call(contract: Address, id: B256, token: Address, total: U256, recipients: Vec<Address>, expires_at: u64, fee_bps: u16) -> Call {
    let fee_bps = if fee_bps == 0 { DEFAULT_FEE_BPS } else { fee_bps };
    let call = Call::new(contract, &AccordoCheques::InitChequeCall { chequeId: id, tokenAddr: token, totalAmount: total, _to: recipients, expiresAt: expires_at, feeBps: fee_bps });
    if token == Address::ZERO { call.with_value(total) } else { call }
}

/// The swap terms: what the creator locks (`give`) and asks for in return (`get`). receiver ZERO = anyone
/// with the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapTerms {
    pub receiver: Address,
    pub give_token: Address,
    pub give: U256,
    pub get_token: Address,
    pub get: U256,
}

pub fn init_swap_call(contract: Address, id: B256, t: &SwapTerms, expires_at: u64, fee_bps: u16) -> Call {
    let fee_bps = if fee_bps == 0 { DEFAULT_FEE_BPS } else { fee_bps };
    let call = Call::new(contract, &AccordoCheques::InitSwapChequeCall { chequeId: id, _receiver: t.receiver, _tokenIn: t.give_token, _amountIn: t.give, _tokenOut: t.get_token, _amountOut: t.get, expiresAt: expires_at, feeBps: fee_bps });
    if t.give_token == Address::ZERO { call.with_value(t.give) } else { call }
}

/// Locks the swap's `give` (an approve first for a token); returns the transaction hash.
pub async fn create_swap(w: &Wallet, contract: Address, id: B256, t: &SwapTerms, expires_at: u64, fee_bps: u16) -> Result<B256, TxError> {
    if t.give.is_zero() || t.get.is_zero() {
        return Err(TxError::Other("Both amounts must be above zero.".into()));
    }
    if t.give_token == t.get_token {
        return Err(TxError::Other("Swap one token for another, not for itself.".into()));
    }
    w.ensure_allowance(t.give_token, contract, t.give).await?;
    w.send(init_swap_call(contract, id, t, expires_at, fee_bps), Stage::Signing).await
}

/// Takes a swap: pays `amount_out` of `token_out` (a permission first for a token), gets what's locked.
pub async fn take_swap(w: &Wallet, contract: Address, secret: B256, token_out: Address, amount_out: U256) -> Result<B256, TxError> {
    w.ensure_allowance(token_out, contract, amount_out).await?;
    let value = if token_out == Address::ZERO { amount_out } else { U256::ZERO };
    w.send(claim_call(contract, secret, true, value), Stage::Signing).await
}

pub fn claim_call(contract: Address, secret: B256, swap: bool, value: U256) -> Call {
    if swap {
        Call::new(contract, &AccordoCheques::CashOutSwapChequeCall { secret }).with_value(value)
    } else {
        Call::new(contract, &AccordoCheques::CashOutChequeCall { secret })
    }
}

pub fn refund_call(contract: Address, id: B256, swap: bool) -> Call {
    if swap { Call::new(contract, &AccordoCheques::RefundSwapChequeCall { chequeId: id }) } else { Call::new(contract, &AccordoCheques::RefundChequeCall { chequeId: id }) }
}

pub fn extend_call(contract: Address, id: B256, new_expires_at: u64, swap: bool) -> Call {
    if swap {
        Call::new(contract, &AccordoCheques::ExtendSwapChequeCall { chequeId: id, newExpiresAt: new_expires_at })
    } else {
        Call::new(contract, &AccordoCheques::ExtendChequeCall { chequeId: id, newExpiresAt: new_expires_at })
    }
}

/// `{to, data, value: "0"}` as the backend keeps it in detail.claims / detail.refund (getClaimData).
pub fn call_json(c: &Call) -> Value {
    json!({"to": c.to.to_checksum(None), "data": c.data, "value": c.value.to_string()})
}

/// Locks the money (an approve first for a token); returns the transaction hash.
#[allow(clippy::too_many_arguments)] // the contract's own parameters
pub async fn create(w: &Wallet, contract: Address, id: B256, recipients: Vec<Address>, total: U256, token: Address, expires_at: u64, fee_bps: u16) -> Result<B256, TxError> {
    if recipients.is_empty() {
        return Err(TxError::Other("Add at least one recipient.".into()));
    }
    if recipients.len() > MAX_RECIPIENTS {
        return Err(TxError::Other(format!("Too many recipients — at most {MAX_RECIPIENTS}.")));
    }
    if token != Address::ZERO {
        w.ensure_allowance(token, contract, total).await?;
    }
    w.send(init_call(contract, id, token, total, recipients, expires_at, fee_bps), Stage::Signing).await
}

pub async fn claim(w: &Wallet, contract: Address, secret: B256, swap: bool, value: U256) -> Result<B256, TxError> {
    w.send(claim_call(contract, secret, swap, value), Stage::Signing).await
}

pub async fn refund(w: &Wallet, contract: Address, id: B256, swap: bool) -> Result<B256, TxError> {
    w.send(refund_call(contract, id, swap), Stage::Signing).await
}

pub async fn extend(w: &Wallet, contract: Address, id: B256, new_expires_at: u64, swap: bool) -> Result<B256, TxError> {
    w.send(extend_call(contract, id, new_expires_at, swap), Stage::Signing).await
}

pub async fn info(rpc: &Rpc, contract: Address, id: B256) -> Result<AccordoCheques::ChequeInfo, TxError> {
    rpc.read(contract, &AccordoCheques::getChequeInfoCall { id }).await
}

pub async fn swap_detail(rpc: &Rpc, contract: Address, id: B256) -> Result<AccordoCheques::SwapInfo, TxError> {
    rpc.read(contract, &AccordoCheques::getSwapDetailCall { id }).await
}

/// (minBps, defaultBps, maxBps, denominator); the web's fallback (10, 50, 500, 10000) when unreadable.
pub async fn fee(rpc: &Rpc, contract: Address) -> (u16, u16, u16, u32) {
    match rpc.read(contract, &AccordoCheques::getFeeCall {}).await {
        Ok(r) => (r.minBps.to::<u16>(), r.defaultBps.to::<u16>(), r.maxBps.to::<u16>(), r.denominator.to::<u32>()),
        Err(_) => (10, DEFAULT_FEE_BPS, 500, 10_000),
    }
}

/// What a recipient gets of `total` after the fee taken on claim (splitFee).
pub fn split_fee(total: U256, fee_bps: u16) -> (U256, U256) {
    let fee = total * U256::from(fee_bps) / U256::from(10_000u32);
    (fee, total - fee)
}

/// The body of POST /api/cheques for an AccordoCheques cheque, as WriteChequeCard.tsx sends it.
pub struct ChequeRecord<'a> {
    pub recipient_identifiers: &'a [String],
    pub owner_username: &'a str,
    pub secret: B256,
    pub id: B256,
    pub tx_hash: B256,
    pub contract: Address,
    pub from: Address,
    pub recipients: &'a [Address],
    pub amount: &'a str,
    pub net_amount: &'a str,
    pub expires_at: u64,
    pub token_symbol: &'a str,
    pub note: &'a str,
    pub explorer: Option<&'a str>,
    pub chain_name: &'a str,
    pub fee_bps: u16,
    pub room_id: Option<&'a str>,
}

pub fn cheque_record(r: &ChequeRecord) -> Value {
    let claim = call_json(&claim_call(r.contract, r.secret, false, U256::ZERO));
    let mut claims = serde_json::Map::new();
    let mut slot = 1;
    for a in r.recipients {
        if *a == Address::ZERO {
            claims.insert(format!("slot{slot}"), json!({"data": claim, "claimed": false}));
            slot += 1;
        } else {
            claims.insert(a.to_checksum(None), json!({"data": claim, "claimed": false}));
        }
    }
    let tx = format!("{:#x}", r.tx_hash);
    let mut detail = json!({
        "amount": r.net_amount,
        "from": r.from.to_checksum(None),
        "to": r.recipients.iter().map(|a| a.to_checksum(None)).collect::<Vec<_>>(),
        "original_amount": r.amount,
        "expires_at": r.expires_at,
        "token": r.token_symbol,
        "note": r.note,
        "tx_hash": r.explorer.map(|e| format!("{}/tx/{tx}", e.trim_end_matches('/'))).unwrap_or_else(|| tx.clone()),
        "chain": r.chain_name,
        "is_swap": false,
        "claims": claims,
        "refund": call_json(&refund_call(r.contract, r.id, false)),
        "feeBps": r.fee_bps,
        "mentioned_users": r.recipient_identifiers,
        "webhoookUrl": "N/A",
    });
    if let Some(room) = r.room_id {
        detail["room_id"] = json!(room);
    }
    json!({
        "recipient_identificator": r.recipient_identifiers.join("\n"),
        "cheque_owner_identificator": r.owner_username,
        "cheque_type": "Cheque",
        "cheque_secret": format!("{:#x}", r.secret),
        "cheque_id": format!("{:#x}", r.id),
        "tx_hash": tx,
        "detail": detail,
    })
}

/// The body of POST /api/cheques for a swap ("SwapCheque"). The amounts and tokens here are only for
/// a record kept on this computer: Accordo reads them from the SwapChequeCreated event.
pub struct SwapRecord<'a> {
    pub recipient_identifiers: &'a [String],
    pub owner_username: &'a str,
    pub secret: B256,
    pub id: B256,
    pub tx_hash: B256,
    pub contract: Address,
    pub from: Address,
    pub terms: SwapTerms,
    pub give_amount: &'a str,
    pub give_symbol: &'a str,
    pub get_amount: &'a str,
    pub get_symbol: &'a str,
    pub expires_at: u64,
    pub note: &'a str,
    pub explorer: Option<&'a str>,
    pub chain_name: &'a str,
    pub fee_bps: u16,
}

pub fn swap_record(r: &SwapRecord) -> Value {
    let value = if r.terms.get_token == Address::ZERO { r.terms.get } else { U256::ZERO };
    let claim = call_json(&claim_call(r.contract, r.secret, true, value));
    let slot = if r.terms.receiver == Address::ZERO { "slot1".to_string() } else { r.terms.receiver.to_checksum(None) };
    let tx = format!("{:#x}", r.tx_hash);
    json!({
        "recipient_identificator": r.recipient_identifiers.join("\n"),
        "cheque_owner_identificator": r.owner_username,
        "cheque_type": "SwapCheque",
        "cheque_secret": format!("{:#x}", r.secret),
        "cheque_id": format!("{:#x}", r.id),
        "tx_hash": tx,
        "detail": {
            "is_swap": true,
            "amount": r.give_amount,
            "original_amount": r.give_amount,
            "token": r.give_symbol,
            "pay_amount": r.get_amount,
            "pay_token": r.get_symbol,
            "from": r.from.to_checksum(None),
            "to": [r.terms.receiver.to_checksum(None)],
            "expires_at": r.expires_at,
            "note": r.note,
            "tx_hash": r.explorer.map(|e| format!("{}/tx/{tx}", e.trim_end_matches('/'))).unwrap_or_else(|| tx.clone()),
            "chain": r.chain_name,
            "claims": { slot: {"data": claim, "claimed": false} },
            "refund": call_json(&refund_call(r.contract, r.id, true)),
            "feeBps": r.fee_bps,
            "mentioned_users": r.recipient_identifiers,
        },
    })
}

// ── AccordoTask ──

/// Claim signatures from a link never expire on their own (the cheque's expiry still applies).
pub const NO_DEADLINE: u64 = u64::MAX;

/// A one-time claim key: (key as 0x-hex, its address — the cheque's claimSigner).
pub fn new_claim_key() -> (Zeroizing<String>, Address) {
    let s = PrivateKeySigner::random();
    let key = Zeroizing::new(format!("0x{}", alloy_primitives::hex::encode(s.to_bytes())));
    (key, s.address())
}

/// keccak256(abi.encode(sender, salt)) — AccordoTask cheques, AccordoDrop drops and campaigns all derive ids this way.
pub fn id_for(sender: Address, salt: B256) -> B256 {
    keccak256((sender, salt).abi_encode())
}

/// EIP-712 signature of a claim by `key` (65 bytes, v = 27/28 like ethers).
pub fn sign_claim(key: &str, chain_id: u64, contract: Address, cheque_id: B256, recipient: Address, deadline: u64) -> Result<Bytes, TxError> {
    let signer = crate::wallets::evm::signer(key).ok_or(TxError::BadKey)?;
    let domain = eip712_domain! { name: "AccordoCheques", version: "6", chain_id: chain_id, verifying_contract: contract, };
    let msg = Claim { chequeId: cheque_id, recipient, relayerFee: U256::ZERO, deadline };
    let sig = signer.sign_hash_sync(&msg.eip712_signing_hash(&domain)).map_err(|_| TxError::BadKey)?;
    Ok(Bytes::from(sig.as_bytes().to_vec()))
}

pub struct TaskChequeCreated {
    pub cheque_id: B256,
    pub tx_hash: B256,
    /// For the link (None when `claim_signer` was given — tasks: the Accordo arbiter signs).
    pub claim_key: Option<Zeroizing<String>>,
}

/// Creates an AccordoTask cheque for `named` wallets plus `open_slots` anyone-with-the-link slots.
#[allow(clippy::too_many_arguments)] // the contract's own parameters
pub async fn create_task_cheque(w: &Wallet, contract: Address, token: Address, total: U256, named: Vec<Address>, open_slots: u16, expires_at: u64, fee_bps: u16, claim_signer: Option<Address>) -> Result<TaskChequeCreated, TxError> {
    let (claim_key, signer) = match claim_signer {
        Some(s) => (None, s),
        None => {
            let (k, a) = new_claim_key();
            (Some(k), a)
        }
    };
    let salt = random_b256();
    if token != Address::ZERO {
        w.ensure_allowance(token, contract, total).await?;
    }
    let call = Call::new(contract, &AccordoTask::createChequeCall { salt, token, total, named, openSlots: open_slots, claimSigner: signer, expiresAt: expires_at, feeBps: fee_bps });
    let call = if token == Address::ZERO { call.with_value(total) } else { call };
    let tx_hash = w.send(call, Stage::Signing).await?;
    Ok(TaskChequeCreated { cheque_id: id_for(w.address(), salt), tx_hash, claim_key })
}

/// Claim from your own wallet (you pay the gas) — when the relayer isn't available.
pub async fn claim_task_cheque(w: &Wallet, contract: Address, id: B256, recipient: Address, signature: Bytes, deadline: u64) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoTask::claimCall { id, recipient, relayerFee: U256::ZERO, deadline, signature }), Stage::Signing).await
}

pub async fn refund_task_cheque(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoTask::refundCall { id }), Stage::Signing).await
}

pub async fn extend_task_cheque(w: &Wallet, contract: Address, id: B256, new_expires_at: u64) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoTask::extendCall { id, newExpiresAt: new_expires_at }), Stage::Signing).await
}

/// The cheque as the contract has it; None if there's no such cheque.
pub async fn read_task_cheque(rpc: &Rpc, contract: Address, id: B256) -> Result<Option<AccordoTask::getChequeReturn>, TxError> {
    let r = rpc.read(contract, &AccordoTask::getChequeCall { id }).await?;
    Ok((r.cheque.sender != Address::ZERO).then_some(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_rule() {
        assert_eq!(expires_at(0, 1_700_000_000), 0);
        assert_eq!(expires_at(2, 1_700_000_000), 1_700_000_000 + 2 * 86_400);
        assert_eq!(expires_at(1_800_000_000, 1_700_000_000), 1_800_000_000);
    }

    #[test]
    fn swap_record_keeps_a_claim_call_with_the_price() {
        let contract = Address::repeat_byte(0x22);
        let (secret, id) = new_v4();
        let terms = SwapTerms { receiver: Address::ZERO, give_token: Address::repeat_byte(0x33), give: U256::from(10u8), get_token: Address::ZERO, get: U256::from(7u8) };
        let r = swap_record(&SwapRecord {
            recipient_identifiers: &["bob".to_string()], owner_username: "alice", secret, id, tx_hash: B256::repeat_byte(1), contract, from: Address::repeat_byte(0x44),
            terms, give_amount: "10", give_symbol: "USDC", get_amount: "7", get_symbol: "ETH", expires_at: 0, note: "", explorer: None, chain_name: "Base", fee_bps: 50,
        });
        assert_eq!(r["cheque_type"], "SwapCheque");
        assert_eq!(r["detail"]["is_swap"], true);
        // An open swap is "slot1"; taking it pays the asked native amount as value.
        let call = &r["detail"]["claims"]["slot1"]["data"];
        assert_eq!(call["value"], "7");
        assert_eq!(call, &call_json(&claim_call(contract, secret, true, U256::from(7u8))));
        // The native coin is sent along only when it's what gets locked.
        assert_eq!(init_swap_call(contract, id, &terms, 0, 0).value, U256::ZERO);
        let eth_for_token = SwapTerms { give_token: Address::ZERO, get_token: Address::repeat_byte(0x33), ..terms };
        assert_eq!(init_swap_call(contract, id, &eth_for_token, 0, 0).value, U256::from(10u8), "locking the native coin sends it along");
    }

    #[test]
    fn fee_split() {
        let (fee, net) = split_fee(U256::from(10_000u32), 50);
        assert_eq!((fee, net), (U256::from(50u32), U256::from(9_950u32)));
    }
}
