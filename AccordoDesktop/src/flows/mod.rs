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
use crate::evm::client::Progress;
use crate::evm::errors::TxError;

/// What the review window runs on Confirm.
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
