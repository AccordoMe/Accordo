// SPDX-License-Identifier: GPL-3.0-or-later
//! Data the app asked for and is waiting on: the networks and their tokens, the account's wallets
//! after a change, `/me` again, a token's logo, an avatar picture, and the store being written.
//!
//! Responsible for: the state these loads fill (the messages are `msg/data.rs`) (`chains`, `tokens`, `logos`,
//! `store`, the avatar cache). Not here: the requests themselves (they are made by whoever needs
//! the data — `picker.rs`, `wallet.rs`, `enter`), the pickers' UI (`picker.rs`) and the avatar
//! card (`avatar.rs`).

use super::*;
impl App {

    /// The messages of this area — see [`DataMsg`].
    pub(super) fn data_update(&mut self, m: DataMsg) -> Task<Message> {
        match m {
            DataMsg::AvatarLoaded(k, pic) => crate::avatars::finish(k, pic),
            DataMsg::AvatarPump => {}
            DataMsg::UserRefreshed(Ok(user)) => {
                if let Some(s) = self.session.as_mut() {
                    // A new plan (Enterprise given or ended): what the rail offers changes with it.
                    let plan_changed = s.user.subscription != user.subscription;
                    s.user = user;
                    let saved = if self.store.set_account_avatar(s.user.id, &s.base, s.user.picture()) { self.save_store() } else { Task::none() };
                    let orgs = match self.session.as_ref().filter(|s| plan_changed && !s.token.is_empty()) {
                        Some(s) => Task::perform(crate::api::orgs::mine(s.api()), |r| Message::Org(OrgMsg::Loaded(r.map_err(|e| e.message)))),
                        None => Task::none(),
                    };
                    return Task::batch([saved, orgs]);
                }
            }
            DataMsg::UserRefreshed(Err(_)) => {}
            DataMsg::TokenLogo(url, handle) => {
                self.token_logos.insert(url, handle);
            }
            DataMsg::WalletsSynced(Ok(user)) => {
                if let Some(s) = self.session.as_mut() {
                    s.user = user;
                }
            }
            DataMsg::WalletsSynced(Err(e)) => tracing::warn!("wallets: couldn't save the re-sealed vaults: {}", e.message),
            DataMsg::PublicWalletsSaved(shown, before, result) => {
                let Some(s) = self.session.as_mut() else { return Task::none() };
                return match result {
                    Ok(user) => {
                        s.user = user;
                        self.toasts.show(Kind::Success, if shown { "Shown in your profile" } else { "Hidden from your profile" })
                    }
                    Err(e) => {
                        s.user.public_wallets = Some(before);
                        self.toasts.show(Kind::Error, format!("Couldn't change your profile: {}", e.message))
                    }
                };
            }
            DataMsg::ChainsLoaded(Ok(chains)) => {
                // Keep the pick if the server still lists it; else Base, else the first one.
                if !chains.iter().any(|c| c.id == self.chain_id) {
                    self.chain_id = chains.iter().find(|c| c.id == DEFAULT_CHAIN_ID).or(chains.first()).map(|c| c.id.clone()).unwrap_or_default();
                }
                self.chains = chains;
                self.refresh_explorers();
                return self.load_tokens();
            }
            DataMsg::ChainsLoaded(Err(e)) => return self.toasts.show(Kind::Error, format!("Couldn't load networks: {}", e.message)),
            DataMsg::TokensLoaded(Ok((chain_id, mut list))) => {
                self.tokens_loading.remove(&chain_id);
                // No list for this network: its native coin only.
                if list.is_empty() {
                    if let Some(chain) = self.chains.iter().find(|c| c.id == chain_id) {
                        list.push(Token::native(chain));
                    }
                }
                let picker_waits = self.token_picker.as_ref().is_some_and(|p| p.chain_id == chain_id);
                self.tokens.insert(chain_id, list);
                if picker_waits {
                    return Task::batch([self.picker_balances(), self.request_logos(), self.picker_check()]);
                }
                return self.request_logos();
            }
            DataMsg::TokensLoaded(Err(e)) => {
                self.tokens_loading.clear();
                return self.toasts.show(Kind::Error, format!("Couldn't load tokens: {}", e.message));
            }
            DataMsg::UnsavedRetried((saved, left)) => {
                let asked = self.unsaved_retry.take() == Some(true);
                self.refresh_unsaved();
                if saved > 0 {
                    return self.toasts.show(Kind::Success, format!("Saved {saved} record{} to Accordo", if saved == 1 { "" } else { "s" }));
                }
                if left > 0 {
                    tracing::info!("{left} on-chain record(s) still wait to be saved to Accordo");
                    if asked {
                        return self.toasts.show(Kind::Error, "Accordo still can't save them — they stay safe on this computer. Try again later.");
                    }
                }
            }
            DataMsg::UnsavedRetry => {
                let Some(session) = self.session.as_ref().filter(|s| !s.token.is_empty()) else { return Task::none() };
                if self.unsaved_retry.is_some() {
                    return Task::none();
                }
                self.unsaved_retry = Some(true);
                return Task::perform(crate::store::unsaved::retry_all(session.api(), session.user.username.clone()), |r| Message::Data(DataMsg::UnsavedRetried(r)));
            }
            DataMsg::UnsavedBackup => {
                let Some((me, server)) = self.session.as_ref().map(|s| (s.user.username.clone(), s.base.clone())) else { return Task::none() };
                return match crate::store::unsaved::save_copy(&me, &server) {
                    Ok(path) => self.toasts.show(Kind::Success, format!("Saved to {} — it holds cheque secrets, keep it private", path.display())),
                    Err(e) => self.toasts.show(Kind::Error, format!("Couldn't save the copy: {e}")),
                };
            }
            DataMsg::StoreSaved(Ok(())) => {}
            DataMsg::StoreSaved(Err(e)) => return self.toasts.show(Kind::Error, format!("Couldn't save the app settings: {e}")),
        }
        Task::none()
    }
}
