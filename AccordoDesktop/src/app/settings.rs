// SPDX-License-Identifier: GPL-3.0-or-later
//! App settings (SettingsPage.tsx): the handler for the settings window's messages, reading them
//! at start-up, and writing them back to `<config>/state.json` (store/app_store).
//!
//! Responsible for: `SettingsMsg` and the preferences in `store` (appearance, privacy, hotkeys,
//! developer mode, lock-on-hide…). Not here: the settings *views*
//! (`components/app_settings_modal.rs`, `pages/settings_page.rs`), chat-encryption settings
//! (`key_backup_update` in `app/mod.rs`), or wallets (`app/wallet.rs`, when it exists).

use crate::app::{DataMsg};
use super::*;

impl App {

    /// Settings cards backed by the API: devices signed in, the webhooks add-on.
    pub(super) fn settings_update(&mut self, msg: SettingsMsg) -> Task<Message> {
        let Some(session) = self.session.clone() else { return Task::none() };
        let settings = &mut self.settings;
        match msg {
            SettingsMsg::SessionsLoaded(Ok(list)) => settings.sessions = Some(list),
            SettingsMsg::SessionsLoaded(Err(e)) => {
                settings.sessions = Some(Vec::new());
                return self.toasts.show(Kind::Error, format!("Couldn't load your sessions: {}", e.message));
            }
            SettingsMsg::EndSession(id) => {
                settings.busy = Some(id.clone());
                return Task::perform(account::end_session(session.api(), id), |r| Message::Settings(SettingsMsg::SessionEnded(r)));
            }
            SettingsMsg::SessionEnded(result) => {
                settings.busy = None;
                return match result {
                    Ok(id) => {
                        let device = settings.sessions.as_ref().and_then(|l| l.iter().find(|s| s.id == id)).map(|s| s.device.clone()).unwrap_or_default();
                        if let Some(list) = settings.sessions.as_mut() {
                            list.retain(|s| s.id != id);
                        }
                        self.toasts.show(Kind::Success, format!("Signed out {device}"))
                    }
                    Err(e) => self.toasts.show(Kind::Error, e.message),
                };
            }
            SettingsMsg::EndOthers => {
                settings.busy = Some("others".into());
                return Task::perform(account::end_other_sessions(session.api()), |r| Message::Settings(SettingsMsg::OthersEnded(r)));
            }
            SettingsMsg::OthersEnded(result) => {
                settings.busy = None;
                return match result {
                    // This session got a fresh token (the old one is now refused): keep and remember it.
                    Ok(token) => {
                        let mut tasks = vec![self.toasts.show(Kind::Success, "Every other session is signed out")];
                        if let Some(current) = self.session.as_mut() {
                            current.token = token;
                            tasks.push(Task::perform(session_store::save(current.key(), current.saved()), |v| Message::Session(SessionMsg::Saved(v))));
                        }
                        tasks.push(self.load_settings());
                        Task::batch(tasks)
                    }
                    Err(e) => self.toasts.show(Kind::Error, e.message),
                };
            }
            SettingsMsg::WebhooksLoaded(Ok(status)) => {
                settings.webhook_url = status.default_url.clone().unwrap_or_default();
                settings.webhooks = Some(status);
            }
            SettingsMsg::WebhooksLoaded(Err(e)) => {
                settings.webhooks = Some(Default::default());
                return self.toasts.show(Kind::Error, format!("Couldn't load webhooks: {}", e.message));
            }
            SettingsMsg::RefreshWebhooks => {
                return Task::perform(webhooks::status(session.api()), |r| Message::Settings(SettingsMsg::WebhooksLoaded(r)));
            }
            m @ (SettingsMsg::VaultSave
            | SettingsMsg::VaultDone(_)
            | SettingsMsg::VaultLockNow
            | SettingsMsg::VaultCopy
            | SettingsMsg::VaultReset
            | SettingsMsg::VaultResetDone(_)) => return self.vault_update(m, &session),
            other => {
                let effect = settings_page::update(settings, other);
                return self.apply(effect);
            }
        }
        Task::none()
    }

    pub(super) fn load_settings(&mut self) -> Task<Message> {
        // (No token: the screenshot demo user, which has nothing to load.)
        let Some(s) = self.session.as_ref().filter(|s| !s.token.is_empty()) else { return Task::none() };
        Task::batch([
            Task::perform(account::sessions(s.api()), |r| Message::Settings(SettingsMsg::SessionsLoaded(r))),
            Task::perform(webhooks::status(s.api()), |r| Message::Settings(SettingsMsg::WebhooksLoaded(r))),
            // The limits' counts come with /me: fresh ones for the profile card.
            Task::perform(crate::api::auth::me(s.api()), |v| Message::Data(DataMsg::UserRefreshed(v))),
        ])
    }

    pub(super) fn save_store(&mut self) -> Task<Message> {
        let store = self.store.clone();
        Task::perform(
            async move { tokio::task::spawn_blocking(move || store.save().map_err(|e| e.to_string())).await.unwrap_or_else(|e| Err(e.to_string())) },
            |v| Message::Data(DataMsg::StoreSaved(v)),
        )
    }

    /// Preferences that live in other parts of the app.
    pub(super) fn apply_prefs(&mut self) {
        self.chat.blur_messages = self.store.blur_messages;
        self.chat.blur_previews = self.store.blur_previews;
    }
}
