// SPDX-License-Identifier: GPL-3.0-or-later
//! The money flows: what a user asks for — claim a cheque, pay an invoice, issue one, claim or
//! refund a drop, launch / back a campaign — turned into a transaction to review and sign.
//!
//! Responsible for: the `start_*` entry points, `contribution_share`, `can_start` (is there a
//! wallet that can sign?), and the wallet / chain lookups the flows need (`my_wallets`,
//! `chain_named`, `explorer_of`, `chain_with`). Not here: the review window and signing itself
//! (`app/tx.rs`), the forms that produce the drafts (`pages/*`), and the contract calls
//! (`flows/*.rs`, `evm/`).

use super::*;

impl App {

    /// The person's EVM wallets that can sign (a key in the vault).
    pub(super) fn my_wallets(&self) -> Vec<crate::flows::claim::MyWallet> {
        self.store
            .wallets
            .iter()
            .filter(|w| crate::wallets::chain_of(&w.address) == "EVM" && w.vault.parsed().is_some())
            .map(|w| crate::flows::claim::MyWallet { address: w.address.clone(), name: w.name.clone(), vault: w.vault.clone() })
            .collect()
    }

    /// The network a cheque / invoice is on (by the name its record keeps), else the selected one.
    pub(super) fn chain_named(&self, name: Option<&str>) -> Option<Chain> {
        let by_name = name.and_then(|n| {
            let n = n.trim().to_lowercase();
            self.chains.iter().find(|c| c.name.to_lowercase() == n || c.to_string().to_lowercase() == n)
        });
        by_name.or_else(|| self.selected_chain()).cloned()
    }

    pub(super) fn explorer_of(&self, chain: &Chain) -> Option<String> {
        self.store.explorer_override(&chain.id).map(|e| e.trim_end_matches('/').to_string()).or_else(|| chain.explorer.clone())
    }

    /// What every claim / payment needs first: signed in, no other window, wallets unlocked.
    pub(super) fn can_start(&mut self, needs_key: bool) -> Result<String, Task<Message>> {
        if self.tx.is_some() || self.tx_preparing {
            return Err(Task::none());
        }
        let Some(username) = self.session.as_ref().filter(|s| !s.token.is_empty()).map(|s| s.user.username.clone()) else {
            return Err(self.toasts.show(Kind::Error, "Sign in first"));
        };
        if self.my_wallets().is_empty() {
            self.wallet_open = true;
            return Err(self.toasts.show(Kind::Error, "Please select a wallet first!"));
        }
        if needs_key && !self.wallet_session.unlocked() {
            if let Some(unlock) = self.dev_unlock() {
                return Err(Task::batch([unlock, self.toasts.show(Kind::Info, "Unlocking with the test password — try again in a moment")]));
            }
            self.wallet_open = true;
            return Err(self.toasts.show(Kind::Error, "Unlock your Accordo wallet first"));
        }
        Ok(username)
    }

    pub(super) fn start_claim(&mut self, cheque: serde_json::Value, recipient: Option<String>) -> Task<Message> {
        let gasless = crate::flows::claim::is_task_cheque(&cheque);
        let username = match self.can_start(!gasless) {
            Ok(u) => u,
            Err(t) => return t,
        };
        let name = cheque.pointer("/detail/chain").and_then(|c| c.as_str()).or_else(|| cheque.get("chain").and_then(|c| c.as_str())).map(str::to_string);
        let Some(chain) = self.chain_named(name.as_deref()) else { return self.toasts.show(Kind::Error, "Pick a network first") };
        if chain.solana {
            return self.toasts.show(Kind::Error, "Solana cheques aren't in the desktop client yet");
        }
        self.tx_subject = cheque.get("cheque_id").and_then(|c| c.as_str()).map(str::to_string);
        let ask = crate::flows::claim::Ask {
            cheque,
            recipient,
            rpc_urls: self.rpc_urls(&chain),
            explorer: self.explorer_of(&chain),
            chain,
            wallets: self.my_wallets(),
            active: self.active_wallet().map(|w| w.address.clone()).filter(|a| crate::wallets::chain_of(a) == "EVM"),
            username,
        };
        self.tx_preparing = true;
        Task::perform(crate::flows::claim::prepare(ask), |r| Message::TxReady(r.map(crate::flows::Handoff::new)))
    }

    pub(super) fn start_pay(&mut self, invoice: serde_json::Value) -> Task<Message> {
        if let Err(t) = self.can_start(true) {
            return t;
        }
        let name = invoice.get("chain_name").and_then(|c| c.as_str()).or_else(|| invoice.pointer("/detail/chain").and_then(|c| c.as_str())).map(str::to_string);
        let Some(chain) = self.chain_named(name.as_deref()) else { return self.toasts.show(Kind::Error, "Pick a network first") };
        if chain.solana {
            return self.toasts.show(Kind::Error, "This invoice is on Solana — pay it in the web app for now");
        }
        self.tx_subject = invoice.get("invoice_id").and_then(|c| c.as_str()).map(str::to_string);
        let ask = crate::flows::invoices::PayAsk {
            invoice,
            rpc_urls: self.rpc_urls(&chain),
            explorer: self.explorer_of(&chain),
            chain,
            wallets: self.my_wallets(),
            active: self.active_wallet().map(|w| w.address.clone()).filter(|a| crate::wallets::chain_of(a) == "EVM"),
        };
        self.tx_preparing = true;
        Task::perform(crate::flows::invoices::prepare_pay(ask), |r| Message::TxReady(r.map(crate::flows::Handoff::new)))
    }

    /// The invoice form is filled in: check the wallet and the network, then the review window.
    pub(super) fn start_invoice(&mut self, draft: crate::flows::invoices::Draft) -> Task<Message> {
        let owner = match self.can_start(true) {
            Ok(u) => u,
            Err(t) => return t,
        };
        let Some(chain) = self.selected_chain().cloned() else { return self.toasts.show(Kind::Error, "Pick a network first") };
        if chain.solana || chain.evm_id().is_none() {
            return self.toasts.show(Kind::Error, format!("Invoices on {} aren't in the desktop client yet — pick an EVM network", chain.name));
        }
        let Some(contract) = crate::flows::invoices::contract_of(&chain) else {
            return self.toasts.show(Kind::Error, format!("Accordo invoices aren't on {} yet — pick another network", chain.name));
        };
        let wallet = match self.signing_wallet() {
            Ok(w) => crate::flows::claim::MyWallet { address: w.address, name: w.name, vault: w.vault },
            Err(t) => return t,
        };
        let explorer = self.explorer_of(&chain);
        let job = crate::flows::invoices::CreateJob { draft, rpc_urls: self.rpc_urls(&chain), chain, contract, wallet, owner };
        let review = crate::flows::invoices::create_review(&job);
        let job = crate::flows::Job::CreateInvoice(Box::new(job));
        self.tx_kind = Some(job.kind());
        self.tx = Some(crate::components::tx_review::State::new(review, explorer, job));
        self.simulate_tx()
    }

    #[cfg(any(feature = "drops", feature = "crowdfund"))]
    /// The selected EVM network with one of Accordo's contracts on it.
    pub(super) fn chain_with(&mut self, what: &str, contract: impl Fn(&Chain) -> Option<alloy_primitives::Address>) -> Result<(Chain, alloy_primitives::Address), Task<Message>> {
        let Some(chain) = self.selected_chain().cloned() else { return Err(self.toasts.show(Kind::Error, "Pick a network first")) };
        if chain.solana || chain.evm_id().is_none() {
            return Err(self.toasts.show(Kind::Error, format!("{what} on {} aren't in the desktop client yet — pick an EVM network", chain.name)));
        }
        match contract(&chain) {
            Some(a) => Ok((chain, a)),
            None => Err(self.toasts.show(Kind::Error, format!("{what} aren't available on {} yet — pick another network", chain.name))),
        }
    }

    #[cfg(feature = "drops")]
    pub(super) fn start_drop(&mut self, draft: crate::flows::drops::Draft) -> Task<Message> {
        let owner = match self.can_start(true) {
            Ok(u) => u,
            Err(t) => return t,
        };
        let (chain, _) = match self.chain_with("Drops", crate::flows::drops::contract_of) {
            Ok(c) => c,
            Err(t) => return t,
        };
        let wallet = match self.signing_my_wallet() {
            Ok(w) => w,
            Err(t) => return t,
        };
        let ask = crate::flows::drops::CreateAsk { draft, rpc_urls: self.rpc_urls(&chain), explorer: self.explorer_of(&chain), chain, wallet, owner };
        self.tx_preparing = true;
        Task::perform(crate::flows::drops::prepare_create(ask), |r| Message::TxReady(r.map(crate::flows::Handoff::new)))
    }

    #[cfg(feature = "drops")]
    pub(super) fn start_drop_claim(&mut self, drop: crate::api::drops::DropInfo, slot: crate::api::drops::DropSlot) -> Task<Message> {
        if let Err(t) = self.can_start(true) {
            return t;
        }
        let Some(chain) = self.chain_named(Some(&drop.chain)).filter(|c| c.name.eq_ignore_ascii_case(drop.chain.trim())) else {
            return self.toasts.show(Kind::Error, format!("This drop runs on {}, which isn't in your network list", drop.chain));
        };
        let Some(contract) = crate::flows::own_contract(drop.contract_address.as_deref()).or_else(|| crate::flows::drops::contract_of(&chain)) else { return self.toasts.show(Kind::Error, format!("Drops aren't available on {} yet", chain.name)) };
        let Some(wallet) = self.my_wallets().into_iter().find(|w| w.address.eq_ignore_ascii_case(&slot.address)) else {
            return self.toasts.show(Kind::Error, format!("This share is for {} — it isn't one of your wallets", crate::wallets::short_address(&slot.address)));
        };
        let Ok(id) = drop.drop_id.parse() else { return self.toasts.show(Kind::Error, "This drop's id isn't an on-chain one") };
        self.tx_subject = Some(drop.drop_id.clone());
        let job = crate::flows::drops::ClaimJob { rpc_urls: self.rpc_urls(&chain), chain, contract, id, slot, wallet };
        let review = crate::flows::drops::claim_review(&drop, &job);
        let explorer = self.explorer_of(&job.chain);
        let job = crate::flows::Job::DropClaim(Box::new(job));
        self.tx_kind = Some(job.kind());
        self.tx = Some(crate::components::tx_review::State::new(review, explorer, job));
        self.simulate_tx()
    }

    #[cfg(feature = "drops")]
    pub(super) fn start_drop_refund(&mut self, drop: crate::api::drops::DropInfo) -> Task<Message> {
        if let Err(t) = self.can_start(true) {
            return t;
        }
        let Some(chain) = self.chain_named(Some(&drop.chain)) else { return self.toasts.show(Kind::Error, "Pick a network first") };
        let Some(contract) = crate::flows::own_contract(drop.contract_address.as_deref()).or_else(|| crate::flows::drops::contract_of(&chain)) else { return self.toasts.show(Kind::Error, format!("Drops aren't available on {}", chain.name)) };
        let Some(wallet) = self.my_wallets().into_iter().find(|w| w.address.eq_ignore_ascii_case(&drop.creator_address)) else {
            return self.toasts.show(Kind::Error, format!("Use the wallet this drop was created from ({})", crate::wallets::short_address(&drop.creator_address)));
        };
        let Ok(id) = drop.drop_id.parse() else { return self.toasts.show(Kind::Error, "This drop's id isn't an on-chain one") };
        let job = crate::flows::drops::RefundJob { rpc_urls: self.rpc_urls(&chain), chain, contract, id, wallet };
        let review = crate::flows::drops::refund_review(&drop, &job);
        let explorer = self.explorer_of(&job.chain);
        let job = crate::flows::Job::DropRefund(Box::new(job));
        self.tx_kind = Some(job.kind());
        self.tx = Some(crate::components::tx_review::State::new(review, explorer, job));
        self.simulate_tx()
    }

    #[cfg(feature = "crowdfund")]
    pub(super) fn start_launch(&mut self, draft: crate::flows::crowdfund::LaunchDraft) -> Task<Message> {
        let owner = match self.can_start(true) {
            Ok(u) => u,
            Err(t) => return t,
        };
        let (chain, contract) = match self.chain_with("Campaigns", crate::flows::crowdfund::contract_of) {
            Ok(c) => c,
            Err(t) => return t,
        };
        let wallet = match self.signing_my_wallet() {
            Ok(w) => w,
            Err(t) => return t,
        };
        let explorer = self.explorer_of(&chain);
        let job = crate::flows::crowdfund::LaunchJob { draft, rpc_urls: self.rpc_urls(&chain), chain, contract, wallet, owner };
        let review = crate::flows::crowdfund::launch_review(&job);
        let job = crate::flows::Job::Launch(Box::new(job));
        self.tx_kind = Some(job.kind());
        self.tx = Some(crate::components::tx_review::State::new(review, explorer, job));
        self.simulate_tx()
    }

    #[cfg(feature = "crowdfund")]
    pub(super) fn start_back(&mut self, draft: crate::flows::crowdfund::BackDraft) -> Task<Message> {
        if let Err(t) = self.can_start(true) {
            return t;
        }
        let campaign = match &self.crowdfund.looked {
            Some(Ok(c)) if c.campaign_id.eq_ignore_ascii_case(draft.campaign_id.trim()) => c.clone(),
            Some(Err(e)) => return self.toasts.show(Kind::Error, e.clone()),
            _ => return self.toasts.show(Kind::Error, "Looking the campaign up — try again in a moment"),
        };
        let Some(chain) = self.chain_named(Some(&campaign.chain_name)).filter(|c| crate::evm::crowdfund::same_network(&c.name, &campaign.chain_name)) else {
            return self.toasts.show(Kind::Error, format!("This campaign runs on {}, which isn't in your network list", campaign.chain_name));
        };
        let wallet = match self.signing_my_wallet() {
            Ok(w) => w,
            Err(t) => return t,
        };
        let ask = crate::flows::crowdfund::BackAsk { draft, campaign, rpc_urls: self.rpc_urls(&chain), explorer: self.explorer_of(&chain), chain, wallet };
        self.tx_preparing = true;
        Task::perform(crate::flows::crowdfund::prepare_back(ask), |r| Message::TxReady(r.map(crate::flows::Handoff::new)))
    }

    /// 25 % … MAX (ContributeCampaignCard's handleQuickPercent): a share of what the active wallet
    /// can spend in the campaign's token, on the campaign's network. Reads only: no unlock.
    #[cfg(feature = "crowdfund")]
    pub(super) fn contribution_share(&mut self, pct: u8) -> Task<Message> {
        use crate::pages::crowdfund_page::CrowdfundMsg;
        let campaign = match &self.crowdfund.looked {
            Some(Ok(c)) => c.clone(),
            Some(Err(e)) => return self.toasts.show(Kind::Error, e.clone()),
            None => return self.toasts.show(Kind::Error, "Please select a campaign to support!"),
        };
        let Some(chain) = self.chain_named(Some(&campaign.chain_name)).filter(|c| crate::evm::crowdfund::same_network(&c.name, &campaign.chain_name)) else {
            return self.toasts.show(Kind::Error, format!("This campaign runs on {}, which isn't in your network list", campaign.chain_name));
        };
        let Some(owner) = self.active_wallet().map(|w| w.address.clone()).filter(|a| crate::wallets::chain_of(a) == "EVM") else {
            return self.toasts.show(Kind::Error, "Select an EVM wallet first");
        };
        let urls = self.rpc_urls(&chain);
        let expect = chain.evm_id();
        let (token, decimals, symbol, net) = (campaign.token_address.clone(), campaign.token_decimals, campaign.token_symbol.clone(), chain.name.clone());
        let id = campaign.campaign_id.clone();
        Task::perform(
            async move {
                use crate::evm::client::{address, format_units, token_or_native, Rpc};
                let rpc = Rpc::new(urls, expect).map_err(|e| e.to_string())?;
                let owner = address(&owner).map_err(|e| e.to_string())?;
                let token = token_or_native(token.as_deref()).map_err(|e| e.to_string())?;
                let can = crate::evm::balances::spendable(&rpc, owner, token).await.map_err(|e| format!("Couldn't read the balance: {e}"))?;
                if can.is_zero() {
                    return Err(format!("This wallet has no {symbol} to spend on {net}"));
                }
                Ok(format_units(crate::evm::balances::share(can, pct), decimals))
            },
            move |r| Message::Crowdfund(CrowdfundMsg::Shared(id.clone(), r)),
        )
    }

    #[cfg(feature = "crowdfund")]
    pub(super) fn start_campaign_action(&mut self, campaign: crate::api::crowdfund::CampaignRead, action: crate::flows::crowdfund::Action) -> Task<Message> {
        if let Err(t) = self.can_start(true) {
            return t;
        }
        let Some(chain) = self.chain_named(Some(&campaign.chain_name)).filter(|c| crate::evm::crowdfund::same_network(&c.name, &campaign.chain_name)) else {
            return self.toasts.show(Kind::Error, format!("This campaign runs on {}, which isn't in your network list", campaign.chain_name));
        };
        let ask = crate::flows::crowdfund::ActionAsk { action, campaign, rpc_urls: self.rpc_urls(&chain), explorer: self.explorer_of(&chain), chain, wallets: self.my_wallets() };
        self.tx_preparing = true;
        Task::perform(crate::flows::crowdfund::prepare_action(ask), |r| Message::TxReady(r.map(crate::flows::Handoff::new)))
    }

    /// The cheque form is filled in: check the wallet and the network, then the review window.
    pub(super) fn start_cheque(&mut self, draft: cheques_page::Draft) -> Task<Message> {
        use crate::components::tx_review;
        if self.tx.is_some() {
            return Task::none();
        }
        let Some(owner) = self.session.as_ref().filter(|s| !s.token.is_empty()).map(|s| s.user.username.clone()) else {
            return self.toasts.show(Kind::Error, "Sign in to write a cheque");
        };
        let Some(chain) = self.selected_chain().cloned() else { return self.toasts.show(Kind::Error, "Pick a network first") };
        if chain.solana || chain.evm_id().is_none() {
            return self.toasts.show(Kind::Error, format!("Cheques on {} aren't in the desktop client yet — pick an EVM network", chain.name));
        }
        let contract = match chain.contract("AccordoCheques").map(crate::evm::client::address) {
            Some(Ok(a)) => a,
            _ => return self.toasts.show(Kind::Error, format!("Accordo cheques aren't on {} yet — pick another network", chain.name)),
        };
        let wallet = match self.signing_wallet() {
            Ok(w) => w,
            Err(t) => return t,
        };
        let explorer = self.store.explorer_override(&chain.id).map(|e| e.trim_end_matches('/').to_string()).or_else(|| chain.explorer.clone());
        let job = crate::flows::cheques::Job { draft, rpc_urls: self.rpc_urls(&chain), chain, contract, wallet_address: wallet.address.clone(), wallet_vault: wallet.vault.clone(), explorer: explorer.clone(), owner };
        let review = crate::flows::cheques::review(&job, &wallet.name);
        self.module_cheque_opening(&job.draft.amount, &job.draft.token.symbol, &job.chain.name);
        let job = crate::flows::Job::Cheque(Box::new(job));
        self.tx_kind = Some(job.kind());
        self.tx = Some(tx_review::State::new(review, explorer, job));
        self.module_mark_tx();
        self.simulate_tx()
    }
}
