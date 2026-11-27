// SPDX-License-Identifier: GPL-3.0-or-later
//! What happens after a form is filled in: the review window, signing with the Accordo wallet,
//! the on-chain transaction, then the record saved to Accordo (or kept on this computer).
//! One module per kind; the app only glues them to its state.

pub mod cheques;
pub mod claim;
#[cfg(feature = "crowdfund")]
pub mod crowdfund;
#[cfg(feature = "drops")]
pub mod drops;
pub mod invoices;
#[cfg(test)]
mod live;

use alloy_primitives::B256;
use iced::futures::channel::mpsc;
use iced::futures::{stream, StreamExt};
use iced::Task;

use crate::app::Message;
use crate::components::tx_review::{Failed, Finished, TxMsg};
use crate::api::client::Api;
use crate::evm::client::{Progress, Wallet};
use crate::evm::errors::TxError;

/// What the review window runs on Confirm.
#[derive(Clone)]
pub enum Job {
    Cheque(Box<cheques::Job>),
    Claim(Box<claim::Job>),
    CreateInvoice(Box<invoices::CreateJob>),
    PayInvoice(Box<invoices::PayJob>),
    #[cfg(feature = "drops")]
    Drop(Box<drops::CreateJob>),
    #[cfg(feature = "drops")]
    DropRefund(Box<drops::RefundJob>),
    #[cfg(feature = "drops")]
    DropClaim(Box<drops::ClaimJob>),
    #[cfg(feature = "crowdfund")]
    Launch(Box<crowdfund::LaunchJob>),
    #[cfg(feature = "crowdfund")]
    Back(Box<crowdfund::BackJob>),
    #[cfg(feature = "crowdfund")]
    Campaign(Box<crowdfund::ActionJob>),
}

/// A review worked out off the UI thread (after reading the contract): ready to show.
pub struct Ready {
    pub review: crate::components::tx_review::Review,
    pub explorer: Option<String>,
    pub job: Job,
}

/// A Ready through a message (messages are cloned; the job is taken once).
#[derive(Clone)]
pub struct Handoff(std::sync::Arc<std::sync::Mutex<Option<Ready>>>);

impl Handoff {
    pub fn new(r: Ready) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Some(r))))
    }
    pub fn take(&self) -> Option<Ready> {
        self.0.lock().ok()?.take()
    }
}

impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Handoff(..)")
    }
}

/// Never shows the job (keys to the vault are in it, sealed, but still).
impl std::fmt::Debug for Ready {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ready { .. }")
    }
}

/// Which form a review window came from (to tidy it up afterwards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cheque,
    Claim,
    CreateInvoice,
    PayInvoice,
    #[cfg(feature = "drops")]
    Drop,
    #[cfg(feature = "drops")]
    DropRefund,
    #[cfg(feature = "drops")]
    DropClaim,
    #[cfg(feature = "crowdfund")]
    Launch,
    #[cfg(feature = "crowdfund")]
    Back,
    #[cfg(feature = "crowdfund")]
    Campaign,
}

impl Job {
    pub fn kind(&self) -> Kind {
        match self {
            Job::Cheque(_) => Kind::Cheque,
            Job::Claim(_) => Kind::Claim,
            Job::CreateInvoice(_) => Kind::CreateInvoice,
            Job::PayInvoice(_) => Kind::PayInvoice,
            #[cfg(feature = "drops")]
            Job::Drop(_) => Kind::Drop,
            #[cfg(feature = "drops")]
            Job::DropRefund(_) => Kind::DropRefund,
            #[cfg(feature = "drops")]
            Job::DropClaim(_) => Kind::DropClaim,
            #[cfg(feature = "crowdfund")]
            Job::Launch(_) => Kind::Launch,
            #[cfg(feature = "crowdfund")]
            Job::Back(_) => Kind::Back,
            #[cfg(feature = "crowdfund")]
            Job::Campaign(_) => Kind::Campaign,
        }
    }
}

/// Who signs a job, and where.
pub struct Signer {
    pub vault: crate::store::app_store::WalletVault,
    pub address: String,
    pub rpc_urls: Vec<String>,
    pub chain_id: Option<u64>,
    /// The network's coin ("ETH").
    pub native: String,
}

impl Job {
    /// The wallet and network the job signs with; None when Accordo submits it (AccordoTask
    /// claims: nothing is signed here, Accordo pays the gas).
    pub fn signer(&self) -> Option<Signer> {
        use crate::api::data::Chain;
        fn of(w: &claim::MyWallet, urls: &[String], chain: &Chain) -> Signer {
            Signer { vault: w.vault.clone(), address: w.address.clone(), rpc_urls: urls.to_vec(), chain_id: chain.evm_id(), native: chain.native_symbol.clone() }
        }
        Some(match self {
            Job::Cheque(j) => Signer { vault: j.wallet_vault.clone(), address: j.wallet_address.clone(), rpc_urls: j.rpc_urls.clone(), chain_id: j.chain.evm_id(), native: j.chain.native_symbol.clone() },
            Job::Claim(c) => match c.as_ref() {
                claim::Job::Cheques(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
                claim::Job::Task { .. } => return None,
            },
            Job::CreateInvoice(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            Job::PayInvoice(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "drops")]
            Job::Drop(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "drops")]
            Job::DropRefund(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "drops")]
            Job::DropClaim(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "crowdfund")]
            Job::Launch(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "crowdfund")]
            Job::Back(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
            #[cfg(feature = "crowdfund")]
            Job::Campaign(j) => of(&j.wallet, &j.rpc_urls, &j.chain),
        })
    }
}

/// Runs a job's flow with this wallet: the real one on Confirm, or a planner (the simulation).
pub async fn run_job(job: Job, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    match job {
        Job::Cheque(j) => cheques::run(*j, wallet, api, out).await,
        Job::Claim(c) => match *c {
            claim::Job::Cheques(j) => claim::run_cheques(*j, wallet, api, out).await,
            claim::Job::Task { cheque_id, recipient } => claim::run_task(cheque_id, recipient, api).await,
        },
        Job::CreateInvoice(j) => invoices::run_create(*j, wallet, api, out).await,
        Job::PayInvoice(j) => invoices::run_pay(*j, wallet, api, out).await,
        #[cfg(feature = "drops")]
        Job::Drop(j) => drops::run_create(*j, wallet, api, out).await,
        #[cfg(feature = "drops")]
        Job::DropRefund(j) => drops::run_refund(*j, wallet, api, out).await,
        #[cfg(feature = "drops")]
        Job::DropClaim(j) => drops::run_claim(*j, wallet, api, out).await,
        #[cfg(feature = "crowdfund")]
        Job::Launch(j) => crowdfund::run_launch(*j, wallet, api, out).await,
        #[cfg(feature = "crowdfund")]
        Job::Back(j) => crowdfund::run_back(*j, wallet, api, out).await,
        #[cfg(feature = "crowdfund")]
        Job::Campaign(j) => crowdfund::run_action(*j, wallet, api, out).await,
    }
}

/// The review window's check before signing: the job's flow run dry (no key, nothing sent,
/// nothing saved — the flow stops at its real transaction), then what it would send simulated.
pub async fn simulate(job: Job, api: Api) -> Result<crate::evm::simulate::Outcome, String> {
    let Some(side) = job.signer() else { return Err("Accordo submits this one — nothing to simulate".into()) };
    let from: alloy_primitives::Address = side.address.trim().parse().map_err(|_| "Not an EVM wallet".to_string())?;
    let rpc = crate::evm::client::Rpc::new(side.rpc_urls, side.chain_id).map_err(|e| e.to_string())?;
    let (wallet, plan) = Wallet::planner(from, rpc.clone());
    let (out, _ignored) = mpsc::unbounded::<TxMsg>();
    let ran = run_job(job, wallet, api, out).await;
    let plan = plan.lock().map(|p| p.clone()).unwrap_or_default();
    // A dry run ends at its real transaction; one that ended before it failed on the way (a
    // read, a wrong network) — that's the answer.
    if !plan.iter().any(|p| !p.approve) {
        return Err(match ran {
            Err(f) => f.message,
            Ok(_) => "There's nothing to simulate".into(),
        });
    }
    crate::evm::simulate::run(&rpc, from, plan, side.native).await.map_err(|e| e.to_string())
}

pub fn failed(hash: Option<B256>) -> impl Fn(TxError) -> Failed {
    move |e| Failed { message: e.to_string(), hash }
}

/// Runs `work` and streams its progress and its end to the review window, in order (progress
/// and the result go through one channel, so a late "Confirming" can't arrive after "Done").
pub fn run<F, Fut>(work: F) -> Task<Message>
where
    F: FnOnce(Progress, mpsc::UnboundedSender<TxMsg>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Finished, Failed>> + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded::<TxMsg>();
    let progress_tx = tx.clone();
    let progress: Progress = std::sync::Arc::new(move |s, h| {
        let _ = progress_tx.unbounded_send(TxMsg::Stage(s, h));
    });
    let done_tx = tx.clone();
    let job = async move {
        let r = work(progress, tx).await;
        let _ = done_tx.unbounded_send(TxMsg::Done(r));
    };
    let s = stream::select(rx, stream::once(job).filter_map(|_| async { None }));
    Task::run(s, Message::Tx)
}

/// The contract an existing record lives on, as Accordo kept it at registration
/// (`contract_address`; None on records from before it was kept, and the zero address never).
/// Actions on the record (claim, refund, pay, back, withdraw) go there — a redeployed contract
/// mustn't strand records made on the previous one. New records go to the network's current one.
pub fn own_contract(kept: Option<&str>) -> Option<alloy_primitives::Address> {
    kept.and_then(|a| crate::evm::client::address(a).ok()).filter(|a| !a.is_zero())
}
