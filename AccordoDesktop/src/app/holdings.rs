// SPDX-License-Identifier: GPL-3.0-or-later
//! What the active wallet holds on a network, for the forms and the token picker alike: every
//! token of the network's list read at once through Multicall3 (evm/balances.rs, the web's
//! useWalletTokenBalances), kept per wallet and network.
//!
//! Read by itself while a page with a form is open — when the network, the wallet or the picked
//! token changes, every half minute, and right after a transaction (`pump`, after each update) —
//! so the form's token field shows the balance without opening the picker, and the picker opens
//! with the numbers already there.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use alloy_primitives::U256;
use iced::Task;

use super::{App, Message, Nav};
use crate::api::data::Token;
use crate::components::token_picker;

/// Older than this, a page with a form reads them again.
const FRESH: Duration = Duration::from_secs(25);
/// USD prices are asked again after this (and a token with no price isn't asked sooner).
const PRICE_FRESH: Duration = Duration::from_secs(300);
/// How often the open form page looks (the subscription's clock).
pub const TICK: Duration = Duration::from_secs(30);

/// One wallet on one network.
#[derive(Debug, Default, Clone)]
pub struct Holdings {
    /// Lowercase token address (token_picker::key; the gas coin under ZERO) → balance. A token
    /// that couldn't be read is absent.
    pub by_token: HashMap<String, U256>,
    /// The tokens asked for last time (one missing from the answer isn't asked again and again).
    pub asked: HashSet<String>,
    pub loading: bool,
    /// When the last answer came; None: never, or marked stale by a transaction.
    pub at: Option<Instant>,
}

impl Holdings {
    fn stale(&self) -> bool {
        self.at.is_none_or(|t| t.elapsed() > FRESH)
    }
}

/// Lowercase wallet address on a network.
pub type Key = (String, String);

impl App {
    /// The active wallet, when it's an EVM one (lowercase).
    fn holdings_owner(&self) -> Option<String> {
        self.active_wallet().map(|w| w.address.trim().to_lowercase()).filter(|a| crate::wallets::chain_of(a) == "EVM")
    }

    /// What the active wallet holds on this network, as far as known.
    pub fn holdings(&self, chain_id: &str) -> Option<&Holdings> {
        let owner = self.holdings_owner()?;
        self.holdings.get(&(chain_id.to_string(), owner))
    }

    /// A page whose form has a token field is on screen.
    pub(super) fn form_page(&self) -> bool {
        matches!(self.nav, Nav::Cheques | Nav::Invoices | Nav::Drops | Nav::Crowdfund) && self.session.is_some()
    }

    /// The tokens the forms have picked (they're read even when not in the network's list).
    fn picked_tokens(&self) -> Vec<&Token> {
        let mut out: Vec<&Token> = [&self.cheques.token, &self.cheques.get_token, &self.invoices.token].into_iter().flatten().collect();
        #[cfg(feature = "drops")]
        out.extend(self.drops.form.token.as_ref());
        #[cfg(feature = "crowdfund")]
        out.extend(self.crowdfund.token.as_ref());
        out
    }

    /// Reads the active wallet's balances of the network's whole token list, the custom tokens
    /// and the picked ones, in one go.
    pub(super) fn load_holdings(&mut self, chain_id: &str) -> Task<Message> {
        let Some(owner) = self.holdings_owner() else { return Task::none() };
        let (Some(rpc), Ok(owner_addr)) = (self.picker_rpc(chain_id), crate::evm::client::address(&owner)) else { return Task::none() };
        let mut tokens: Vec<String> = self.tokens.get(chain_id).into_iter().flatten().map(|t| token_picker::key(&t.address)).collect();
        tokens.extend(self.custom_tokens(chain_id).iter().map(|t| token_picker::key(&t.address)));
        if chain_id == self.chain_id {
            tokens.extend(self.picked_tokens().iter().map(|t| token_picker::key(&t.address)));
        }
        tokens.sort();
        tokens.dedup();
        let list: Vec<alloy_primitives::Address> = tokens.iter().filter_map(|t| crate::evm::client::address(t).ok()).collect();
        let key: Key = (chain_id.to_string(), owner);
        let entry = self.holdings.entry(key.clone()).or_default();
        if entry.loading {
            return Task::none();
        }
        entry.loading = true;
        entry.asked = tokens.into_iter().collect();
        Task::perform(
            async move {
                let found = crate::evm::balances::of(&rpc, owner_addr, &list).await;
                found.into_iter().map(|(a, b)| (format!("{a:#x}"), b)).collect::<HashMap<String, U256>>()
            },
            move |b| Message::Holdings(key.0.clone(), key.1.clone(), b),
        )
    }

    /// After every update: a form page on screen reads the balances it's missing or that went
    /// stale — the network or the wallet changed, a token was picked, time passed, a
    /// transaction went through. One read at a time per wallet and network.
    pub fn pump_holdings(&mut self) -> Task<Message> {
        if !self.form_page() {
            return Task::none();
        }
        let Some(owner) = self.holdings_owner() else { return Task::none() };
        let chain = self.chain_id.clone();
        if !self.chains.iter().any(|c| c.id == chain && !c.solana) {
            return Task::none();
        }
        let needs = match self.holdings.get(&(chain.clone(), owner)) {
            None => true,
            Some(h) if h.loading => false,
            // A picked token, or the network's list that arrived since, was never asked for.
            Some(h) => {
                let unasked = |t: &Token| !h.asked.contains(&token_picker::key(&t.address));
                h.stale() || self.picked_tokens().into_iter().any(unasked) || self.tokens.get(&chain).into_iter().flatten().any(unasked)
            }
        };
        let prices = self.want_prices();
        if needs { Task::batch([self.load_holdings(&chain), prices]) } else { prices }
    }

    /// An answer: kept, and handed to the token picker when it's open on that wallet and network.
    pub(super) fn holdings_arrived(&mut self, chain: String, owner: String, found: HashMap<String, U256>) -> Task<Message> {
        let h = self.holdings.entry((chain.clone(), owner.clone())).or_default();
        h.by_token = found;
        h.loading = false;
        h.at = Some(Instant::now());
        let by_token = h.by_token.clone();
        if self.holdings_owner().as_deref() == Some(owner.as_str()) {
            if let Some(st) = self.token_picker.as_mut().filter(|st| st.chain_id == chain) {
                // The picker's own read may have more (it asked for its whole list): merge.
                st.balances.extend(by_token);
            }
        }
        self.want_prices()
    }

    /// The picker read the balances itself: the forms get them too.
    pub(super) fn holdings_from_picker(&mut self, chain: &str, found: &HashMap<String, U256>) {
        let Some(owner) = self.holdings_owner() else { return };
        let h = self.holdings.entry((chain.to_string(), owner)).or_default();
        h.by_token.extend(found.iter().map(|(k, v)| (k.clone(), *v)));
        h.asked.extend(found.keys().cloned());
        h.at = Some(Instant::now());
    }

    /// USD prices of what the wallet holds on the form's network and of the picked tokens, from
    /// Accordo (DefiLlama behind it) — each asked at most every few minutes.
    pub(super) fn want_prices(&mut self) -> Task<Message> {
        let Some(api) = self.session.as_ref().map(crate::api::auth::Session::api) else { return Task::none() };
        let chain = self.chain_id.clone();
        let mut want: Vec<String> = self.picked_tokens().iter().map(|t| token_picker::key(&t.address)).collect();
        if let Some(h) = self.holdings(&chain) {
            want.extend(h.by_token.iter().filter(|(_, v)| !v.is_zero()).map(|(k, _)| k.clone()));
        }
        let mut keys: Vec<String> = want.iter().map(|k| crate::api::data::price_key(&chain, k)).collect();
        keys.sort();
        keys.dedup();
        keys.retain(|k| self.price_asked.get(k).is_none_or(|t| t.elapsed() > PRICE_FRESH));
        if keys.is_empty() {
            return Task::none();
        }
        let now = Instant::now();
        for k in &keys {
            self.price_asked.insert(k.clone(), now);
        }
        Task::perform(crate::api::data::prices(api, keys), |r| Message::Prices(r.unwrap_or_default()))
    }

    /// The USD price of a token on the form's network, when known.
    pub fn price_of(&self, chain_id: &str, token: &str) -> Option<f64> {
        self.prices.get(&crate::api::data::price_key(chain_id, &token_picker::key(token))).copied()
    }

    /// A transaction went through: what the wallets hold has changed — read again when shown.
    pub(super) fn holdings_spent(&mut self) {
        for h in self.holdings.values_mut() {
            h.at = None;
        }
    }

    /// The balance of a form's token field: ("0.0421 ETH", "≈ $123.45" when priced); "…" while
    /// it's being read, None with no EVM wallet or nothing picked.
    pub fn field_balance(&self, picked: Option<&Token>) -> Option<(String, Option<String>)> {
        let t = picked?;
        let h = self.holdings(&self.chain_id);
        match h.and_then(|h| h.by_token.get(&token_picker::key(&t.address))) {
            Some(v) => Some((format!("{} {}", token_picker::amount(*v, t.decimals, 6), t.symbol), token_picker::usd(*v, t.decimals, self.price_of(&self.chain_id, &t.address)))),
            None if self.holdings_owner().is_none() => None,
            None if h.is_none_or(|h| h.loading || !h.asked.contains(&token_picker::key(&t.address))) => Some(("…".into(), None)),
            // Asked, and the node had no answer for it.
            None => Some(("—".into(), None)),
        }
    }
}
