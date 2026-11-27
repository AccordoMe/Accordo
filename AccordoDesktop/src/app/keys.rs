// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard input: what every key press does — the privacy screen, the quick switcher (Ctrl+K),
//! Tab / Esc / F12, the zoom keys and the app's hotkeys (`hotkeys.rs` decides what a combination
//! means; this module turns it into a Message or an `App` action), and recording a shortcut's new
//! keys in App settings → Shortcuts.
//!
//! Responsible for: key presses, and nothing else — no window state of its own, no network, no
//! drawing. Menu/item activation by mouse is the widget that was clicked.
//!
//! Why an `impl App` block sits in a child module: inherent impls may live in any module of the
//! same crate, and a child module can use `App`'s private fields (see `docs/app-modules.md`).

use crate::app::{WindowMsg};
use super::*;

impl App {
    /// A key press: the privacy screen, the quick switcher, Tab / Esc / F12, then shortcuts.
    pub(super) fn key_pressed(&mut self, key: Key, physical: iced::keyboard::key::Physical, m: keyboard::Modifiers, captured: bool) -> Task<Message> {
        use crate::components::quick_switcher::{self as qs, SwitcherMsg};
        use crate::hotkeys::Action;
        if self.hidden {
            // Any key shows it again (modifiers alone don't).
            if !matches!(key, Key::Named(Named::Shift | Named::Control | Named::Alt | Named::Super)) {
                return Task::done(Message::Window(WindowMsg::Unhide));
            }
            return Task::none();
        }
        if let Some(action) = self.hotkey_recording {
            return self.record_hotkey(action, &key, physical, m);
        }
        // An organization's window (not under the file picker choosing its logo).
        if self.orgs.window.is_some() && self.picker.is_none() && matches!(key, Key::Named(Named::Escape)) {
            return Task::done(Message::Org(OrgMsg::Close));
        }
        if self.admin.open && self.tx.is_none() && self.time_picker.is_none() && matches!(key, Key::Named(Named::Escape)) {
            return Task::done(Message::Admin(crate::pages::admin_page::AdminMsg::Close));
        }
        if self.time_picker.is_some() && matches!(key, Key::Named(Named::Escape)) {
            return Task::done(Message::TimePicker(crate::components::time_picker::TimeMsg::Close));
        }
        if self.attach.is_some() && matches!(key, Key::Named(Named::Escape)) {
            return Task::done(Message::Attach(crate::components::chat::attach_picker::AttachMsg::Close));
        }
        // A module's window (consent, permission, link): Esc is its Cancel / Deny all.
        if matches!(self.module_console.front(), Some(crate::module_host::Wait::Consent(_) | crate::module_host::Wait::Allow { .. } | crate::module_host::Wait::Link(_))) {
            if matches!(key, Key::Named(Named::Escape)) {
                return Task::done(Message::Module(ModMsg::Window(crate::components::module_window::ModuleWinMsg::Cancel)));
            }
            return Task::none();
        }
        if let Some(tx) = &self.tx {
            if matches!(key, Key::Named(Named::Escape)) && !tx.running() {
                return Task::done(Message::Tx(crate::components::tx_review::TxMsg::Close));
            }
            return Task::none();
        }
        if self.token_picker.is_some() {
            match key {
                Key::Named(Named::ArrowUp) => return Task::done(Message::TokenPicker(TokenPickerMsg::Move(-1))),
                Key::Named(Named::ArrowDown) => return Task::done(Message::TokenPicker(TokenPickerMsg::Move(1))),
                Key::Named(Named::Escape) => return Task::done(Message::TokenPicker(TokenPickerMsg::Close)),
                _ => {}
            }
        }
        if self.contact_picker.is_some() {
            use crate::components::contact_picker::ContactsMsg;
            match key {
                Key::Named(Named::ArrowUp) => return Task::done(Message::Contacts(ContactsMsg::Move(-1))),
                Key::Named(Named::ArrowDown) => return Task::done(Message::Contacts(ContactsMsg::Move(1))),
                Key::Named(Named::Escape) => return Task::done(Message::Contacts(ContactsMsg::Close)),
                _ => {}
            }
        }
        if self.switcher.is_some() {
            match key {
                Key::Named(Named::ArrowUp) => return Task::done(Message::Switcher(SwitcherMsg::Move(-1))),
                Key::Named(Named::ArrowDown) => return Task::done(Message::Switcher(SwitcherMsg::Move(1))),
                Key::Named(Named::Escape) => return Task::done(Message::Switcher(SwitcherMsg::Close)),
                _ => {}
            }
        }
        // The chat search's suggestions: the arrows go through them.
        if let Some(s) = self.chat.search.as_ref().filter(|_| self.nav == Nav::Chat) {
            use crate::pages::chat_search::{suggestions, SearchMsg};
            let by = match key {
                Key::Named(Named::ArrowUp) => -1,
                Key::Named(Named::ArrowDown) => 1,
                _ => 0,
            };
            if by != 0 && !suggestions(&s.query, self.chat.search_index.get(&s.room_key), &self.chat.me_name).is_empty() {
                return Task::done(Message::Chat(chat_page::ChatMsg::Search(SearchMsg::Move(by))));
            }
        }
        if crate::hotkeys::devtools(&key, physical, m) {
            return Task::done(Message::Dev(DevMsg::Toggle));
        }
        match &key {
            Key::Named(Named::Tab) if !captured => return Task::done(Message::Window(WindowMsg::FocusNext(m.shift()))),
            Key::Named(Named::Escape) if !captured => {
                if self.chain_settings.is_some() {
                    self.chain_settings = None;
                    return Task::none();
                }
                if self.app_settings_open {
                    self.close_app_settings();
                    return Task::none();
                }
                return Task::done(Message::Chat(chat_page::ChatMsg::Escape));
            }
            _ => {}
        }
        // Signed out, or only the confirm-your-email screen up: no shortcuts into the app.
        if !self.store.shortcuts || self.session.is_none() || self.awaiting_email() {
            return Task::none();
        }
        let Some(action) = crate::hotkeys::action(&self.store.hotkeys, &key, physical, m) else { return Task::none() };
        // The next chat in the list's order, up or down (only unread ones: `unread`), round the end.
        let step = |app: &mut App, dir: i32, unread: bool| -> Task<Message> {
            use crate::components::chat::display::RoomKind;
            let order: Vec<String> = [RoomKind::Suggested, RoomKind::Public, RoomKind::Group, RoomKind::Dm].into_iter().flat_map(|k| app.chat.visible_rooms(k).into_iter().map(|r| r.room_key.clone()).collect::<Vec<_>>()).collect();
            let n = order.len() as i32;
            let at = app.chat.selected.as_ref().and_then(|s| order.iter().position(|k| k == s)).map(|i| i as i32);
            // Not in the list: Down starts at the top, Up at the bottom.
            let from = at.unwrap_or(if dir > 0 { -1 } else { n });
            let next = (1..=n).map(|i| (from + dir * i).rem_euclid(n.max(1)) as usize).find(|&i| !unread || (Some(i as i32) != at && app.chat.unread(&order[i]).unread_count > 0));
            let Some(i) = next else {
                return if unread { app.toasts.show(Kind::Info, "No unread chats") } else { Task::none() };
            };
            app.nav = Nav::Chat;
            Task::done(Message::Chat(chat_page::ChatMsg::OpenRoom(order[i].clone())))
        };
        // Things of the open chat: only on the chat page, with a chat open.
        let in_chat = self.nav == Nav::Chat && self.chat.selected.is_some();
        let chat = |m: chat_page::ChatMsg| Task::done(Message::Chat(m));
        let chain_step = |app: &mut App, dir: i32| -> Task<Message> {
            let n = app.chains.len() as i32;
            if n < 2 {
                return Task::none();
            }
            let at = app.chains.iter().position(|c| c.id == app.chain_id).map_or(0, |i| i as i32);
            let next = app.chains[(at + dir).rem_euclid(n) as usize].clone();
            let toast = app.toasts.show(Kind::Info, format!("Network: {}", next.name));
            Task::batch([Task::done(Message::SelectChain(next)), toast])
        };
        let zoom = |app: &mut App, to: f32| -> Task<Message> {
            app.store.ui_scale = to;
            app.save_store()
        };
        match action {
            Action::QuickSwitcher => {
                self.switcher = Some(qs::State::default());
                self.close_app_settings();
                return iced::widget::operation::focus(qs::INPUT);
            }
            Action::PrevChat => return step(self, -1, false),
            Action::NextChat => return step(self, 1, false),
            Action::PrevUnread => return step(self, -1, true),
            Action::NextUnread => return step(self, 1, true),
            Action::Page(nav) if self.modules().page_shown(nav) => return Task::done(Message::Navigate(nav)),
            Action::Page(_) => {}
            Action::NewChat | Action::NewGroup => {
                use crate::components::chat::new_chat_modal::Kind as NewKind;
                self.nav = Nav::Chat;
                return chat(chat_page::ChatMsg::OpenNewChat(if action == Action::NewGroup { NewKind::Group } else { NewKind::Dm }));
            }
            Action::JoinLink => {
                self.nav = Nav::Chat;
                return chat(chat_page::ChatMsg::JoinInvite(crate::components::chat::join_invite::JoinMsg::Open));
            }
            Action::BrowsePublic => {
                self.nav = Nav::Chat;
                return chat(chat_page::ChatMsg::OpenBrowse);
            }
            Action::ToggleMembers if in_chat => return chat(chat_page::ChatMsg::ToggleMembers),
            Action::Pinned if in_chat => return chat(chat_page::ChatMsg::OpenPinned),
            Action::Search if in_chat => return chat(chat_page::ChatMsg::Search(crate::pages::chat_search::SearchMsg::Toggle)),
            Action::RoomSettings if in_chat => return chat(chat_page::ChatMsg::OpenManage(crate::components::chat::manage_room::Tab::General)),
            Action::Emoji if in_chat => return chat(chat_page::ChatMsg::OpenEmoji),
            Action::Gifs if in_chat => return chat(chat_page::ChatMsg::OpenGifs),
            Action::Attach if in_chat => return chat(chat_page::ChatMsg::OpenPlus),
            Action::VoiceMessage if in_chat => {
                return chat(match &self.chat.recording {
                    Some(r) if r.kind == crate::recorder::Kind::Voice => chat_page::ChatMsg::RecordStop,
                    Some(_) => return Task::none(), // a video note is on
                    None => chat_page::ChatMsg::RecordStart(crate::recorder::Kind::Voice),
                });
            }
            Action::ToggleMembers | Action::Pinned | Action::Search | Action::RoomSettings | Action::Emoji | Action::Gifs | Action::Attach | Action::VoiceMessage => {}
            Action::OpenWallet => return Task::done(if self.wallet_open { Message::CloseWallet } else { Message::OpenWallet }),
            Action::Contacts => {
                use crate::components::contact_picker::{ContactsMsg, Target};
                return Task::done(Message::Contacts(if self.contact_picker.is_some() { ContactsMsg::Close } else { ContactsMsg::Open(Target::Manage) }));
            }
            Action::ChainSettings => {
                use crate::components::chain_settings::ChainSettingsMsg;
                return Task::done(Message::ChainSettings(if self.chain_settings.is_some() { ChainSettingsMsg::Close } else { ChainSettingsMsg::Open }));
            }
            Action::NextChain => return chain_step(self, 1),
            Action::PrevChain => return chain_step(self, -1),
            Action::AccountMenu => return Task::done(Message::Session(SessionMsg::ToggleMenu)),
            Action::AdminPanel => {
                if self.session.as_ref().is_some_and(|s| s.user.is_admin) {
                    return Task::done(Message::Admin(crate::pages::admin_page::AdminMsg::Open));
                }
            }
            Action::ToggleRail => self.rail_hidden = !self.rail_hidden,
            Action::AppSettings => {
                if self.app_settings_open {
                    self.close_app_settings();
                } else {
                    return Task::done(Message::AppSettings(AppSettingsMsg::Open));
                }
            }
            Action::Shortcuts => {
                if self.app_settings_open && self.app_settings_tab == app_settings_modal::Tab::Shortcuts {
                    self.close_app_settings();
                } else {
                    self.app_settings_open = true;
                    return Task::done(Message::AppSettings(AppSettingsMsg::Tab(app_settings_modal::Tab::Shortcuts)));
                }
            }
            Action::ZoomIn => {
                let cur = self.store.ui_scale;
                if let Some(s) = app_settings_modal::SCALES.iter().find(|s| **s > cur + 0.01) {
                    return zoom(self, *s);
                }
            }
            Action::ZoomOut => {
                let cur = self.store.ui_scale;
                if let Some(s) = app_settings_modal::SCALES.iter().rev().find(|s| **s < cur - 0.01) {
                    return zoom(self, *s);
                }
            }
            Action::ZoomReset => return zoom(self, 1.0),
            Action::HideScreen => return self.hide(),
            Action::LockWallets => {
                if self.wallet_session.unlocked() {
                    let forget = self.lock_and_forget_wallets();
                    return Task::batch([forget, self.toasts.show(Kind::Info, "Wallets locked")]);
                }
            }
        }
        Task::none()
    }

    /// App settings → Shortcuts is waiting for `action`'s new keys: this press is them (Esc: never
    /// mind). Keys that can't be a shortcut keep it waiting, with the reason under the row.
    fn record_hotkey(&mut self, action: crate::hotkeys::Action, key: &Key, physical: iced::keyboard::key::Physical, m: keyboard::Modifiers) -> Task<Message> {
        use crate::hotkeys::{self, Combo};
        if matches!(key, Key::Named(Named::Escape)) {
            self.hotkey_recording = None;
            self.hotkey_note = None;
            return Task::none();
        }
        if matches!(key, Key::Named(Named::Shift | Named::Control | Named::Alt | Named::Super | Named::AltGraph)) {
            return Task::none(); // the key itself is still to come
        }
        let Some(combo) = Combo::from_event(key, physical, m) else {
            self.hotkey_note = Some((action, "This key can't be a shortcut — try a letter, a digit or F1…F11".into()));
            return Task::none();
        };
        if let Some(why) = hotkeys::refuse(&combo) {
            self.hotkey_note = Some((action, why));
            return Task::none();
        }
        self.hotkey_recording = None;
        self.hotkey_note = hotkeys::assign(&mut self.store.hotkeys, action, &combo).map(|took| (action, format!("{combo} was “{took}” — that one has no shortcut now")));
        self.save_store()
    }

    /// Closes App settings (and stops waiting for a shortcut's keys).
    pub(super) fn close_app_settings(&mut self) {
        self.app_settings_open = false;
        self.hotkey_recording = None;
        self.hotkey_note = None;
    }
}
