// SPDX-License-Identifier: GPL-3.0-or-later
//! Drops on-chain (utils/drops.ts, AccordoDrop): one deposit paid out to a fixed list of
//! wallets, each with its own amount (and optionally vesting). The contract keeps only the Merkle
//! root of (slot, wallet, amount, schedule); Accordo keeps the list and hands each wallet its proof.

use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_sol_types::SolValue;

use super::abi::AccordoDrop;
use super::cheques::id_for;
use super::client::{random_b256, Call, Rpc, Stage, Wallet};
use super::errors::TxError;

pub const MAX_RECIPIENTS: usize = 50_000;

/// Vesting in steps: `upfront_bps` at start, the rest in `periods` equal parts, the first at
/// start + cliff, then one every `interval` seconds. INSTANT: everything at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub start: u64,
    pub interval: u32,
    pub periods: u16,
    pub cliff: u32,
    pub upfront_bps: u16,
}

pub const INSTANT: Schedule = Schedule { start: 0, interval: 0, periods: 1, cliff: 0, upfront_bps: 0 };

impl Schedule {
    pub fn end(&self) -> u64 {
        self.start + self.cliff as u64 + (self.periods.max(1) as u64 - 1) * self.interval as u64
    }

    fn sol(&self) -> AccordoDrop::Schedule {
        AccordoDrop::Schedule { start: self.start, interval: self.interval, periods: self.periods, cliff: self.cliff, upfrontBps: self.upfront_bps }
    }
}

#[derive(Debug, Clone)]
pub struct Recipient {
    pub address: Address,
    pub amount: U256,
    pub schedule: Schedule,
}

/// keccak256(keccak256(abi.encode(uint32 slot, address, uint256 amount, uint64 start, uint32
/// interval, uint16 periods, uint32 cliff, uint16 upfrontBps))) — the contract's _leaf.
pub fn leaf(slot: u32, account: Address, amount: U256, s: &Schedule) -> B256 {
    let inner = keccak256((slot, account, amount, s.start, s.interval, s.periods, s.cliff, s.upfront_bps).abi_encode_params());
    keccak256(inner)
}

fn hash_pair(a: B256, b: B256) -> B256 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(lo.as_slice());
    buf[32..].copy_from_slice(hi.as_slice());
    keccak256(buf)
}

/// The root only — Accordo builds the proofs when the drop is registered.
pub fn root(recipients: &[Recipient]) -> Result<B256, TxError> {
    if recipients.is_empty() {
        return Err(TxError::Other("Add at least one recipient.".into()));
    }
    let mut layer: Vec<B256> = recipients.iter().enumerate().map(|(i, r)| leaf(i as u32, r.address, r.amount, &r.schedule)).collect();
    while layer.len() > 1 {
        layer = layer.chunks(2).map(|p| if p.len() == 2 { hash_pair(p[0], p[1]) } else { p[0] }).collect();
    }
    Ok(layer[0])
}

/// Whether `proof` leads from this slot's leaf to `root` (MerkleProof.verify: sorted pairs) — a
/// share Accordo lists for a wallet is one the contract will pay only if it does.
pub fn proves(root: B256, slot: u32, account: Address, amount: U256, s: &Schedule, proof: &[B256]) -> bool {
    proof.iter().fold(leaf(slot, account, amount, s), |h, p| hash_pair(h, *p)) == root
}

/// The latest unlock of any wallet: the drop can't expire before everyone's last part.
pub fn vesting_end(recipients: &[Recipient]) -> u64 {
    recipients.iter().map(|r| r.schedule.end()).max().unwrap_or(0)
}

/// Native coin charged once on creation (on top of the deposit): slotFee × wallets, capped.
pub async fn creation_fee(rpc: &Rpc, contract: Address, recipients: u32) -> Result<U256, TxError> {
    rpc.read(contract, &AccordoDrop::creationFeeCall { recipients }).await
}

/// (slotFee, maxDropFee) — for showing the price before the form is sent.
pub async fn pricing(rpc: &Rpc, contract: Address) -> Result<(U256, U256), TxError> {
    Ok((rpc.read(contract, &AccordoDrop::slotFeeCall {}).await?, rpc.read(contract, &AccordoDrop::maxDropFeeCall {}).await?))
}

pub fn fee_for(slot_fee: U256, max_drop_fee: U256, recipients: u32) -> U256 {
    (slot_fee * U256::from(recipients)).min(max_drop_fee)
}

pub struct Created {
    pub drop_id: B256,
    pub tx_hash: B256,
    pub total: U256,
    pub creation_fee: U256,
}

/// Locks the deposit for the list. The fee is read right before sending (the admin may have
/// changed the price since the form showed it).
pub async fn create(w: &Wallet, contract: Address, token: Address, recipients: &[Recipient], expires_at: u64, fee_bps: u16) -> Result<Created, TxError> {
    if recipients.len() > MAX_RECIPIENTS {
        return Err(TxError::Other(format!("At most {MAX_RECIPIENTS} wallets in one drop.")));
    }
    let salt = random_b256();
    let total: U256 = recipients.iter().map(|r| r.amount).fold(U256::ZERO, |a, b| a + b);
    let count = recipients.len() as u32;
    let fee = creation_fee(&w.rpc, contract, count).await?;
    if token != Address::ZERO {
        w.ensure_allowance(token, contract, total).await?;
    }
    let call = Call::new(contract, &AccordoDrop::InitMerkleChequeCall {
        salt,
        tokenAddr: token,
        totalAmount: total,
        merkleRoot: root(recipients)?,
        totalRecipientsCount: count,
        expiresAt: expires_at,
        feeBps: fee_bps,
        vestingEnd: vesting_end(recipients),
    })
    .with_value(if token == Address::ZERO { total + fee } else { fee });
    let tx_hash = w.send(call, Stage::Signing).await?;
    Ok(Created { drop_id: id_for(w.address(), salt), tx_hash, total, creation_fee: fee })
}

/// Claim what has unlocked on your own slot — sent from the slot's wallet.
pub async fn claim(w: &Wallet, contract: Address, drop_id: B256, slot: u32, amount: U256, schedule: &Schedule, proof: Vec<B256>) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoDrop::CashOutMerkleChequeCall { chequeId: drop_id, slot, amount, schedule: schedule.sol(), merkleProof: proof }), Stage::Signing).await
}

pub async fn refund(w: &Wallet, contract: Address, drop_id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoDrop::RefundMerkleChequeCall { chequeId: drop_id }), Stage::Signing).await
}

pub async fn extend(w: &Wallet, contract: Address, drop_id: B256, new_expires_at: u64) -> Result<B256, TxError> {
    w.send(Call::new(contract, &AccordoDrop::ExtendMerkleChequeCall { chequeId: drop_id, newExpiresAt: new_expires_at }), Stage::Signing).await
}

pub async fn info(rpc: &Rpc, contract: Address, drop_id: B256) -> Result<AccordoDrop::MerkleChequeInfo, TxError> {
    rpc.read(contract, &AccordoDrop::getMerkleChequeInfoCall { chequeId: drop_id }).await
}

pub async fn slot_claimed(rpc: &Rpc, contract: Address, drop_id: B256, slot: u32) -> Result<U256, TxError> {
    rpc.read(contract, &AccordoDrop::slotClaimedAmountCall { chequeId: drop_id, slot }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_end() {
        assert_eq!(INSTANT.end(), 0);
        let s = Schedule { start: 1000, interval: 100, periods: 4, cliff: 50, upfront_bps: 0 };
        assert_eq!(s.end(), 1000 + 50 + 3 * 100);
    }

    #[test]
    fn single_leaf_is_root() {
        let r = Recipient { address: Address::repeat_byte(0x11), amount: U256::from(5u8), schedule: INSTANT };
        assert_eq!(root(std::slice::from_ref(&r)).unwrap(), leaf(0, r.address, r.amount, &INSTANT));
        assert!(root(&[]).is_err());
    }

    #[test]
    fn proofs_lead_to_the_root() {
        let vest = Schedule { start: 1000, interval: 100, periods: 4, cliff: 50, upfront_bps: 2500 };
        let list: Vec<Recipient> = (1..=3u8).map(|i| Recipient { address: Address::repeat_byte(i), amount: U256::from(i), schedule: if i == 2 { vest } else { INSTANT } }).collect();
        let top = root(&list).unwrap();
        let l: Vec<B256> = list.iter().enumerate().map(|(i, r)| leaf(i as u32, r.address, r.amount, &r.schedule)).collect();
        // Three leaves: (l0, l1) pair up, l2 moves up alone.
        let p1 = [l[0], l[2]];
        assert!(proves(top, 1, list[1].address, list[1].amount, &vest, &p1));
        assert!(proves(top, 2, list[2].address, list[2].amount, &INSTANT, &[hash_pair(l[0], l[1])]));
        // Another amount, slot, wallet or plan: not this list.
        assert!(!proves(top, 1, list[1].address, U256::from(9u8), &vest, &p1));
        assert!(!proves(top, 0, list[1].address, list[1].amount, &vest, &p1));
        assert!(!proves(top, 1, list[0].address, list[1].amount, &vest, &p1));
        assert!(!proves(top, 1, list[1].address, list[1].amount, &INSTANT, &p1));
    }
}
