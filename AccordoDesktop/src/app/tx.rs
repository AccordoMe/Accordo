// SPDX-License-Identifier: GPL-3.0-or-later
//! Transaction review and signing: the review window's messages, the `Effect`s pages ask for
//! (sign and call, open a window, report), and choosing the wallet that signs.
//!
//! Responsible for: `tx_update`, `apply`, `signing_wallet` / `signing_my_wallet`, and the review
//! state in `App`. Not here: what is being signed (the flows, `app/flows.rs`), the review window's
//! drawing (`components/tx_review.rs`), and ABI calls / signing itself (`evm/`, `wallets/`).

use super::*;

impl App {

    /// The wallet that signs for the forms: the active Accordo wallet, EVM, unlocked. Otherwise
    /// says what's missing (and opens the wallet drawer, as the web opens its wallet modal).
    pub(super) fn signing_wallet(&mut self) -> Result<crate::store::app_store::SecureWallet, Task<Message>> {
        let Some(w) = self.active_wallet().cloned() else {
            self.wallet_open = true;
            return Err(self.toasts.show(Kind::Error, "Please select a wallet first!"));
        };
        if crate::wallets::chain_of(&w.address) != "EVM" {
            return Err(self.toasts.show(Kind::Error, "Pick an EVM wallet — the desktop app signs on EVM networks only for now"));
        }
        if w.vault.parsed().is_none() {
            return Err(self.toasts.show(Kind::Error, "This wallet has no key in its vault — it can't sign"));
        }
        if !self.wallet_session.unlocked() {
            if let Some(unlock) = self.dev_unlock() {
                return Err(Task::batch([unlock, self.toasts.show(Kind::Info, "Unlocking with the test password — try again in a moment")]));
            }
            self.wallet_open = true;
            return Err(self.toasts.show(Kind::Error, "Unlock your Accordo wallet first"));
        }
        Ok(w)
    }

    pub(super) fn signing_my_wallet(&mut self) -> Result<crate::flows::claim::MyWallet, Task<Message>> {
        self.signing_wallet().map(|w| crate::flows::claim::MyWallet { address: w.address, name: w.name, vault: w.vault })
    }

    /// The transaction window, and what a module waiting on it gets told: paid / written, or
    /// the window closed without it.
    pub(super) fn tx_update(&mut self, m: crate::components::tx_review::TxMsg) -> Task<Message> {
        use crate::components::tx_review::TxMsg;
        let kind = self.tx_kind;
        let done = match &m {
            TxMsg::Done(Ok(f)) => Some(f.clone()),
            _ => None,
        };
        let closing = matches!(m, TxMsg::Close) && self.tx.is_some();
        let task = self.tx_update_inner(m);
        let module = match done {
            Some(f) => self.module_tx_done(kind, &f),
            None if closing && self.tx.is_none() => self.module_tx_closed(),
            None => Task::none(),
        };
        Task::batch([task, module])
    }

    pub(super) fn tx_update_inner(&mut self, m: crate::components::tx_review::TxMsg) -> Task<Message> {
        use crate::components::tx_review::{Failed, Step, TxMsg};
        use crate::flows::Job;
        let Some(tx) = self.tx.as_mut() else { return Task::none() };
        match m {
            TxMsg::Close => {
                if !tx.running() {
                    self.tx = None;
                }
                Task::none()
            }
            TxMsg::Copy(a) => Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Success, "Address copied")]),
            TxMsg::CopyHash => match tx.hash {
                Some(h) => Task::batch([iced::clipboard::write(format!("{h:#x}")), self.toasts.show(Kind::Success, "Transaction hash copied")]),
                None => Task::none(),
            },
            TxMsg::OpenTx => {
                if let Some(url) = tx.tx_url() {
                    if open::that_detached(&url).is_err() {
                        return self.toasts.show(Kind::Info, format!("Open {url} in your browser"));
                    }
                }
                Task::none()
            }
            TxMsg::Confirm => {
                let Some(job) = tx.job.take() else { return Task::none() };
                let Some(api) = self.session.as_ref().map(Session::api) else { return Task::none() };
                // AccordoTask claims: Accordo submits them — no key is opened.
                if let Job::Claim(c) = &job {
                    if let crate::flows::claim::Job::Task { cheque_id, recipient } = c.as_ref() {
                        let (id, to) = (cheque_id.clone(), recipient.clone());
                        tx.step = Step::Signing;
                        return crate::flows::run(move |_, _| crate::flows::claim::run_task(id, to, api));
                    }
                }
                let (vault, address, urls, chain_id) = match &job {
                    Job::Cheque(j) => (j.wallet_vault.clone(), j.wallet_address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    Job::Claim(c) => match c.as_ref() {
                        crate::flows::claim::Job::Cheques(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                        crate::flows::claim::Job::Task { .. } => unreachable!("handled above"),
                    },
                    Job::CreateInvoice(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    Job::PayInvoice(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "drops")]
                    Job::Drop(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "drops")]
                    Job::DropRefund(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "drops")]
                    Job::DropClaim(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "crowdfund")]
                    Job::Launch(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "crowdfund")]
                    Job::Back(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                    #[cfg(feature = "crowdfund")]
                    Job::Campaign(j) => (j.wallet.vault.clone(), j.wallet.address.clone(), j.rpc_urls.clone(), j.chain.evm_id()),
                };
                // The key leaves the vault only here, into the signer; the string is wiped right after.
                let wallet = match self.wallet_session.open(&vault) {
                    Ok(key) => crate::evm::client::Rpc::new(urls, chain_id).and_then(|rpc| crate::evm::client::Wallet::new(&key, rpc)).map_err(|e| e.to_string()),
                    Err(crate::e2ee::wallet_session::OpenError::Locked) => {
                        self.wallet_open = true;
                        Err("Your wallets locked meanwhile — unlock them and try again".to_string())
                    }
                    Err(e) => Err(e.to_string()),
                };
                let wallet = wallet.and_then(|w| {
                    if w.address().to_checksum(None).eq_ignore_ascii_case(address.trim()) { Ok(w) } else { Err("The key in this wallet's vault doesn't match its address".to_string()) }
                });
                let tx = self.tx.as_mut().expect("checked above");
                let wallet = match wallet {
                    Ok(w) => w,
                    Err(e) => {
                        tx.apply(&TxMsg::Done(Err(Failed { message: e, hash: None })));
                        return Task::none();
                    }
                };
                // A token may need the permission first (skipped when it's already given).
                tx.step = if tx.review.approve { Step::Approving } else { Step::Signing };
                match job {
                    Job::Cheque(j) => crate::flows::run(move |progress, out| crate::flows::cheques::run(*j, wallet.with_progress(Some(progress)), api, out)),
                    Job::Claim(c) => match *c {
                        crate::flows::claim::Job::Cheques(j) => crate::flows::run(move |progress, out| crate::flows::claim::run_cheques(*j, wallet.with_progress(Some(progress)), api, out)),
                        crate::flows::claim::Job::Task { .. } => Task::none(),
                    },
                    Job::CreateInvoice(j) => crate::flows::run(move |progress, out| crate::flows::invoices::run_create(*j, wallet.with_progress(Some(progress)), api, out)),
                    Job::PayInvoice(j) => crate::flows::run(move |progress, out| crate::flows::invoices::run_pay(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "drops")]
                    Job::Drop(j) => crate::flows::run(move |progress, out| crate::flows::drops::run_create(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "drops")]
                    Job::DropRefund(j) => crate::flows::run(move |progress, out| crate::flows::drops::run_refund(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "drops")]
                    Job::DropClaim(j) => crate::flows::run(move |progress, out| crate::flows::drops::run_claim(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "crowdfund")]
                    Job::Launch(j) => crate::flows::run(move |progress, out| crate::flows::crowdfund::run_launch(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "crowdfund")]
                    Job::Back(j) => crate::flows::run(move |progress, out| crate::flows::crowdfund::run_back(*j, wallet.with_progress(Some(progress)), api, out)),
                    #[cfg(feature = "crowdfund")]
                    Job::Campaign(j) => crate::flows::run(move |progress, out| crate::flows::crowdfund::run_action(*j, wallet.with_progress(Some(progress)), api, out)),
                }
            }
            m @ TxMsg::Done(_) => {
                tx.apply(&m);
                match &m {
                    TxMsg::Done(Ok(f)) => {
                        use crate::components::finance::records::ListKind;
                        use crate::flows::Kind;
                        if !f.saved {
                            tracing::warn!("an on-chain record couldn't be saved to Accordo; kept on this computer");
                            self.refresh_unsaved();
                        }
                        // The form it came from starts over; the lists that show it load again.
                        let touched: &[ListKind] = match self.tx_kind {
                            Some(Kind::Cheque) => {
                                cheques_page::sent(&mut self.cheques);
                                &[ListKind::MyCheques]
                            }
                            Some(Kind::CreateInvoice) => {
                                invoices_page::sent(&mut self.invoices);
                                &[ListKind::MyInvoices]
                            }
                            Some(Kind::Claim) => &[ListKind::IncomingCheques],
                            Some(Kind::PayInvoice) => &[ListKind::IncomingInvoices],
                            #[cfg(feature = "drops")]
                            Some(Kind::DropClaim) => {
                                if let (Some(id), Some(api)) = (self.tx_subject.clone(), self.session.as_ref().map(Session::api)) {
                                    return crate::pages::chat_media::load_drop_card(&mut self.chat, &api, &id);
                                }
                                &[]
                            }
                            #[cfg(feature = "drops")]
                            Some(Kind::Drop | Kind::DropRefund) => {
                                #[cfg(feature = "drops")]
                                if self.tx_kind == Some(Kind::Drop) {
                                    crate::pages::drops_page::sent(&mut self.drops);
                                }
                                if let Some(api) = self.session.as_ref().map(Session::api) {
                                    return Task::batch([crate::pages::drops_page::load(&mut self.drops, &api), crate::pages::drops_page::reload_open(&mut self.drops, &api)]);
                                }
                                &[]
                            }
                            #[cfg(feature = "crowdfund")]
                            Some(Kind::Launch | Kind::Back | Kind::Campaign) => {
                                #[cfg(feature = "crowdfund")]
                                if self.tx_kind == Some(Kind::Launch) {
                                    crowdfund_page::launched(&mut self.crowdfund);
                                }
                                if self.tx_kind == Some(Kind::Back) {
                                    crowdfund_page::backed(&mut self.crowdfund);
                                }
                                if let Some(api) = self.session.as_ref().map(Session::api) {
                                    return crowdfund_page::reload(&mut self.crowdfund, &api);
                                }
                                &[]
                            }
                            None => &[],
                        };
                        let reload: Vec<ListKind> = touched.iter().copied().filter(|k| self.lists.get(*k).items.is_some()).collect();
                        let mut tasks: Vec<Task<Message>> = reload.into_iter().map(|k| self.load_list(k)).collect();
                        // A cheque card in the chat shows the claim.
                        if let (Some(kind), Some(id), Some(api)) = (self.tx_kind, self.tx_subject.clone(), self.session.as_ref().map(Session::api)) {
                            match kind {
                                Kind::Claim => tasks.push(crate::pages::chat_media::reload_cheque(&mut self.chat, &api, &id)),
                                Kind::PayInvoice if self.chat.invoices.contains_key(&id) => tasks.push(crate::pages::chat_media::load_invoice(&mut self.chat, &api, &id)),
                                _ => {}
                            }
                        }
                        return Task::batch(tasks);
                    }
                    TxMsg::Done(Err(_)) => tracing::info!("transaction flow failed"),
                    _ => {}
                }
                Task::none()
            }
            m => {
                tx.apply(&m);
                Task::none()
            }
        }
    }

    pub(super) fn apply(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::None => Task::none(),
            Effect::NotYet(what) => self.toasts.show(Kind::Info, format!("{what} isn't in the desktop client yet — use the web app for now.")),
            Effect::Error(e) => self.toasts.show(Kind::Error, e),
            Effect::PickTime(purpose, at) => self.time_update(crate::components::time_picker::TimeMsg::Open(purpose, at)),
            Effect::WriteCheque(d) => self.start_cheque(*d),
            Effect::IssueInvoice(d) => self.start_invoice(*d),
            Effect::Run(t) => t,
            Effect::Info(t) => self.toasts.show(Kind::Success, t),
            Effect::Copy(value, what) => Task::batch([iced::clipboard::write(value), self.toasts.show(Kind::Success, format!("{what} copied"))]),
            Effect::Contacts(target) => self.contacts_update(crate::components::contact_picker::ContactsMsg::Open(target)),
            Effect::Batch(list) => Task::batch(list.into_iter().map(|e| self.apply(e)).collect::<Vec<_>>()),
            #[cfg(feature = "drops")]
            Effect::CreateDrop(d) => self.start_drop(*d),
            #[cfg(feature = "drops")]
            Effect::RefundDrop(d) => self.start_drop_refund(*d),
            #[cfg(feature = "crowdfund")]
            Effect::LaunchCampaign(d) => self.start_launch(*d),
            #[cfg(feature = "crowdfund")]
            Effect::BackCampaign(d) => self.start_back(*d),
            #[cfg(feature = "crowdfund")]
            Effect::CampaignAction(c, a) => self.start_campaign_action(*c, a),
            #[cfg(feature = "crowdfund")]
            Effect::ContributionShare(pct) => self.contribution_share(pct),
            Effect::OpenWeb(path) => {
                let base = self.session.as_ref().map(|s| crate::api::client::normalize_base(&s.base)).unwrap_or_default();
                // Full URLs (x.com, mailto:) as they are; paths on the Accordo site.
                let url = if path.contains(':') { path.to_string() } else { format!("{base}{path}") };
                if open::that_detached(&url).is_err() {
                    return self.toasts.show(Kind::Info, format!("Open {url} in your browser"));
                }
                Task::none()
            }
        }
    }
}
