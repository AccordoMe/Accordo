// SPDX-License-Identifier: GPL-3.0-or-later
//! Crowdfunding: launching a campaign (CreateCampaignCard.handleLaunchCampaign), backing one
//! (ContributeCampaignCard.handleContribute) and what can be done with one afterwards
//! (CampaignDetailsCard: withdraw, end early, refund everyone, get your refund).
//!
//! Backing and the actions read the campaign from the contract before the review window: the
//! state, the pool left and the fee shown are the contract's.

use alloy_primitives::{Address, B256, U256};
use iced::futures::channel::mpsc;

use super::claim::MyWallet;
use super::{failed, Ready};
use crate::api::client::Api;
use crate::api::crowdfund::{CampaignCreatePayload, CampaignRead, CampaignStatus};
use crate::api::data::{Chain, Token};
use crate::components::tx_review::{Failed, Finished, Item, Review, TxMsg};
use crate::evm::client::{address, format_units, parse_units, token_or_native, Rpc, Wallet};
use crate::evm::crowdfund::{self as cf, NewCampaign};
use crate::store::unsaved;
use crate::wallets::short_address;

/// Crowdfunding Terms version the creator / backer agrees to (src/legal.ts).
pub const TERMS_VERSION: &str = "2026-10-15";

/// Cover presets (utils/campaignCover.ts); a new campaign gets one at random.
const COVERS: [&str; 5] = [
    "linear-gradient(135deg, #6366f1 0%, #a855f7 100%)",
    "linear-gradient(135deg, #3b82f6 0%, #06b6d4 100%)",
    "linear-gradient(135deg, #f59e0b 0%, #ef4444 100%)",
    "linear-gradient(135deg, #10b981 0%, #059669 100%)",
    "linear-gradient(135deg, #ec4899 0%, #8b5cf6 100%)",
];

fn random_cover() -> &'static str {
    let mut b = [0u8; 1];
    let _ = getrandom::fill(&mut b);
    COVERS[b[0] as usize % COVERS.len()]
}

pub fn contract_of(chain: &Chain) -> Option<Address> {
    chain.contract("AccordoCrowdfund").and_then(|a| address(a).ok()).filter(|a| *a != Address::ZERO)
}

fn when(ts: u64) -> String {
    chrono::DateTime::from_timestamp(ts as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
}

fn pct(bps: u16) -> String {
    format!("{}", f64::from(bps) / 100.0)
}

fn sync_note(r: Result<crate::api::crowdfund::SyncResult, crate::api::client::ApiError>, done: &str) -> Option<String> {
    r.err().map(|_| format!("{done} on-chain — the campaign's numbers at Accordo update shortly."))
}

// ── launching ──

#[derive(Debug, Clone)]
pub struct LaunchDraft {
    pub title: String,
    pub description: String,
    pub category: String,
    pub token: Token,
    pub target: String,
    pub target_units: u128,
    pub deadline: u64,
    pub fee_bps: u16,
    pub all_or_nothing: bool,
}

#[derive(Clone)]
pub struct LaunchJob {
    pub draft: LaunchDraft,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub wallet: MyWallet,
    pub owner: String,
}

pub fn launch_review(job: &LaunchJob) -> Review {
    let d = &job.draft;
    let native = token_or_native(Some(&d.token.address)).map(|a| a == Address::ZERO).unwrap_or(true);
    let mut items = vec![
        Item::strong("Target", format!("{} {}", d.target, d.token.symbol)),
        Item::row("Ends", when(d.deadline)),
        Item::row("All or nothing", if d.all_or_nothing { "Yes — target missed means everyone gets refunded" } else { "No" }),
        Item::row("Accordo fee, only on what you withdraw", format!("{}%", pct(d.fee_bps))),
        Item::row("Category", d.category.clone()),
        Item::note("Launching moves no money — only the network fee. Backers get everything back if it ends in refunds."),
        Item::Section("Your side"),
        Item::row("From wallet", format!("{} · {}", job.wallet.name, short_address(&job.wallet.address))),
        Item::row("Network", job.chain.name.clone()),
        Item::Section("Contracts"),
        Item::address("Accordo crowdfunding", job.contract.to_checksum(None)),
    ];
    if !native {
        items.push(Item::address(format!("{} token", d.token.symbol), d.token.address.clone()));
    }
    Review { title: "Launch campaign".into(), subtitle: d.title.clone(), confirm: "Launch", items, approve: false, record: true, done_title: "Campaign launched", gasless: false }
}

pub async fn run_launch(job: LaunchJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let d = &job.draft;
    let token = token_or_native(Some(&d.token.address)).map_err(failed(None))?;
    let (_, hash) = cf::create(&wallet, job.contract, &NewCampaign { token, target: d.target_units, deadline: d.deadline, fee_bps: d.fee_bps, all_or_nothing: d.all_or_nothing }).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let payload = CampaignCreatePayload {
        tx_hash: format!("{hash:#x}"),
        chain_name: job.chain.name.clone(),
        title: d.title.trim().to_string(),
        description: Some(d.description.trim().to_string()),
        category: Some(d.category.clone()),
        cover_image: Some(random_cover().to_string()),
        token_symbol: Some(d.token.symbol.clone()),
        accepted_terms: Some(TERMS_VERSION.to_string()),
    };
    let server = api.base().to_string();
    match crate::api::crowdfund::create(api, payload.clone()).await {
        Ok(_) => Ok(Finished { hash, saved: true, warning: None, id: None }),
        Err(_) => {
            let body = serde_json::to_value(&payload).unwrap_or_default();
            match unsaved::keep("campaign", "/api/crowdfund", body, Some(format!("{hash:#x}")), &job.owner, &server) {
                Ok(()) => Ok(Finished { hash, saved: false, warning: None, id: None }),
                Err(e) => Err(Failed { message: format!("The campaign is live, but Accordo couldn't list it and its details couldn't be kept on this computer either ({e})."), hash: Some(hash) }),
            }
        }
    }
}

// ── backing ──

#[derive(Debug, Clone)]
pub struct BackDraft {
    pub campaign_id: String,
    pub amount: String,
    pub message: String,
}

pub struct BackAsk {
    pub draft: BackDraft,
    /// The campaign as Accordo lists it (looked up by id first).
    pub campaign: CampaignRead,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub explorer: Option<String>,
    pub wallet: MyWallet,
}

#[derive(Clone)]
pub struct BackJob {
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub id: B256,
    pub token: Address,
    pub amount: U256,
    pub message: String,
    pub wallet: MyWallet,
}

/// The campaign by its id, at Accordo (where its title, token and network are).
pub async fn lookup(api: Api, id: String) -> Result<CampaignRead, String> {
    let id = id.trim().to_string();
    if !(id.starts_with("0x") && id.len() == 66) {
        return Err("Paste the campaign's 32-byte ID (0x… 66 characters)".into());
    }
    crate::api::crowdfund::get(api, id).await.map(|d| d.campaign).map_err(|e| if e.status == Some(404) { "No campaign with this ID at Accordo".to_string() } else { e.message })
}

pub async fn prepare_back(ask: BackAsk) -> Result<Ready, String> {
    let c = &ask.campaign;
    if c.status != CampaignStatus::Active {
        return Err("This campaign is no longer accepting contributions.".into());
    }
    if c.hidden == Some(true) {
        return Err("This campaign was hidden by moderators — Accordo isn't taking contributions for it.".into());
    }
    let contract = super::own_contract(c.contract_address.as_deref()).or_else(|| contract_of(&ask.chain)).ok_or_else(|| format!("Crowdfunding isn't available on {} yet.", ask.chain.name))?;
    let id: B256 = c.campaign_id.parse().map_err(|_| "This campaign's id isn't an on-chain one".to_string())?;
    let amount = parse_units(&ask.draft.amount, c.token_decimals).ok().filter(|a| !a.is_zero()).ok_or("Please enter a valid contribution amount!")?;
    let rpc = Rpc::new(ask.rpc_urls.clone(), ask.chain.evm_id()).map_err(|e| e.to_string())?;
    let on_chain = cf::get(&rpc, contract, id).await.map_err(|e| format!("Couldn't read the campaign on-chain: {e}"))?;
    if on_chain.creator == Address::ZERO {
        return Err(format!("This campaign isn't on {} — is the right network picked?", ask.chain.name));
    }
    if cf::state(&rpc, contract, id).await.map_err(|e| e.to_string())? != cf::State::Active {
        return Err("This campaign is no longer accepting contributions.".into());
    }
    let token = on_chain.token;
    let symbol = c.token_symbol.clone();
    let shown = format_units(amount, c.token_decimals);
    let mut items = vec![Item::strong("You give", format!("{shown} {symbol}"))];
    if !ask.draft.message.trim().is_empty() {
        items.push(Item::row("Your message", ask.draft.message.trim().to_string()));
    }
    items.push(Item::row("Raised so far", format!("{} / {} {symbol}", c.raised_amount, c.target_amount)));
    items.push(Item::row("Created by", format!("{} · {}", c.creator_username, short_address(&on_chain.creator.to_checksum(None)))));
    items.push(Item::note("All of it goes into the pool — no fee on the way in. If the campaign ends in refunds, you get all of it back."));
    if token != Address::ZERO {
        items.push(Item::note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {shown} {symbol}, then the contribution.")));
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{} · {}", ask.wallet.name, short_address(&ask.wallet.address))));
    items.push(Item::row("Network", ask.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo crowdfunding", contract.to_checksum(None)));
    if token != Address::ZERO {
        items.push(Item::address(format!("{symbol} token"), token.to_checksum(None)));
    }
    let review = Review { title: "Back this campaign".into(), subtitle: c.title.clone(), confirm: "Contribute", items, approve: token != Address::ZERO, record: true, done_title: "Contribution confirmed — thank you!", gasless: false };
    let job = BackJob { chain: ask.chain, rpc_urls: ask.rpc_urls, contract, id, token, amount, message: ask.draft.message.trim().to_string(), wallet: ask.wallet };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::Back(Box::new(job)) })
}

pub async fn run_back(job: BackJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let hash = cf::contribute(&wallet, job.contract, job.id, job.token, job.amount, &job.message).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let warning = sync_note(crate::api::crowdfund::sync(api, format!("{:#x}", job.id), format!("{hash:#x}")).await, "Contribution confirmed");
    Ok(Finished { hash, saved: true, warning, id: None })
}

// ── after launch: withdraw, end early, refund everyone, get your refund ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The creator takes what's left in the pool (after the deadline / ending early).
    Withdraw,
    /// The creator stops it early once the target is reached.
    EndEarly,
    /// The creator refunds every backer instead.
    RefundAll,
    /// A backer takes their contribution back (refunds are open).
    Refund,
}

pub struct ActionAsk {
    pub action: Action,
    pub campaign: CampaignRead,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub explorer: Option<String>,
    pub wallets: Vec<MyWallet>,
}

#[derive(Clone)]
pub struct ActionJob {
    pub action: Action,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub id: B256,
    pub wallet: MyWallet,
    /// claimFundsTo on current contracts; claimFunds on the older ones.
    pub legacy: bool,
}

pub async fn prepare_action(ask: ActionAsk) -> Result<Ready, String> {
    let c = &ask.campaign;
    let contract = super::own_contract(c.contract_address.as_deref()).or_else(|| contract_of(&ask.chain)).ok_or_else(|| format!("Crowdfunding isn't available on {} yet.", ask.chain.name))?;
    let id: B256 = c.campaign_id.parse().map_err(|_| "This campaign's id isn't an on-chain one".to_string())?;
    let rpc = Rpc::new(ask.rpc_urls.clone(), ask.chain.evm_id()).map_err(|e| e.to_string())?;
    let on_chain = cf::get(&rpc, contract, id).await.map_err(|e| format!("Couldn't read the campaign on-chain: {e}"))?;
    let state = cf::state(&rpc, contract, id).await.map_err(|e| e.to_string())?;
    let dec = c.token_decimals;
    let sym = c.token_symbol.clone();
    let fmt = |v: U256| format!("{} {sym}", format_units(v, dec));
    let find = |a: Address| ask.wallets.iter().find(|w| address(&w.address).ok() == Some(a)).cloned();

    let creator_wallet = || find(on_chain.creator).ok_or_else(|| format!("Use the wallet this campaign was created from ({}) — it isn't one of yours", short_address(&on_chain.creator.to_checksum(None))));
    let mut legacy = false;
    let (wallet, title, confirm, done, items): (MyWallet, &str, &'static str, &'static str, Vec<Item>) = match ask.action {
        Action::Withdraw => {
            if !matches!(state, cf::State::AwaitingDecision | cf::State::Claimed) {
                return Err("The pool can be withdrawn once the campaign has ended".into());
            }
            let w = creator_wallet()?;
            let mut items = Vec::new();
            match cf::claimable(&rpc, contract, id).await {
                Some(left) => {
                    if left.is_zero() {
                        return Err("Nothing is left in the pool".into());
                    }
                    let fee = cf::withdrawal_fee(&rpc, contract, id, left).await.unwrap_or(U256::ZERO);
                    items.push(Item::row("Leaves the pool", fmt(left)));
                    items.push(Item::row(format!("Accordo fee ({}%)", pct(c.fee_bps as u16)), fmt(fee)));
                    items.push(Item::strong("Your wallet gets", fmt(left - fee)));
                }
                None => {
                    legacy = true;
                    items.push(Item::strong("Leaves the pool", format!("{} {sym}", c.net_raised_amount)));
                    items.push(Item::row(format!("Accordo fee ({}%)", pct(c.fee_bps as u16)), "taken from it"));
                }
            }
            items.push(Item::row("Sent to", format!("{} · {}", w.name, short_address(&w.address))));
            if state == cf::State::AwaitingDecision {
                items.push(Item::note("This is your decision to keep the funds: refunds close for good."));
            }
            (w, "Withdraw the funds", "Withdraw", "Funds sent to your wallet", items)
        }
        Action::EndEarly => {
            if state != cf::State::Active {
                return Err("Only a running campaign can be ended early".into());
            }
            if cf::goal_reached(&rpc, contract, id).await != Some(true) {
                return Err("The campaign can end early only once the target is reached".into());
            }
            let w = creator_wallet()?;
            (w, "End the campaign now", "End campaign", "Campaign ended — you can withdraw now", vec![Item::note("Contributions stop at once, and you can withdraw what was raised (or refund everyone). This is final.")])
        }
        Action::RefundAll => {
            if !matches!(state, cf::State::Active | cf::State::AwaitingDecision) {
                return Err("Refunds can't be opened for this campaign any more".into());
            }
            let w = creator_wallet()?;
            let text = if state == cf::State::Active {
                "Stop this campaign and refund everyone? No new contributions will be accepted. This is final."
            } else {
                "Refund all backers instead of withdrawing? This is final."
            };
            (w, "Refund everyone", "Open refunds", "Refunds are open for all backers", vec![Item::note(text)])
        }
        Action::Refund => {
            if state != cf::State::Refunding {
                return Err("Refunds aren't open for this campaign".into());
            }
            let mut mine = None;
            for w in &ask.wallets {
                let Ok(a) = address(&w.address) else { continue };
                if let Ok(v) = cf::contribution(&rpc, contract, id, a).await {
                    if !v.is_zero() {
                        mine = Some((w.clone(), v));
                        break;
                    }
                }
            }
            let (w, back) = mine.ok_or("None of your wallets has anything to get back from this campaign")?;
            (w, "Get your refund", "Refund", "Refund sent to your wallet", vec![Item::strong("You get back", fmt(back)), Item::note("Everything you put in, no fee.")])
        }
    };
    let mut items = items;
    items.insert(0, Item::row("Campaign", c.title.clone()));
    items.push(Item::Section("Your side"));
    items.push(Item::row("Wallet", format!("{} · {}", wallet.name, short_address(&wallet.address))));
    items.push(Item::row("Network", ask.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo crowdfunding", contract.to_checksum(None)));
    let review = Review { title: title.into(), subtitle: c.title.clone(), confirm, items, approve: false, record: true, done_title: done, gasless: false };
    let job = ActionJob { action: ask.action, chain: ask.chain, rpc_urls: ask.rpc_urls, contract, id, wallet, legacy };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::Campaign(Box::new(job)) })
}

pub async fn run_action(job: ActionJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let (c, id) = (job.contract, job.id);
    let hash = match job.action {
        Action::Withdraw if job.legacy => cf::claim_funds(&wallet, c, id).await,
        // amount 0 = everything left, to this wallet.
        Action::Withdraw => cf::claim_funds_to(&wallet, c, id, wallet.address(), U256::ZERO).await,
        Action::EndEarly => cf::end_early(&wallet, c, id).await,
        Action::RefundAll => cf::cancel(&wallet, c, id).await,
        Action::Refund => cf::refund(&wallet, c, id).await,
    }
    .map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let warning = sync_note(crate::api::crowdfund::sync(api, format!("{id:#x}"), format!("{hash:#x}")).await, "Done");
    Ok(Finished { hash, saved: true, warning, id: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_are_presets() {
        for _ in 0..20 {
            assert!(COVERS.contains(&random_cover()));
        }
    }
}
