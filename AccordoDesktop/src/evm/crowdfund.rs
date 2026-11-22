// SPDX-License-Identifier: GPL-3.0-or-later
//! Crowdfunding on-chain (utils/crowdfundContract.ts, AccordoCrowdfund). Every write waits for
//! the receipt and fails on a revert.

use alloy_primitives::{Address, B256, U256};

use super::abi::Crowdfund;
use super::cheques::id_for;
use super::client::{random_b256, Call, Rpc, Stage, Wallet};
use super::errors::TxError;

/// AccordoCrowdfund.State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    None,
    Active,
    AwaitingDecision,
    Claimed,
    Refunding,
}

impl State {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => State::Active,
            2 => State::AwaitingDecision,
            3 => State::Claimed,
            4 => State::Refunding,
            _ => State::None,
        }
    }
}

/// Same derivation as the contract: ids are bound to their creator.
pub fn campaign_id(creator: Address, salt: B256) -> B256 {
    id_for(creator, salt)
}

pub struct NewCampaign {
    /// Address::ZERO = the native coin.
    pub token: Address,
    pub target: u128,
    pub deadline: u64,
    pub fee_bps: u16,
    pub all_or_nothing: bool,
}

/// Creates the campaign; returns (its id, the tx hash).
pub async fn create(w: &Wallet, contract: Address, p: &NewCampaign) -> Result<(B256, B256), TxError> {
    let salt = random_b256();
    let call = Call::new(contract, &Crowdfund::createCampaignCall { salt, token: p.token, targetAmount: p.target, deadline: p.deadline, feeBps: p.fee_bps, allOrNothing: p.all_or_nothing });
    let hash = w.send(call, Stage::Signing).await?;
    Ok((campaign_id(w.address(), salt), hash))
}

pub fn contribute_call(contract: Address, id: B256, token: Address, amount: U256, message: &str) -> Call {
    if token == Address::ZERO {
        Call::new(contract, &Crowdfund::contributeETHCall { id, message: message.to_string() }).with_value(amount)
    } else {
        Call::new(contract, &Crowdfund::contributeERC20Call { id, amount, message: message.to_string() })
    }
}

pub async fn contribute(w: &Wallet, contract: Address, id: B256, token: Address, amount: U256, message: &str) -> Result<B256, TxError> {
    if token != Address::ZERO {
        w.ensure_allowance(token, contract, amount).await?;
    }
    w.send(contribute_call(contract, id, token, amount, message), Stage::Signing).await
}

pub async fn claim_funds(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &Crowdfund::claimFundsCall { id }), Stage::Signing).await
}

/// `amount` zero = everything left. The first withdrawal closes refunds for good.
pub async fn claim_funds_to(w: &Wallet, contract: Address, id: B256, recipient: Address, amount: U256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &Crowdfund::claimFundsToCall { id, recipient, amount }), Stage::Signing).await
}

/// Target reached: stop contributions now and open the withdraw-or-refund decision.
pub async fn end_early(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &Crowdfund::endEarlyCall { id }), Stage::Signing).await
}

pub async fn cancel(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &Crowdfund::cancelCampaignCall { id }), Stage::Signing).await
}

pub async fn refund(w: &Wallet, contract: Address, id: B256) -> Result<B256, TxError> {
    w.send(Call::new(contract, &Crowdfund::refundCall { id }), Stage::Signing).await
}

pub async fn get(rpc: &Rpc, contract: Address, id: B256) -> Result<Crowdfund::Campaign, TxError> {
    rpc.read(contract, &Crowdfund::getCampaignCall { id }).await
}

pub async fn state(rpc: &Rpc, contract: Address, id: B256) -> Result<State, TxError> {
    Ok(State::from_u8(rpc.read(contract, &Crowdfund::getCampaignStateCall { id }).await?))
}

/// Pool left to withdraw; None on contracts from before partial withdrawals.
pub async fn claimable(rpc: &Rpc, contract: Address, id: B256) -> Option<U256> {
    rpc.read(contract, &Crowdfund::claimableAmountCall { id }).await.ok()
}

/// The fee on taking `amount` out next; None on older contracts (fee per contribution).
pub async fn withdrawal_fee(rpc: &Rpc, contract: Address, id: B256, amount: U256) -> Option<U256> {
    rpc.read(contract, &Crowdfund::withdrawalFeeCall { id, amount }).await.ok()
}

/// Target reached per the contract; None on contracts from before ending early.
pub async fn goal_reached(rpc: &Rpc, contract: Address, id: B256) -> Option<bool> {
    rpc.read(contract, &Crowdfund::goalReachedCall { id }).await.ok()
}

/// What `donor` gets back if the campaign ends in refunds.
pub async fn contribution(rpc: &Rpc, contract: Address, id: B256, donor: Address) -> Result<U256, TxError> {
    rpc.read(contract, &Crowdfund::getContributionCall { id, donor }).await
}

/// chain.data.json names may carry a "?" prefix ("?Base"); the backend stores them without it.
pub fn same_network(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().trim_start_matches('?').to_lowercase();
    !a.trim().is_empty() && n(a) == n(b)
}
