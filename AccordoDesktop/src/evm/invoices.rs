// SPDX-License-Identifier: GPL-3.0-or-later
//! Invoices on-chain (utils/invoiceContract.ts, InvoiceHub). The merchant creates the invoice
//! (a random 32-byte id, optionally a PIN: keccak256 of it is stored, the payer sends the PIN);
//! the payer pays it with payETH / payERC20.

use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_sol_types::SolCall;

use super::abi::InvoiceHub;
use super::client::{random_b256, Call, Rpc, Stage, Wallet};
use super::errors::TxError;

pub const DEFAULT_FEE_BPS: u16 = 50;
/// Most invoices one createInvoices takes (a bill split).
pub const MAX_BATCH: usize = 50;

pub fn new_id() -> B256 {
    random_b256()
}

/// keccak256 of the trimmed PIN; zero when there's none (computeCodeHash).
pub fn code_hash(code: &str) -> B256 {
    let c = code.trim();
    if c.is_empty() { B256::ZERO } else { keccak256(c.as_bytes()) }
}

/// Whether `code` opens an invoice with this on-chain hash (no PIN set: anything does).
pub fn code_matches(code: &str, on_chain: B256) -> bool {
    on_chain == B256::ZERO || code_hash(code) == on_chain
}

/// The web keeps the fee in 0.1%–5% and falls back to 0.5% otherwise.
fn fee_or_default(fee_bps: u16) -> u16 {
    if (10..=500).contains(&fee_bps) { fee_bps } else { DEFAULT_FEE_BPS }
}

/// One invoice to create.
#[derive(Debug, Clone)]
pub struct NewInvoice {
    pub id: B256,
    pub merchant: Address,
    /// Address::ZERO = the native coin.
    pub token: Address,
    pub amount: u128,
    /// 0 = no due date.
    pub due_at: u64,
    pub fee_bps: u16,
    /// Address::ZERO = anyone may pay.
    pub payer: Address,
    pub code: String,
}

impl NewInvoice {
    fn tuple(&self) -> InvoiceHub::NewInvoice {
        InvoiceHub::NewInvoice { id: self.id, merchant: self.merchant, token: self.token, amount: self.amount, dueAt: self.due_at, feeBps: fee_or_default(self.fee_bps), payer: self.payer, codeHash: code_hash(&self.code) }
    }
}

pub fn create_call(contract: Address, i: &NewInvoice) -> Call {
    let t = i.tuple();
    Call::new(contract, &InvoiceHub::createInvoiceCall { id: t.id, merchant: t.merchant, token: t.token, amount: t.amount, dueAt: t.dueAt, feeBps: t.feeBps, payer: t.payer, codeHash: t.codeHash })
}

pub fn create_batch_call(contract: Address, items: &[NewInvoice]) -> Result<Call, TxError> {
    if items.is_empty() || items.len() > MAX_BATCH {
        return Err(TxError::Other(format!("A split needs 1 to {MAX_BATCH} invoices.")));
    }
    Ok(Call::new(contract, &InvoiceHub::createInvoicesCall { items: items.iter().map(NewInvoice::tuple).collect() }))
}

/// The payer's transaction: payETH (value = amount) or payERC20.
pub fn pay_call(contract: Address, id: B256, token: Address, amount: U256, code: &str) -> Call {
    let code = code.trim().to_string();
    if token == Address::ZERO {
        Call::new(contract, &InvoiceHub::payETHCall { id, amount, secretCode: code }).with_value(amount)
    } else {
        Call::new(contract, &InvoiceHub::payERC20Call { id, amount, secretCode: code })
    }
}

pub fn cancel_call(contract: Address, id: B256) -> Call {
    Call::new(contract, &InvoiceHub::cancelInvoiceCall { id })
}

pub async fn create(w: &Wallet, contract: Address, i: &NewInvoice) -> Result<B256, TxError> {
    w.send(create_call(contract, i), Stage::Signing).await
}

pub async fn create_batch(w: &Wallet, contract: Address, items: &[NewInvoice]) -> Result<B256, TxError> {
    w.send(create_batch_call(contract, items)?, Stage::Signing).await
}

/// Pays: an approve of exactly `amount` first for a token.
pub async fn pay(w: &Wallet, contract: Address, id: B256, token: Address, amount: U256, code: &str) -> Result<B256, TxError> {
    if token != Address::ZERO {
        w.ensure_allowance(token, contract, amount).await?;
    }
    w.send(pay_call(contract, id, token, amount, code), Stage::Signing).await
}

pub async fn cancel(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(cancel_call(contract, id), Stage::Signing).await
}

/// Older InvoiceHub deployments create one at a time: look for createInvoices in the bytecode
/// before asking anyone to sign a split that would revert. Can't tell → let the estimate decide.
pub async fn supports_batch(rpc: &Rpc, contract: Address) -> bool {
    match rpc.code(contract).await {
        Ok(code) => code.windows(4).any(|w| w == InvoiceHub::createInvoicesCall::SELECTOR),
        Err(_) => true,
    }
}

pub async fn get(rpc: &Rpc, contract: Address, id: B256) -> Result<InvoiceHub::Invoice, TxError> {
    rpc.read(contract, &InvoiceHub::getInvoiceCall { id }).await
}

/// (default fee bps, denominator); (50, 10000) when unreadable.
pub async fn fee(rpc: &Rpc, contract: Address) -> (u16, u32) {
    let def = rpc.read(contract, &InvoiceHub::DEFAULT_FEE_BPSCall {}).await.map(|v| v.to::<u16>()).unwrap_or(DEFAULT_FEE_BPS);
    let den = rpc.read(contract, &InvoiceHub::FEE_DENOMINATORCall {}).await.map(|v| v.to::<u32>()).unwrap_or(10_000);
    (def, den)
}

/// Settled, overdue — as getInvoiceInfo works them out.
pub fn status(inv: &InvoiceHub::Invoice, now: u64) -> (bool, bool) {
    let settled = inv.fact_paid || inv.paid >= inv.amount;
    let overdue = inv.dueAt > 0 && now > inv.dueAt && !settled && !inv.canceled;
    (settled, overdue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_hash() {
        assert_eq!(code_hash("  "), B256::ZERO);
        assert_eq!(code_hash(" 1234 "), keccak256(b"1234"));
        assert!(code_matches("x", B256::ZERO));
        assert!(code_matches("1234", keccak256(b"1234")));
        assert!(!code_matches("4321", keccak256(b"1234")));
    }

    #[test]
    fn fee_bounds() {
        assert_eq!(fee_or_default(5), 50);
        assert_eq!(fee_or_default(100), 100);
        assert_eq!(fee_or_default(600), 50);
    }
}
