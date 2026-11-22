// SPDX-License-Identifier: GPL-3.0-or-later
//! Sessions and accounts: signing in (`enter`), leaving (`leave`), switching between the accounts
//! that stay signed in (`can_switch`, `switch_to`), the privacy screen (`hide`), and what happens
//! when the chat key of the open account changes (`chat_key_changed`).
//!
//! Responsible for: what happens *around* an account (the messages are `msg/session.rs`) — the session in `App`, the unsaved-changes
//! guard (`refresh_unsaved`), and per-account data going in and out of the store. Not here: the
//! sign-in form (`pages/login.rs`), the token and its calls (`api/auth.rs`), the remembered
//! session on disk (`api/session_store.rs`), the window frame (`view` in `app/mod.rs`), and an
//! account's wallets (`app/wallet.rs`).

use crate::app::{DataMsg};
use super::*;
impl App {

    /// The chat reopens its encrypted rooms with the chat key now on this computer.
    pub(super) fn chat_key_changed(&mut self) -> Task<Message> {
        let Some(session) = &self.session else { return Task::none() };
        let api = session.api();
        chat_page::set_device_key(&mut self.chat, self.chat_key.keypair().cloned(), &api)
    }

    /// Into the app with a signed-in session: the networks for the top bar, the chat list.
    pub(super) fn enter(&mut self, session: Session) -> Task<Message> {
        tracing::info!("signed in as @{} (id {}) on {}", session.user.username, session.user.id, session.base);
        self.chat = chat_page::State::new(session.user.id, session.key());
        self.chat.can_create_public = session.user.can_create_public_rooms || session.user.is_admin;
        self.chat.is_admin = session.user.is_admin;
        self.chat.modules = self.modules();
        self.chat.me_name = session.user.username.clone();
        #[cfg(feature = "drops")]
        {
            self.drops = Default::default();
            self.drops.web_base = crate::api::client::normalize_base(&session.base);
        }
        #[cfg(feature = "crowdfund")]
        {
            self.crowdfund.explore = None;
            self.crowdfund.mine = None;
        }
        self.apply_prefs();
        self.chat.my_wallets = session
            .user
            .wallets
            .iter()
            .flatten()
            .filter_map(|w| {
                let addr = w.get("address").and_then(|a| a.as_str())?.to_lowercase();
                let name = w.get("name").and_then(|n| n.as_str()).unwrap_or("Wallet").to_string();
                Some((addr, name))
            })
            .collect();
        self.nav = match self.store.start_page.as_str() {
            "cheques" => Nav::Cheques,
            "invoices" => Nav::Invoices,
            "drops" => Nav::Drops,
            "crowdfund" => Nav::Crowdfund,
            "settings" => Nav::Settings,
            _ => Nav::Chat,
        };
        if !self.modules().page_shown(self.nav) {
            self.nav = Nav::Chat; // the start page is a section that's been hidden or closed since
        }
        // On this computer's list of accounts, as the open one; a fresh sign-in (or the session
        // from the old single entry) into the credential store.
        let persist = match self.persist_next.take() {
            Some(moved_from) if !session.token.is_empty() => {
                let saved = Saved { moved_from, ..session.saved() };
                Task::perform(session_store::save(session.key(), saved), |v| Message::Session(SessionMsg::Saved(v)))
            }
            _ => Task::none(),
        };
        if !session.token.is_empty() {
            self.store.remember_account(session.user.id, &session.user.username, &session.base);
            self.store.set_account_avatar(session.user.id, &session.base, session.user.picture());
        }
        self.login.back_to = None;
        self.reset_module_console(Some(session.user.id));
        // This account's local data (check vault…) and its wallets; another account's keys go.
        self.lock_wallets();
        let wallets = account_wallets(&session.user);
        self.store.set_account_owner(&session.key(), session.user.id, !wallets.is_empty());
        let fixed = self.store.set_wallets(wallets);
        self.settings.vault.lock = self.store.wallet_lock_minutes.and_then(|m| settings_page::LOCK_OPTIONS.iter().position(|(_, v)| *v == m)).unwrap_or(2);
        let api = session.api();
        let fixed = match fixed {
            Some(crate::store::app_store::StoreEffect::SyncWallets(list)) if !session.token.is_empty() => Task::perform(auth::update_wallets(api.clone(), list), |v| Message::Data(DataMsg::WalletsSynced(v))),
            _ => Task::none(),
        };
        let saved = self.save_store();
        let chains = Task::perform(data::chains(api.clone()), |v| Message::Data(DataMsg::ChainsLoaded(v)));
        let chat = chat_page::load(&mut self.chat, &api);
        self.chat_key = ChatKey::Loading;
        let key = Task::perform(chat_key::load(session.key(), session.previous_key(), session.user.id, session.user.e2ee_public_key.clone()), |v| Message::Session(SessionMsg::ChatKeyLoaded(v)));
        // Anything live on-chain that Accordo couldn't take last time: try again quietly.
        self.unsaved = crate::store::unsaved::kinds(&session.user.username, &session.base);
        let unsaved = if !session.token.is_empty() && !self.unsaved.is_empty() {
            self.unsaved_retry = Some(false);
            Task::perform(crate::store::unsaved::retry_all(api.clone(), session.user.username.clone()), |v| Message::Data(DataMsg::UnsavedRetried(v)))
        } else {
            Task::none()
        };
        self.session = Some(session);
        let orgs = self.load_orgs();
        // Testing: the wallets open with state.json's test password, if it's there (debug builds).
        let dev = self.dev_unlock().unwrap_or_else(Task::none);
        Task::batch([persist, chains, chat, key, fixed, saved, unsaved, orgs, dev])
    }

    /// Covers the window (and locks the wallets, if that's on).
    pub(super) fn hide(&mut self) {
        if self.session.is_none() || self.hidden {
            return;
        }
        self.hidden = true;
        self.switcher = None;
        self.hidden_locked = self.store.lock_wallets_on_hide && self.wallet_session.unlocked();
        if self.hidden_locked {
            self.lock_wallets();
        }
    }

    /// Out of the open account, keeping it signed in on this computer: everything of it in memory
    /// goes (its chat, lists, unlocked wallet keys).
    pub(super) fn leave(&mut self) {
        self.session = None;
        self.lists = Default::default();
        self.attach = None;
        self.admin = Default::default();
        self.profile_open = false;
        self.chat = chat_page::State::new(0, String::new());
        self.wallet_open = false;
        self.key_backup = None;
        self.picker = None;
        self.chat_key = ChatKey::Loading;
        self.settings = settings_page::State::default();
        self.orgs = Default::default();
        self.lock_wallets();
        self.unsaved.clear();
        self.unsaved_retry = None;
        self.persist_next = None;
        self.reset_module_console(None);
        let api_url = self.login.api_url.clone();
        self.login = login::State { api_url, ..Default::default() };
    }

    /// Not while a transaction is being reviewed or sent: it belongs to the open account.
    pub(super) fn can_switch(&mut self) -> Result<(), Task<Message>> {
        if self.tx.is_some() || self.tx_preparing {
            return Err(self.toasts.show(Kind::Error, "Finish the transaction first"));
        }
        Ok(())
    }

    /// Opens a saved account: its session from the credential store, checked with its server.
    pub(super) fn switch_to(&mut self, key: String) -> Task<Message> {
        tracing::info!("accounts: switching to {key}");
        self.leave();
        self.restoring = true;
        self.store.active_account = Some(key.clone());
        if let Some(a) = self.store.account(&key) {
            self.login.api_url = a.base.clone();
        }
        let k = key.clone();
        let previous = self.store.account(&key).map(|a| a.previous_key());
        Task::batch([self.save_store(), Task::perform(session_store::load_account(key, previous), move |s| Message::Session(SessionMsg::Loaded(k.clone(), s)))])
    }

    /// The banner's list again, from the file (after a record is kept or saved).
    pub(super) fn refresh_unsaved(&mut self) {
        self.unsaved = self.session.as_ref().map(|s| crate::store::unsaved::kinds(&s.user.username, &s.base)).unwrap_or_default();
    }

    /// The session's messages — see [`SessionMsg`].
    pub(super) fn session_update(&mut self, m: SessionMsg) -> Task<Message> {
        match m {
            SessionMsg::ToggleMenu => self.profile_open = !self.profile_open,
            SessionMsg::Logout => {
                if let Err(t) = self.can_switch() {
                    return t;
                }
                let key = self.session.as_ref().map(Session::key);
                tracing::info!("accounts: log out of {}", key.as_deref().unwrap_or("-"));
                let who = key.as_deref().and_then(|k| self.store.account(k)).map(|a| format!("@{} on {}", a.username, a.server()));
                self.leave();
                if let Some(k) = &key {
                    self.store.forget_account(k);
                }
                let saved = self.save_store();
                let forget = Task::perform(session_store::clear(key.clone()), |_| Message::None);
                // Another account signed in here: open it — and say so, so it isn't taken for a switch.
                if let Some(next) = key.as_deref().and_then(|k| self.store.next_account(k)) {
                    let opened = self.store.account(&next).map(|a| format!("@{} on {}", a.username, a.server())).unwrap_or_default();
                    let note = self.toasts.show(Kind::Info, format!("Logged out of {} and took it off this computer. Opened {opened}.", who.unwrap_or_default()));
                    return Task::batch([saved, forget, note, self.switch_to(next)]);
                }
                return Task::batch([saved, forget, operation::focus(login::FIELD_ID)]);
            }
            SessionMsg::SwitchTo(key) => {
                self.profile_open = false;
                if self.session.as_ref().is_some_and(|s| s.key() == key) {
                    return Task::none();
                }
                if let Err(t) = self.can_switch() {
                    return t;
                }
                let Some(account) = self.store.account(&key).cloned() else { return Task::none() };
                if account.signed_out {
                    // Its token was refused: the sign-in form, filled in.
                    self.leave();
                    self.login.identifier = account.username.clone();
                    self.login.api_url = account.base.clone();
                    return operation::focus(login::FIELD_PASSWORD);
                }
                return self.switch_to(key);
            }
            SessionMsg::Loaded(key, saved) => match saved {
                Some(saved) => {
                    self.login.api_url = saved.base.clone();
                    if saved.moved_from.is_some() {
                        self.persist_next = Some(saved.moved_from.clone());
                    }
                    return Task::perform(auth::restore(saved.base, saved.token), |r| Message::Session(SessionMsg::Restored(r.map(Box::new))));
                }
                None => {
                    // Nothing in the credential store for it (removed by hand, another keyring).
                    self.restoring = false;
                    self.store.mark_signed_out(&key);
                    let name = self.store.account(&key).map(|a| a.username.clone()).unwrap_or_default();
                    self.login.identifier = name.clone();
                    return Task::batch([self.save_store(), self.toasts.show(Kind::Info, format!("Sign in to @{name} again")), operation::focus(login::FIELD_PASSWORD)]);
                }
            },
            SessionMsg::AddAccount => {
                if let Err(t) = self.can_switch() {
                    return t;
                }
                // The open account stays the one opened next time until the new one signs in.
                let back = self.session.as_ref().map(Session::key);
                self.leave();
                self.login.back_to = back;
                return operation::focus(login::FIELD_ID);
            }
            SessionMsg::Remove(key) => {
                tracing::info!("accounts: ✕ on the sign-in screen for {key}");
                if self.session.as_ref().is_some_and(|s| s.key() == key) {
                    return Task::none(); // the open one signs out with Logout
                }
                self.store.forget_account(&key);
                if self.login.back_to.as_deref() == Some(key.as_str()) {
                    self.login.back_to = None;
                }
                return Task::batch([self.save_store(), Task::perform(session_store::clear(Some(key)), |_| Message::None)]);
            }
            SessionMsg::Restored(result) => {
                self.restoring = false;
                match result {
                    Ok(session) => return self.enter(*session),
                    // The token expired or was revoked: forget it and ask to sign in again. The
                    // account stays in the list, marked; the others aren't touched.
                    Err(e) if matches!(e.status, Some(401 | 403)) => {
                        self.persist_next = None;
                        let key = self.store.active_account.clone();
                        if let Some(a) = key.as_deref().and_then(|k| self.store.account(k)).cloned() {
                            self.store.mark_signed_out(&a.key());
                            self.login.identifier = a.username.clone();
                        }
                        return Task::batch([
                            self.save_store(),
                            Task::perform(session_store::clear(key), |_| Message::None),
                            self.toasts.show(Kind::Info, "Your session has expired. Please sign in again."),
                            operation::focus(if self.login.identifier.is_empty() { login::FIELD_ID } else { login::FIELD_PASSWORD }),
                        ]);
                    }
                    // Server unreachable: keep the saved session for the next launch.
                    Err(e) => return Task::batch([self.toasts.show(Kind::Error, e.message), operation::focus(login::FIELD_ID)]),
                }
            }
            SessionMsg::Saved(Err(e)) => {
                return self.toasts.show(Kind::Info, format!("Couldn't remember the sign-in on this computer ({e}). You'll be asked to sign in next time."));
            }
            SessionMsg::Saved(Ok(())) => {}
            SessionMsg::ChatKeyLoaded(loaded) => {
                let account_key = self.session.as_ref().and_then(|s| s.user.e2ee_public_key.clone());
                self.chat_key = ChatKey::from_loaded(loaded, account_key.as_deref());
                // The old shared pins file may have moved to this account with the key.
                self.chat.keys.reload_pins();
                match &self.chat_key {
                    ChatKey::Ready(_) => tracing::info!("chat key: this computer has the account's key"),
                    ChatKey::Missing => tracing::info!("chat key: not on this computer"),
                    ChatKey::Different(_) => tracing::warn!("chat key: this computer holds a different key than the account's"),
                    ChatKey::NoStore(e) => tracing::warn!("chat key: secret store unavailable: {e}"),
                    ChatKey::Loading => {}
                }
                return self.chat_key_changed();
            }
            SessionMsg::ChatKeyInstalled(Ok(keypair)) => {
                self.chat_key = ChatKey::Ready(keypair);
                self.key_backup = None;
                self.picker = None;
                tracing::info!("chat key installed on this computer");
                let reopen = self.chat_key_changed();
                return Task::batch([reopen, self.toasts.show(Kind::Success, "Chat key restored — your encrypted chats open here now")]);
            }
            SessionMsg::ChatKeyInstalled(Err(e)) => {
                tracing::warn!("chat key: {e}");
                if let Some(state) = self.key_backup.as_mut() {
                    state.update(KeyBackupMsg::Done(Err(e)));
                } else if let Some(picker) = self.picker.as_mut() {
                    picker.error = Some(e);
                } else {
                    return self.toasts.show(Kind::Error, e);
                }
            }
        }
        Task::none()
    }
}
