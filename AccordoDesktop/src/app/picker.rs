// SPDX-License-Identifier: GPL-3.0-or-later
//! The pickers: the token picker (search, custom tokens, balances, logos), contacts (the address
//! book, opened by @ in an address field) and chain settings (RPC endpoints checked against the
//! real node, explorers, testnet faucets).
//!
//! Responsible for: `TokenPickerMsg`, `ContactsMsg`, `ChainSettingsMsg`, and the chain/token
//! state in `App` (`tokens`, `chains`, RPC overrides, explorers). Not here: what a picked token is used for —
//! that is the page or flow that opened the picker (`app/flows.rs`, `pages/*`), and the picker's
//! drawing (`components/token_picker.rs`, `components/chain_settings.rs`).

use crate::app::{DataMsg};
use super::*;

impl App {
    pub(super) fn token_picker_update(&mut self, m: TokenPickerMsg) -> Task<Message> {
        use crate::components::token_picker::Purpose;
        if let TokenPickerMsg::Open(purpose) = m {
            let picked = match purpose {
                Purpose::Cheque => self.cheques.token.clone(),
                Purpose::SwapGet => self.cheques.get_token.clone(),
                Purpose::Invoice => self.invoices.token.clone(),
                #[cfg(feature = "crowdfund")]
                Purpose::Campaign => self.crowdfund.token.clone(),
                #[cfg(feature = "drops")]
                Purpose::Drop => self.drops.form.token.clone(),
                Purpose::Module => None,
            };
            self.token_picker = Some(token_picker::State::new(purpose, self.chain_id.clone(), picked.as_ref()));
            let mut tasks = vec![self.load_tokens(), iced::widget::operation::focus(token_picker::SEARCH), self.request_logos(), self.picker_check()];
            if purpose != Purpose::Module {
                tasks.push(self.picker_balances());
            }
            return Task::batch(tasks);
        }
        let Some(chain) = self.token_picker.as_ref().map(|s| s.chain_id.clone()) else { return Task::none() };
        let Some(st) = self.token_picker.as_mut() else { return Task::none() };
        match m {
            TokenPickerMsg::Open(_) => {}
            TokenPickerMsg::Close => {
                let module = st.purpose == Purpose::Module;
                self.token_picker = None;
                if module {
                    return self.module_token(None);
                }
            }
            TokenPickerMsg::Query(q) => {
                st.query = q;
                st.highlighted = None;
                return Task::batch([self.request_logos(), self.picker_check()]);
            }
            TokenPickerMsg::Move(d) => {
                let listed: &[Token] = self.tokens.get(&chain).map(Vec::as_slice).unwrap_or(&[]);
                let custom = self.custom_tokens(&chain);
                let st = self.token_picker.as_mut().expect("open");
                let entries = token_picker::visible(st, &custom, listed);
                if entries.is_empty() {
                    return Task::none();
                }
                let at = st.highlighted.as_ref().and_then(|k| entries.iter().position(|e| token_picker::key(&e.token.address) == *k));
                let next = match at {
                    Some(i) => (i as i32 + d).clamp(0, entries.len() as i32 - 1) as usize,
                    None => 0,
                };
                st.highlighted = Some(token_picker::key(&entries[next].token.address));
                st.add = None;
                return Task::batch([self.request_logos(), self.picker_check()]);
            }
            TokenPickerMsg::Highlight(k) => {
                st.highlighted = Some(k);
                st.add = None;
                return Task::batch([self.request_logos(), self.picker_check()]);
            }
            TokenPickerMsg::Use => {
                if st.add.is_some() {
                    return Task::none();
                }
                let listed: &[Token] = self.tokens.get(&chain).map(Vec::as_slice).unwrap_or(&[]);
                let custom = self.custom_tokens(&chain);
                let st = self.token_picker.as_ref().expect("open");
                let entries = token_picker::visible(st, &custom, listed);
                let Some(e) = st.highlighted.as_ref().and_then(|k| entries.iter().find(|e| token_picker::key(&e.token.address) == *k)).or(entries.first()) else { return Task::none() };
                let token = e.token.clone();
                let purpose = st.purpose;
                self.token_picker = None;
                let pick = match purpose {
                    Purpose::Cheque => Message::Cheques(cheques_page::ChequesMsg::Token(token)),
                    Purpose::SwapGet => Message::Cheques(cheques_page::ChequesMsg::GetToken(token)),
                    Purpose::Invoice => Message::Invoices(invoices_page::InvoicesMsg::Token(token)),
                    #[cfg(feature = "crowdfund")]
                    Purpose::Campaign => Message::Crowdfund(crowdfund_page::CrowdfundMsg::Token(token)),
                    #[cfg(feature = "drops")]
                    Purpose::Drop => Message::Drops(crate::pages::drops_page::DropsMsg::Token(token)),
                    Purpose::Module => return self.module_token(Some(&token)),
                };
                return Task::done(pick);
            }
            TokenPickerMsg::Balances(chain, b) => {
                if chain != st.chain_id {
                    return Task::none();
                }
                st.balances = b;
                st.balances_loading = false;
                let held: Vec<String> = st.balances.iter().filter(|(_, v)| !v.is_zero()).map(|(k, _)| k.clone()).collect();
                let (Some(session), false) = (&self.session, held.is_empty()) else { return self.request_logos() };
                let keys: Vec<String> = held.iter().map(|k| crate::api::data::price_key(&chain, k)).collect();
                let back: Vec<(String, String)> = keys.iter().cloned().zip(held).collect();
                let prices = Task::perform(data::prices(session.api(), keys), move |r| {
                    let m = r.unwrap_or_default();
                    Message::TokenPicker(TokenPickerMsg::Prices(back.iter().filter_map(|(key, k)| m.get(key).map(|p| (k.clone(), *p))).collect()))
                });
                return Task::batch([prices, self.request_logos()]);
            }
            TokenPickerMsg::Prices(p) => st.prices.extend(p),
            TokenPickerMsg::Checked(k, r) => {
                st.checks.insert(k, token_picker::Check::Done(r));
            }
            TokenPickerMsg::StartAdd => {
                let prefill = Some(st.query.trim().to_string()).filter(|q| q.starts_with("0x") && q.len() == 42).unwrap_or_default();
                st.add = Some(token_picker::AddForm { address: String::new(), info: None });
                let focus = iced::widget::operation::focus(token_picker::ADD_INPUT);
                if prefill.is_empty() {
                    return focus;
                }
                return Task::batch([focus, Task::done(Message::TokenPicker(TokenPickerMsg::AddAddress(prefill)))]);
            }
            TokenPickerMsg::AddAddress(a) => {
                let chain = st.chain_id.clone();
                if let Some(f) = st.add.as_mut() {
                    f.address = a.clone();
                    f.info = None;
                }
                let a = a.trim().to_string();
                let (Ok(addr), true) = (crate::evm::client::address(&a), a.len() == 42) else { return Task::none() };
                let Some(rpc) = self.picker_rpc(&chain) else { return Task::none() };
                return Task::perform(async move { crate::evm::balances::token_info(&rpc, addr).await.map_err(|e| e.to_string()) }, move |r| Message::TokenPicker(TokenPickerMsg::AddInfo(a.clone(), r)));
            }
            TokenPickerMsg::AddInfo(a, r) => {
                if let Some(f) = st.add.as_mut().filter(|f| f.address.trim().eq_ignore_ascii_case(&a)) {
                    f.info = Some((a, r));
                }
            }
            TokenPickerMsg::AddSave => {
                let Some(f) = st.add.take() else { return Task::none() };
                let Some((a, Ok((name, symbol, decimals)))) = f.info else { return Task::none() };
                let Ok(addr) = crate::evm::client::address(&a) else { return Task::none() };
                let address = addr.to_checksum(None);
                let chain = st.chain_id.clone();
                st.highlighted = Some(token_picker::key(&address));
                st.query.clear();
                if !self.custom_tokens(&chain).iter().any(|t| t.address.eq_ignore_ascii_case(&address)) {
                    let name = if name.trim().is_empty() { symbol.clone() } else { name };
                    self.store.add_custom_token(&chain, crate::store::app_store::TokenData { symbol: symbol.clone(), name, address, logo_uri: None, decimals: Some(decimals) });
                }
                return Task::batch([self.save_store(), self.toasts.show(Kind::Success, format!("{symbol} added")), self.picker_check(), self.picker_balances()]);
            }
            TokenPickerMsg::AddCancel => st.add = None,
            TokenPickerMsg::Copy(a) => return Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Info, "Address copied")]),
            TokenPickerMsg::Explorer(a) => {
                let chain = st.chain_id.clone();
                if let Some(ex) = self.chains.iter().find(|c| c.id == chain).and_then(|c| self.store.explorer_override(&c.id).or_else(|| c.explorer.clone())) {
                    let _ = open::that(format!("{}/token/{a}", ex.trim_end_matches('/')));
                }
            }
        }
        Task::none()
    }

    pub(super) fn contacts_update(&mut self, m: crate::components::contact_picker::ContactsMsg) -> Task<Message> {
        use crate::components::contact_picker::{self as cp, ContactsMsg};
        use iced::widget::operation::focus;
        if let ContactsMsg::Open(target) = m {
            self.switcher = None;
            self.contact_picker = Some(cp::State::new(target));
            return focus(cp::SEARCH);
        }
        let Some(st) = self.contact_picker.as_mut() else { return Task::none() };
        match m {
            ContactsMsg::Open(_) => {}
            ContactsMsg::Close => self.contact_picker = None,
            ContactsMsg::Query(q) => {
                st.query = q;
                st.index = 0;
            }
            ContactsMsg::Move(d) => {
                let n = cp::matches(&self.store.contacts, &st.query).len();
                if n > 0 && st.add.is_none() {
                    st.index = (st.index.min(n - 1) as i32 + d).rem_euclid(n as i32) as usize;
                }
            }
            ContactsMsg::Use => {
                let found = cp::matches(&self.store.contacts, &st.query);
                let Some(c) = found.get(st.index.min(found.len().saturating_sub(1))).map(|c| (*c).clone()) else { return Task::none() };
                return self.use_contact(c.name, c.address);
            }
            ContactsMsg::Pick(id) => {
                let Some(c) = self.store.contacts.iter().find(|c| c.id == id).cloned() else { return Task::none() };
                return self.use_contact(c.name, c.address);
            }
            ContactsMsg::Remove(id) => {
                let name = self.store.contacts.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default();
                self.store.remove_contact(&id);
                return Task::batch([self.save_store(), self.toasts.show(Kind::Info, format!("{name} deleted"))]);
            }
            ContactsMsg::Copy(a) => return Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Success, "Address copied")]),
            ContactsMsg::StartAdd => {
                st.start_add();
                let named = st.add.as_ref().is_some_and(|f| !f.name.is_empty());
                return focus(if named { cp::ADDRESS } else { cp::NAME });
            }
            ContactsMsg::Name(v) => {
                if let Some(f) = st.add.as_mut() {
                    f.name = v;
                }
            }
            ContactsMsg::Address(v) => {
                if let Some(f) = st.add.as_mut() {
                    f.address = v;
                }
            }
            ContactsMsg::Save => {
                let Some(f) = st.add.as_ref() else { return Task::none() };
                if let Some(p) = cp::problem(f) {
                    return self.toasts.show(Kind::Error, p);
                }
                let (name, address) = (f.name.trim().to_string(), f.address.trim().to_string());
                st.add = None;
                st.query.clear();
                st.index = 0;
                let picks = st.picks();
                self.store.add_contact(name.clone(), address.clone());
                let saved = Task::batch([self.save_store(), self.toasts.show(Kind::Success, format!("{name} saved"))]);
                return Task::batch([saved, if picks { self.use_contact(name, address) } else { focus(cp::SEARCH) }]);
            }
            ContactsMsg::CancelAdd => {
                st.add = None;
                return focus(cp::SEARCH);
            }
        }
        Task::none()
    }

    /// A contact was picked: the window closes and the field that asked gets it.
    fn use_contact(&mut self, name: String, address: String) -> Task<Message> {
        use crate::components::contact_picker::Target;
        let Some(target) = self.contact_picker.as_ref().map(|s| s.target).filter(|t| *t != Target::Manage) else { return Task::none() };
        self.contact_picker = None;
        Task::done(match target {
            Target::Manage => return Task::none(),
            Target::ChequeName(_) | Target::ChequeWallet(_) => Message::Cheques(cheques_page::ChequesMsg::Contact(target, name, address)),
            Target::InvoiceMerchant | Target::InvoiceClient | Target::InvoiceWallet | Target::InvoicePayerName(_) | Target::InvoicePayerWallet(_) => Message::Invoices(invoices_page::InvoicesMsg::Contact(target, name, address)),
            #[cfg(feature = "drops")]
            Target::DropWallet(i) => Message::Drops(crate::pages::drops_page::DropsMsg::Contact(i, address)),
        })
    }

    /// Writes the client-side state (off the UI thread); a failure is shown, not fatal.
    /// The active wallet the picker shows balances of (EVM only).
    pub(super) fn picker_wallet(&self) -> Option<&str> {
        self.active_wallet().map(|w| w.address.as_str()).filter(|a| crate::wallets::chain_of(a) == "EVM")
    }

    pub(super) fn picker_rpc(&self, chain_id: &str) -> Option<crate::evm::client::Rpc> {
        let chain = self.chains.iter().find(|c| c.id == chain_id)?;
        crate::evm::client::Rpc::new(self.rpc_urls(chain), chain.id.parse().ok()).ok()
    }

    /// Reads name / symbol / decimals from the highlighted token's contract, once.
    pub(super) fn picker_check(&mut self) -> Task<Message> {
        let Some(st) = &self.token_picker else { return Task::none() };
        let listed: &[Token] = self.tokens.get(&st.chain_id).map(Vec::as_slice).unwrap_or(&[]);
        let custom = self.custom_tokens(&st.chain_id);
        let entries = token_picker::visible(st, &custom, listed);
        let Some(e) = st.highlighted.as_ref().and_then(|k| entries.iter().find(|e| token_picker::key(&e.token.address) == *k)).or(entries.first()) else { return Task::none() };
        let k = token_picker::key(&e.token.address);
        if token_picker::is_native(&e.token) || st.checks.contains_key(&k) {
            return Task::none();
        }
        let (Some(rpc), Ok(addr)) = (self.picker_rpc(&st.chain_id), crate::evm::client::address(&e.token.address)) else { return Task::none() };
        let chain = st.chain_id.clone();
        let mut tasks = vec![];
        if let Some(st) = self.token_picker.as_mut() {
            st.checks.insert(k.clone(), token_picker::Check::Running);
            if !st.prices.contains_key(&k) {
                if let Some(session) = &self.session {
                    let key = crate::api::data::price_key(&chain, &k);
                    let kk = k.clone();
                    tasks.push(Task::perform(data::prices(session.api(), vec![key.clone()]), move |r| {
                        let p = r.ok().and_then(|m| m.get(&key).copied());
                        Message::TokenPicker(TokenPickerMsg::Prices(p.map(|p| HashMap::from([(kk.clone(), p)])).unwrap_or_default()))
                    }));
                }
            }
        }
        tasks.push(Task::perform(async move { crate::evm::balances::token_info(&rpc, addr).await.map_err(|e| e.to_string()) }, move |r| Message::TokenPicker(TokenPickerMsg::Checked(k.clone(), r))));
        Task::batch(tasks)
    }

    /// Balances of the whole list for the active wallet (then prices of what's held).
    pub(super) fn picker_balances(&mut self) -> Task<Message> {
        let Some(chain) = self.token_picker.as_ref().map(|s| s.chain_id.clone()) else { return Task::none() };
        let (Some(owner), Some(rpc)) = (self.picker_wallet().and_then(|a| crate::evm::client::address(a).ok()), self.picker_rpc(&chain)) else { return Task::none() };
        let mut list: Vec<alloy_primitives::Address> = self.tokens.get(&chain).map(|l| l.iter().filter_map(|t| crate::evm::client::address(&t.address).ok()).collect()).unwrap_or_default();
        list.extend(self.custom_tokens(&chain).iter().filter_map(|t| crate::evm::client::address(&t.address).ok()));
        if let Some(st) = self.token_picker.as_mut() {
            st.balances_loading = true;
        }
        Task::perform(
            async move {
                let found = crate::evm::balances::of(&rpc, owner, &list).await;
                found.into_iter().map(|(a, b)| (format!("{a:#x}"), b)).collect::<HashMap<String, alloy_primitives::U256>>()
            },
            move |b| Message::TokenPicker(TokenPickerMsg::Balances(chain.clone(), b)),
        )
    }

    /// Fetches the pictures of the rows on screen (and of the picked tokens) not fetched yet.
    pub(super) fn request_logos(&mut self) -> Task<Message> {
        let mut want: Vec<String> = Vec::new();
        #[cfg(feature = "crowdfund")]
        let picked = [&self.cheques.token, &self.invoices.token, &self.crowdfund.token];
        #[cfg(not(feature = "crowdfund"))]
        let picked = [&self.cheques.token, &self.invoices.token];
        for t in picked.into_iter().flatten() {
            want.extend(t.logo.clone());
        }
        if let Some(st) = &self.token_picker {
            let listed: &[Token] = self.tokens.get(&st.chain_id).map(Vec::as_slice).unwrap_or(&[]);
            let custom = self.custom_tokens(&st.chain_id);
            let entries = token_picker::visible(st, &custom, listed);
            let at = st.highlighted.as_ref().and_then(|k| entries.iter().position(|e| token_picker::key(&e.token.address) == *k)).unwrap_or(0);
            // The first screenful, and a few around the highlighted row.
            for (i, e) in entries.iter().enumerate() {
                if i < 40 || (i + 8 >= at && i <= at + 8) {
                    want.extend(e.token.logo.clone());
                }
            }
        }
        let mut tasks = Vec::new();
        for url in want {
            if self.token_logos.contains_key(&url) {
                continue;
            }
            self.token_logos.insert(url.clone(), None); // in flight: not asked again
            tasks.push(Task::perform(crate::media::fetch_logo(url.clone()), move |h| Message::Data(DataMsg::TokenLogo(url.clone(), h))));
        }
        Task::batch(tasks)
    }

    pub(super) fn chain_settings_update(&mut self, m: ChainSettingsMsg) -> Task<Message> {
        if let ChainSettingsMsg::Open = m {
            self.close_app_settings();
            self.switcher = None;
            self.chain_settings = Some(chain_settings::State::new(self.chain_id.clone(), self.chain_saved(&self.chain_id)));
            return self.faucet_tokens_task();
        }
        let Some(current) = self.chain_settings.as_ref().map(|s| s.chain_id.clone()) else { return Task::none() };
        let saved = self.chain_saved(&current);
        let chain = self.chains.iter().find(|c| c.id == current).cloned();
        let st = self.chain_settings.as_mut().expect("open");
        match m {
            ChainSettingsMsg::Open => {}
            ChainSettingsMsg::Close => self.chain_settings = None,
            ChainSettingsMsg::Pick(id) => {
                let next = (self.store.rpc_override(&id).map(|l| l.join("\n")).unwrap_or_default(), self.store.explorer_override(&id).unwrap_or_default());
                st.pick(id, &saved, next);
                return self.faucet_tokens_task();
            }
            ChainSettingsMsg::Query(q) => st.query = q,
            ChainSettingsMsg::Rpc(a) => st.rpc.perform(a),
            ChainSettingsMsg::Explorer(e) => st.explorer = e,
            ChainSettingsMsg::CopyContract(a) => return Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Success, "Address copied")]),
            ChainSettingsMsg::OpenContract(a) => {
                let base = Some(st.explorer.trim().trim_end_matches('/').to_string()).filter(|e| !e.is_empty()).or_else(|| chain.as_ref().and_then(|c| c.explorer.clone()));
                let Some(base) = base else { return self.toasts.show(Kind::Info, "This network has no block explorer — set one above") };
                let url = format!("{base}/address/{a}");
                if open::that_detached(&url).is_err() {
                    return self.toasts.show(Kind::Info, format!("Open {url} in your browser"));
                }
            }
            ChainSettingsMsg::Test => {
                let Some(chain) = chain else { return Task::none() };
                let mut urls = st.rpc_lines();
                for u in &chain.rpc_urls {
                    if !urls.contains(u) {
                        urls.push(u.clone());
                    }
                }
                let mut tasks = Vec::new();
                for url in urls.into_iter().filter(|u| u.starts_with("http") || u.starts_with("ws")).take(chain_settings::MAX_RPCS * 2) {
                    st.probes.insert(url.clone(), chain_settings::ProbeState::Running);
                    let (solana, id) = (chain.solana, chain.id.clone());
                    tasks.push(Task::perform(crate::chain_rpc::probe(url.clone(), solana, id), move |r| Message::ChainSettings(ChainSettingsMsg::Probed(url.clone(), r))));
                }
                return Task::batch(tasks);
            }
            ChainSettingsMsg::Probed(url, r) => {
                st.probes.insert(url, chain_settings::ProbeState::Done(r));
            }
            ChainSettingsMsg::Save => {
                if st.problem().is_some() {
                    return Task::none();
                }
                let rpc = Some(st.rpc_lines()).filter(|l| !l.is_empty());
                let explorer = Some(st.explorer.trim().trim_end_matches('/').to_string()).filter(|e| !e.is_empty());
                st.explorer = explorer.clone().unwrap_or_default();
                st.drafts.remove(&current);
                self.store.set_chain_settings(&current, rpc, explorer);
                self.refresh_explorers();
                let name = chain.map(|c| c.name).unwrap_or_default();
                return Task::batch([self.save_store(), self.toasts.show(Kind::Success, format!("Saved {name} settings"))]);
            }
            ChainSettingsMsg::Discard => st.load(saved),
            ChainSettingsMsg::Reset => {
                st.load((String::new(), String::new()));
                self.store.set_chain_settings(&current, None, None);
                self.refresh_explorers();
                let name = chain.map(|c| c.name).unwrap_or_default();
                return Task::batch([self.save_store(), self.toasts.show(Kind::Info, format!("{name}: back to the defaults"))]);
            }
            ChainSettingsMsg::Use => {
                if let Some(chain) = chain {
                    return Task::done(Message::SelectChain(chain));
                }
            }
            ChainSettingsMsg::Drip(d) => {
                let Some(chain) = chain else { return Task::none() };
                let rpc = self.rpc_urls(&chain).into_iter().find(|u| u.starts_with("http"));
                let (Some(rpc), Some(to)) = (rpc, self.faucet_target(&chain.id).map(str::to_string)) else {
                    return self.toasts.show(Kind::Error, "No HTTP RPC or wallet for the faucet");
                };
                if let Some(st) = self.chain_settings.as_mut() {
                    st.dripping = Some(d.label(chain.solana));
                }
                return Task::perform(crate::chain_rpc::drip(rpc, chain.solana, to, d), |r| Message::ChainSettings(ChainSettingsMsg::Dripped(r)));
            }
            ChainSettingsMsg::Dripped(r) => {
                st.dripping = None;
                let solana = chain.is_some_and(|c| c.solana);
                return match r {
                    Ok(label) => self.toasts.show(Kind::Success, label),
                    Err(e) => self.toasts.show(Kind::Error, format!("Faucet failed: {e}. Is ./{}/start.sh running?", if solana { "local_solana" } else { "local_anvil" })),
                };
            }
            ChainSettingsMsg::FaucetTokens(id, list) => st.faucet_tokens = Some((id, list)),
        }
        Task::none()
    }

    /// What's saved for a network's settings: (RPC URLs one per line, explorer); "" = default.
    pub(super) fn chain_saved(&self, chain_id: &str) -> chain_settings::Saved {
        (self.store.rpc_override(chain_id).map(|l| l.join("\n")).unwrap_or_default(), self.store.explorer_override(chain_id).unwrap_or_default())
    }

    /// Explorer links in the chat, by network name: the user's explorer over the server's. And the
    /// networks drop cards read the contract on (the user's RPCs over the server's).
    pub(super) fn refresh_explorers(&mut self) {
        self.chat.explorers = self
            .chains
            .iter()
            .filter_map(|c| self.store.explorer_override(&c.id).map(|e| e.trim_end_matches('/').to_string()).or_else(|| c.explorer.clone()).map(|e| (c.name.to_lowercase(), e)))
            .collect();
        self.chat.fund.chains = self
            .chains
            .iter()
            .filter(|c| !c.solana)
            .map(|c| {
                let drops = c.contract("AccordoDrop").and_then(|a| crate::evm::client::address(a).ok()).filter(|a| !a.is_zero());
                let r = crate::components::chat::fund_cards::ChainRef { name: c.name.clone(), rpc_urls: self.rpc_urls(c), chain_id: c.evm_id(), native_symbol: c.native_symbol.clone(), drops };
                (crate::components::chat::fund_cards::chain_key(&c.name), r)
            })
            .collect();
    }

    /// Where the faucet of a local network sends: the active wallet, if it's an EVM one.
    pub(super) fn faucet_target(&self, chain_id: &str) -> Option<&str> {
        let chain = self.chains.iter().find(|c| c.id == chain_id)?;
        if chain.solana {
            return None; // no Solana wallets in the desktop app yet
        }
        self.active_wallet().map(|w| w.address.as_str()).filter(|a| crate::wallets::chain_of(a) == "EVM")
    }

    /// The local chain's test tokens, fetched once when its settings are opened.
    pub(super) fn faucet_tokens_task(&self) -> Task<Message> {
        let (Some(st), Some(session)) = (&self.chain_settings, &self.session) else { return Task::none() };
        let Some(chain) = self.chains.iter().find(|c| c.id == st.chain_id) else { return Task::none() };
        if !chain.local || chain.solana || st.faucet_tokens.as_ref().is_some_and(|(id, _)| *id == chain.id) {
            return Task::none();
        }
        let id = chain.id.clone();
        Task::perform(data::tokens(session.api(), id.clone()), move |r| {
            let zero = format!("0x{}", "0".repeat(40));
            let list = r.map(|(_, l)| l.into_iter().filter(|t| !t.address.eq_ignore_ascii_case(&zero)).collect()).unwrap_or_default();
            Message::ChainSettings(ChainSettingsMsg::FaucetTokens(id.clone(), list))
        })
    }
}
