// SPDX-License-Identifier: GPL-3.0-or-later
//! Wallets and their secrets: the wallet drawer's messages (add, rename, remove, show key,
//! switch), the master password that unseals them (`vault_update`), the chat-password form that
//! seals a key backup (`key_backup_update`), and locking the session (`lock_wallets`).
//!
//! Responsible for: `WalletMsg`, `SettingsMsg`'s vault cases, `KeyBackupMsg`, and the wallet
//! session in `App` (`wallet_session`, `wallet_open`). Not here: signing transactions
//! (`app/tx.rs`, `app/flows.rs`), the drawer's drawing (`components/wallet_panel.rs`), and the
//! crypto itself (`e2ee/vault.rs`, `wallets/`).

use crate::app::{DataMsg};
use super::*;

impl App {
    /// The wallet drawer (WalletModal): add, rename, remove, show key.
    pub(super) fn wallet_update(&mut self, m: crate::components::wallet_panel::WalletMsg) -> Task<Message> {
        use crate::components::wallet_panel::{Reveal, Secret, WalletMsg as W, HIDE_AFTER_S, RENAME_ID, REVEAL_ID};
        use crate::store::app_store::StoreEffect;
        let ui = &mut self.wallet_ui;
        let sync = |app: &mut App, effect: StoreEffect| -> Task<Message> {
            let save = app.save_store();
            match (effect, app.session.as_ref().filter(|s| !s.token.is_empty())) {
                (StoreEffect::SyncWallets(list), Some(s)) => Task::batch([save, Task::perform(auth::update_wallets(s.api(), list), |v| Message::Data(DataMsg::WalletsSynced(v)))]),
                _ => save,
            }
        };
        match m {
            W::Search(v) => ui.search = v,
            W::StartAdd => {
                ui.adding = true;
                ui.mode = crate::components::wallet_panel::AddMode::Generate;
                ui.menu = None;
                let n = self.store.wallets.len();
                ui.generate(n);
            }
            W::Back => ui.clear_form(),
            W::Mode(mode) => {
                ui.mode = mode;
                let n = self.store.wallets.len();
                ui.clear_form();
                ui.adding = true;
                if mode == crate::components::wallet_panel::AddMode::Generate {
                    ui.generate(n);
                }
            }
            W::Regenerate => {
                let n = self.store.wallets.len();
                ui.generate(n);
            }
            W::Solana => return self.apply(Effect::NotYet("Solana wallets")),
            W::Name(v) => ui.name = v,
            W::Key(v) => {
                let key = v.trim().to_string();
                ui.address = crate::wallets::evm::address_of(&key).unwrap_or_default();
                ui.key = zeroize::Zeroizing::new(key);
            }
            W::Address(v) => ui.address = v,
            W::ToggleKey => ui.key_visible = !ui.key_visible,
            W::CopyNewAddress => {
                let a = ui.address.clone();
                return Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Success, "Address copied to clipboard!")]);
            }
            W::CopyNewKey => {
                let k = ui.key.to_string();
                return Task::batch([iced::clipboard::write(k), self.toasts.show(Kind::Success, "Private key copied to clipboard!")]);
            }
            W::Save => {
                if !self.wallet_session.unlocked() {
                    return self.toasts.show(Kind::Error, "Vault is locked");
                }
                let (name, address) = (ui.name.trim().to_string(), ui.address.trim().to_string());
                if name.is_empty() || address.is_empty() || ui.key.is_empty() {
                    return self.toasts.show(Kind::Error, "All fields are required");
                }
                // The address must be the key's own: a typo here would send payments nowhere.
                let derived = crate::wallets::evm::address_of(&ui.key);
                if derived.as_deref().map(str::to_lowercase) != Some(address.to_lowercase()) {
                    return self.toasts.show(Kind::Error, "That address doesn't belong to this private key");
                }
                if self.store.wallets.iter().any(|w| w.address.eq_ignore_ascii_case(&address)) {
                    return self.toasts.show(Kind::Error, "This wallet is already in your list");
                }
                let key = zeroize::Zeroizing::new(crate::wallets::evm::normalize_key(&ui.key));
                let vault = match self.wallet_session.seal(&key, self.store.encrypted_vault.as_ref()) {
                    Ok(v) => v,
                    Err(e) => return self.toasts.show(Kind::Error, e.to_string()),
                };
                let chain = crate::wallets::chain_of(&address).to_string();
                let effect = self.store.add_wallet(name, derived.unwrap_or(address), vault, chain);
                self.wallet_ui.clear_form();
                return Task::batch([sync(self, effect), self.toasts.show(Kind::Success, "Wallet added")]);
            }
            W::Menu(id) => ui.menu = if ui.menu == id { None } else { id },
            W::MakeActive(id) => {
                ui.menu = None;
                if ui.renaming.is_some() {
                    return Task::none();
                }
                if let Some(w) = self.store.wallets.iter().find(|w| w.id == id) {
                    let (address, chain) = (w.address.clone(), w.chain.clone());
                    self.store.set_active_wallet(Some(address), chain);
                    return self.save_store();
                }
            }
            W::TogglePublic(id) => {
                ui.menu = None;
                let Some(w) = self.store.wallets.iter().find(|w| w.id == id) else { return Task::none() };
                let Some(s) = self.session.as_mut().filter(|s| !s.token.is_empty()) else { return Task::none() };
                let Some(before) = s.user.public_wallets.clone() else { return Task::none() };
                let shown = !auth::is_public(&before, w.chain.as_deref(), &w.address);
                let mut next: Vec<String> = before.iter().filter(|p| !auth::is_public(std::slice::from_ref(p), w.chain.as_deref(), &w.address)).cloned().collect();
                if shown {
                    next.push(w.address.clone());
                }
                // Right away on screen; the answer replaces it (or puts `before` back).
                s.user.public_wallets = Some(next.clone());
                return Task::perform(auth::set_public_wallets(s.api(), next), move |r| Message::Data(DataMsg::PublicWalletsSaved(shown, before.clone(), r)));
            }
            W::CopyAddress(id) => {
                ui.menu = None;
                if let Some(w) = self.store.wallets.iter().find(|w| w.id == id) {
                    let a = w.address.clone();
                    return Task::batch([iced::clipboard::write(a), self.toasts.show(Kind::Success, "Address copied")]);
                }
            }
            W::Rename(id) => {
                ui.menu = None;
                ui.rename_value = self.store.wallets.iter().find(|w| w.id == id).map(|w| w.name.clone()).unwrap_or_default();
                ui.renaming = Some(id);
                return iced::widget::operation::focus(RENAME_ID);
            }
            W::RenameValue(v) => ui.rename_value = v.chars().take(40).collect(),
            W::RenameCommit => {
                let Some(id) = ui.renaming.take() else { return Task::none() };
                let name = ui.rename_value.trim().to_string();
                if !name.is_empty() && self.store.wallets.iter().any(|w| w.id == id && w.name != name) {
                    let effect = self.store.rename_wallet(&id, name);
                    return Task::batch([sync(self, effect), self.toasts.show(Kind::Success, "Wallet renamed")]);
                }
            }
            W::Reveal(id) => {
                ui.menu = None;
                ui.delete = None;
                ui.reveal = Some(Reveal { wallet_id: id, password: Default::default(), key: None, shown: false, busy: false, error: None, left: HIDE_AFTER_S });
                return iced::widget::operation::focus(REVEAL_ID);
            }
            W::RevealPassword(v) => {
                if let Some(r) = ui.reveal.as_mut() {
                    r.password = zeroize::Zeroizing::new(v);
                }
            }
            W::RevealSubmit => {
                let Some(r) = ui.reveal.as_mut().filter(|r| !r.busy && !r.password.is_empty()) else { return Task::none() };
                let Some(vault) = self.store.wallets.iter().find(|w| w.id == r.wallet_id).and_then(|w| w.vault.parsed()) else {
                    r.error = Some("This wallet has no stored key (it was added by address only).".into());
                    return Task::none();
                };
                r.busy = true;
                r.error = None;
                let password = zeroize::Zeroizing::new(crate::e2ee::phrase::normalize_secret(&r.password));
                r.password = Default::default();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::e2ee::vault::decrypt_vault(&vault, &password).map(|k| Secret(zeroize::Zeroizing::new(k))).map_err(|_| "Wrong master password".to_string()))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::Wallet(W::RevealDone(r)),
                );
            }
            W::RevealDone(result) => {
                if let Some(r) = ui.reveal.as_mut() {
                    r.busy = false;
                    match result {
                        Ok(Secret(key)) => {
                            r.key = Some(key);
                            r.left = HIDE_AFTER_S;
                        }
                        Err(e) => r.error = Some(e),
                    }
                }
            }
            W::RevealShow => {
                if let Some(r) = ui.reveal.as_mut() {
                    r.shown = true;
                }
            }
            W::RevealCopy => {
                if let Some(r) = ui.reveal.as_mut() {
                    r.shown = true;
                    if let Some(k) = &r.key {
                        let k = k.to_string();
                        return Task::batch([iced::clipboard::write(k), self.toasts.show(Kind::Success, "Private key copied")]);
                    }
                }
            }
            W::RevealClose => ui.reveal = None,
            W::RevealTick => {
                if let Some(r) = ui.reveal.as_mut().filter(|r| r.key.is_some()) {
                    r.left = r.left.saturating_sub(1);
                    if r.left == 0 {
                        ui.reveal = None;
                    }
                }
            }
            W::Delete(id) => {
                ui.menu = None;
                ui.delete = Some(id);
            }
            W::DeleteCancel => ui.delete = None,
            W::DeleteConfirm => {
                let Some(id) = ui.delete.take() else { return Task::none() };
                let effect = self.store.remove_wallet(&id);
                return Task::batch([sync(self, effect), self.toasts.show(Kind::Success, "Wallet removed")]);
            }
        }
        Task::none()
    }

    /// Debug builds only: unlocks the wallets with `dev_wallet_password` from state.json (put there
    /// by hand, for testing), if they're locked and there's a vault to open. Never sets a master
    /// password. None: nothing to do (release build, no test password, unlocked, already trying).
    pub(super) fn dev_unlock(&mut self) -> Option<Task<Message>> {
        if !cfg!(debug_assertions) || self.wallet_session.unlocked() || self.settings.vault.busy {
            return None;
        }
        let password = self.store.dev_wallet_password.as_ref().map(|p| p.0.clone()).filter(|p| !p.trim().is_empty())?;
        let has_vault = self.store.encrypted_vault.is_some() || self.store.wallets.iter().any(|w| w.vault.parsed().is_some());
        if !has_vault {
            return None;
        }
        self.settings.vault.busy = true;
        self.dev_unlocking = true;
        let secret = crate::e2ee::phrase::normalize_secret(&password);
        let check = self.store.encrypted_vault.clone();
        let wallets = self.store.wallets.clone();
        Some(Task::perform(crate::e2ee::wallet_session::unlock_async(secret, check, wallets), |r| Message::Settings(SettingsMsg::VaultDone(r))))
    }

    /// The master-password card's work: unlock / set, lock, forgot → reset.
    pub(super) fn vault_update(&mut self, msg: SettingsMsg, session: &Session) -> Task<Message> {
        let form = &mut self.settings.vault;
        match msg {
            SettingsMsg::VaultSave => {
                let has_password = self.store.encrypted_vault.is_some() || self.store.wallets.iter().any(|w| w.vault.parsed().is_some());
                let secret = if has_password { Some(crate::e2ee::phrase::normalize_secret(&form.unlock)).filter(|s| !s.trim().is_empty()) } else { form.secret() };
                let Some(secret) = secret else {
                    return self.toasts.show(Kind::Error, if has_password { "Enter your master password" } else { "Choose a master password or phrase" });
                };
                if form.busy {
                    return Task::none();
                }
                form.busy = true;
                let check = self.store.encrypted_vault.clone();
                let wallets = self.store.wallets.clone();
                return Task::perform(crate::e2ee::wallet_session::unlock_async(secret, check, wallets), |r| Message::Settings(SettingsMsg::VaultDone(r)));
            }
            SettingsMsg::VaultDone(result) => {
                form.busy = false;
                let dev = std::mem::take(&mut self.dev_unlocking);
                let done = match result {
                    Ok(done) => done,
                    Err(e) if dev => return self.toasts.show(Kind::Error, format!("The test password in state.json didn't unlock the wallets: {e}")),
                    Err(e) => return self.toasts.show(Kind::Error, e),
                };
                let first = self.store.encrypted_vault.is_none() && !self.store.wallets.iter().any(|w| w.vault.parsed().is_some());
                let minutes = settings_page::LOCK_OPTIONS[form.lock].1;
                form.clear();
                self.store.encrypted_vault = Some(done.check.clone());
                self.store.wallet_lock_minutes = Some(minutes);
                self.wallet_session.start(&done, minutes, chrono::Utc::now().timestamp_millis());
                self.store.set_wallet_session(true, self.wallet_session.lock_at);
                let note = if first { "Master password set" } else if dev { "Wallets unlocked with the test password from state.json" } else { "Wallets unlocked" };
                // With a timer the keys stay in the secret store until it runs out; "until restart"
                // removes what an earlier timed unlock kept.
                let account = session.key();
                let kept = match self.wallet_session.persisted() {
                    Some(json) => Task::perform(crate::e2ee::wallet_session::keep(account, json), Message::WalletKept),
                    None => Task::perform(crate::e2ee::wallet_session::forget(vec![account]), |()| Message::None),
                };
                let mut tasks = vec![self.save_store(), self.toasts.show(Kind::Success, note), kept];
                if let Some(list) = done.upgraded.clone() {
                    self.store.wallets = list;
                    if !session.token.is_empty() {
                        tasks.push(Task::perform(auth::update_wallets(session.api(), self.store.wallets_json()), |v| Message::Data(DataMsg::WalletsSynced(v))));
                    }
                }
                return Task::batch(tasks);
            }
            SettingsMsg::VaultLockNow => {
                let forget = self.lock_and_forget_wallets();
                return Task::batch([forget, self.toasts.show(Kind::Info, "Wallets locked")]);
            }
            SettingsMsg::VaultCopy => {
                let value = form.value.clone();
                return Task::batch([iced::clipboard::write(value), self.toasts.show(Kind::Success, "Copied — store it somewhere safe")]);
            }
            SettingsMsg::VaultReset => {
                let sealed = self.store.wallets.iter().any(|w| w.vault.parsed().is_some());
                // Typing RESET guards against losing keys; with no sealed wallets nothing can be lost.
                if form.resetting || (sealed && form.reset_confirm.trim() != "RESET") {
                    return Task::none();
                }
                form.resetting = true;
                // Server first: if it fails, nothing changes here.
                return if sealed && !session.token.is_empty() {
                    Task::perform(auth::update_wallets(session.api(), Vec::new()), |r| Message::Settings(SettingsMsg::VaultResetDone(r.map(|_| ()).map_err(|e| e.message))))
                } else {
                    Task::done(Message::Settings(SettingsMsg::VaultResetDone(Ok(()))))
                };
            }
            SettingsMsg::VaultResetDone(result) => {
                form.resetting = false;
                if let Err(e) = result {
                    return self.toasts.show(Kind::Error, e);
                }
                let had = !self.store.wallets.is_empty();
                form.clear();
                self.store.wallets.clear();
                self.store.set_active_wallet(None, None);
                self.store.encrypted_vault = None;
                let forget = self.lock_and_forget_wallets();
                return Task::batch([
                    forget,
                    self.save_store(),
                    self.toasts.show(Kind::Success, if had { "Wallets removed — choose a new master password" } else { "Master password reset — choose a new one" }),
                ]);
            }
            _ => {}
        }
        Task::none()
    }

    /// The chat-password form: set / change (seal a backup) or restore (open it).
    pub(super) fn key_backup_update(&mut self, msg: KeyBackupMsg) -> Task<Message> {
        let Some(state) = self.key_backup.as_mut() else { return Task::none() };
        match msg {
            KeyBackupMsg::Close => self.key_backup = None,
            KeyBackupMsg::Done(Ok(done)) => {
                self.key_backup = None;
                let refresh = self.session.as_ref().map_or(Task::none(), |s| Task::perform(crate::api::auth::me(s.api()), |v| Message::Data(DataMsg::UserRefreshed(v))));
                return Task::batch([self.toasts.show(Kind::Success, done), refresh]);
            }
            KeyBackupMsg::Submit => {
                let (Some(secret), Some(session)) = (state.secret(), self.session.as_ref()) else { return Task::none() };
                if state.busy {
                    return Task::none();
                }
                let api = session.api();
                match state.mode {
                    KeyMode::Restore => {
                        state.busy = true;
                        state.error = None;
                        return Task::perform(chat_key::restore_backup(api, session.key(), secret), |v| Message::Session(SessionMsg::ChatKeyInstalled(v)));
                    }
                    KeyMode::Create | KeyMode::Change => {
                        let Some(keypair) = self.chat_key.keypair().cloned() else {
                            state.error = Some("This computer doesn't have your chat key yet — restore it first (from the backup or a key file).".into());
                            return Task::none();
                        };
                        state.busy = true;
                        state.error = None;
                        let done = if state.mode == KeyMode::Create { "Chat password set — your chat key is backed up" } else { "Chat password changed" };
                        return Task::perform(chat_key::create_backup(api, keypair, secret), move |r| Message::KeyBackup(KeyBackupMsg::Done(r.map(|()| done.to_string()))));
                    }
                }
            }
            other => state.update(other),
        }
        Task::none()
    }

    /// Locks the wallets (keys wiped from memory). What the secret store kept for the account
    /// stays: leaving or switching accounts isn't locking, and it resumes on coming back.
    pub(super) fn lock_wallets(&mut self) {
        self.wallet_session.lock();
        self.store.set_wallet_session(false, None);
    }

    /// The user's own lock (Lock now, the deadline, hiding the window, a reset): out of memory and
    /// out of the secret store, so the next start asks for the master password.
    pub(super) fn lock_and_forget_wallets(&mut self) -> Task<Message> {
        self.lock_wallets();
        match &self.session {
            Some(s) => Task::perform(crate::e2ee::wallet_session::forget(vec![s.key()]), |()| Message::None),
            None => Task::none(),
        }
    }
}
