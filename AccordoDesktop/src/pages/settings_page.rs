// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings & Preferences (SettingsPage.tsx): a grid of cards, as many columns as fit, and the
//! links footer (SocialLinks.tsx).

use iced::widget::{button, column, container, responsive, row, scrollable, space, text};
use iced::{Alignment, Color, Fill};

use super::Effect;
use crate::api::account::DeviceSession;
use crate::api::webhooks::WebhookStatus;
use crate::api::auth::User;
use crate::api::client::ApiError;
use crate::app::Message;
use crate::components::key_backup_modal::Tab;
use crate::e2ee::phrase::{generate_password, generate_phrase, normalize_secret, phrase_problem};
use zeroize::Zeroize;
use crate::components::settings::{
    account_profile, api_access, chat_encryption, connected_apps, master_vault, notifications, sessions, sign_in_security, webhooks,
};
use crate::components::equal_row::EqualRow;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::{self, *};

/// How long wallets stay unlocked (walletSession.ts LOCK_TIMER_OPTIONS), in minutes (0: until restart).
pub const LOCK_OPTIONS: [(&str, u32); 5] = [("Until restart", 0), ("15 min", 15), ("1 hour", 60), ("8 hours", 480), ("24 hours", 1440)];

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    ToggleHowItWorks,
    Sounds(bool),
    SystemNotifications(bool),
    Preview(bool),
    OpenKeyBackup(crate::components::key_backup_modal::Mode),
    // master password (choosing one: SecretField.tsx; the rest is handled by the app)
    VaultTab(Tab),
    VaultValue(String),
    VaultConfirm(String),
    VaultShow,
    VaultGenerate,
    VaultSavedIt(bool),
    VaultCopy,
    VaultUnlockInput(String),
    VaultLock(usize),
    VaultSave,
    VaultDone(Result<std::sync::Arc<crate::e2ee::wallet_session::Unlocked>, String>),
    VaultLockNow,
    VaultForgot(bool),
    VaultResetConfirm(String),
    VaultReset,
    VaultResetDone(Result<(), String>),
    // webhooks (status from the API, handled by the app)
    WebhookUrl(String),
    WebhookDocs,
    RefreshWebhooks,
    WebhooksLoaded(Result<WebhookStatus, ApiError>),
    // sessions (API calls, handled by the app)
    SessionsLoaded(Result<Vec<DeviceSession>, ApiError>),
    EndSession(String),
    SessionEnded(Result<String, ApiError>),
    EndOthers,
    OthersEnded(Result<String, ApiError>),
    /// Footer links: a path of the web app, or a full URL.
    Open(&'static str),
    NotYet(&'static str),
}

/// The master-password card. Choosing a new one works like SecretField.tsx: a password typed
/// twice, or a 12-word phrase typed in; or either generated — then it must be confirmed as saved.
#[derive(Default)]
pub struct VaultForm {
    pub tab: Tab,
    pub value: String,
    pub confirm: String,
    pub generated: bool,
    pub saved: bool,
    pub show: bool,
    /// A password already exists: the one typed to unlock.
    pub unlock: String,
    /// Index into LOCK_OPTIONS.
    pub lock: usize,
    pub busy: bool,
    pub reset_open: bool,
    pub reset_confirm: String,
    pub resetting: bool,
}

pub const VAULT_MIN_LEN: usize = 10;

impl VaultForm {
    /// What's wrong with the new secret so far (None: nothing, or nothing typed yet).
    pub fn problem(&self) -> Option<String> {
        if self.value.is_empty() {
            return None;
        }
        match self.tab {
            Tab::SeedPhrase => phrase_problem(&self.value),
            Tab::Password if self.value.chars().count() < VAULT_MIN_LEN => Some(format!("At least {VAULT_MIN_LEN} characters")),
            Tab::Password if !self.generated && !self.confirm.is_empty() && self.confirm != self.value => Some("The passwords don't match".into()),
            Tab::Password => None,
        }
    }

    /// The new secret, normalised, once it's acceptable.
    pub fn secret(&self) -> Option<String> {
        let complete = !self.value.is_empty()
            && self.problem().is_none()
            && (self.tab == Tab::SeedPhrase || self.generated || self.confirm == self.value)
            && (!self.generated || self.saved);
        complete.then(|| normalize_secret(&self.value))
    }

    fn switch(&mut self, tab: Tab) {
        self.tab = tab;
        self.value.zeroize();
        self.confirm.zeroize();
        self.generated = false;
        self.saved = false;
    }

    /// Clears what was typed (after a save or unlock).
    pub fn clear(&mut self) {
        let tab = self.tab;
        self.switch(tab);
        self.unlock.zeroize();
        self.reset_open = false;
        self.reset_confirm.clear();
    }
}

/// Per-device preferences and what the cards loaded from the API.
pub struct State {
    pub how_it_works: bool,
    pub sounds: bool,
    pub system_notifications: bool,
    pub preview: bool,
    pub vault: VaultForm,
    /// None until loaded.
    pub sessions: Option<Vec<DeviceSession>>,
    /// A session being signed out ("others" for all of them).
    pub busy: Option<String>,
    pub webhooks: Option<WebhookStatus>,
    pub webhook_url: String,
    pub webhook_docs: bool,
    /// The account picture editor, when open: a link or a preset (web: avatarInput).
    pub avatar_edit: Option<String>,
    pub avatar_saving: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            how_it_works: false,
            sounds: true,
            system_notifications: true,
            preview: true,
            vault: VaultForm { lock: 2, ..Default::default() },
            sessions: None,
            busy: None,
            webhooks: None,
            webhook_url: String::new(),
            webhook_docs: false,
            avatar_edit: None,
            avatar_saving: false,
        }
    }
}

/// Local changes; the API messages (sessions, webhooks loading) are handled by the app,
/// which has the session token.
pub fn update(state: &mut State, msg: SettingsMsg) -> Effect {
    match msg {
        SettingsMsg::ToggleHowItWorks => state.how_it_works = !state.how_it_works,
        SettingsMsg::Sounds(v) => state.sounds = v,
        SettingsMsg::SystemNotifications(v) => state.system_notifications = v,
        SettingsMsg::Preview(v) => state.preview = v,
        SettingsMsg::VaultTab(t) => state.vault.switch(t),
        SettingsMsg::VaultValue(v) => {
            let f = &mut state.vault;
            f.value = v;
            if f.tab == Tab::Password {
                f.generated = false;
                f.saved = false;
            }
        }
        SettingsMsg::VaultConfirm(v) => state.vault.confirm = v,
        SettingsMsg::VaultShow => state.vault.show = !state.vault.show,
        // Generated: shown as words / masked dots with Copy — it must be saved somewhere, then ticked.
        SettingsMsg::VaultGenerate => {
            let f = &mut state.vault;
            f.value = if f.tab == Tab::SeedPhrase { generate_phrase() } else { generate_password(20) };
            f.confirm.zeroize();
            f.generated = true;
            f.saved = false;
        }
        SettingsMsg::VaultSavedIt(v) => state.vault.saved = v,
        SettingsMsg::VaultUnlockInput(v) => state.vault.unlock = v,
        SettingsMsg::VaultLock(i) => state.vault.lock = i,
        SettingsMsg::VaultForgot(open) => {
            state.vault.reset_open = open;
            state.vault.reset_confirm.clear();
        }
        SettingsMsg::VaultResetConfirm(v) => state.vault.reset_confirm = v,
        SettingsMsg::WebhookUrl(v) => state.webhook_url = v,
        SettingsMsg::WebhookDocs => state.webhook_docs = !state.webhook_docs,
        SettingsMsg::Open(target) => return Effect::OpenWeb(target.into()),
        SettingsMsg::NotYet(what) => return Effect::NotYet(what),
        _ => {}
    }
    Effect::None
}

pub const SCROLL_ID: &str = "settings-scroll";
const MIN_CARD: f32 = 360.0;
const GAP: f32 = 20.0;

pub fn view<'a>(user: &'a User, limits: Vec<crate::api::auth::AccountLimit>, state: &'a State, key: &'a crate::e2ee::account::ChatKey, vault_status: master_vault::Status) -> El<'a> {
    responsive(move |size| {
        let header = column![
            text("Settings & Preferences").size(24).font(EXTRABOLD).color(Color::WHITE),
            text("Your profile, chat encryption, wallets and API access.").size(14).color(TEXT_SECONDARY),
            space().height(2),
            container(space().width(Fill).height(1)).style(theme::rule),
        ]
        .spacing(6);

        // CSS: repeat(auto-fit, minmax(360px, 1fr)) — as many columns as fit, 1 to 3.
        let available = size.width - 48.0;
        let columns = (((available + GAP) / (MIN_CARD + GAP)).floor() as usize).clamp(1, 3);
        let mut cards: Vec<El<'a>> = vec![
            account_profile::view(user, state.avatar_edit.as_deref(), state.avatar_saving, limits.clone()),
            chat_encryption::view(user, state.how_it_works, key),
            api_access::view(user),
            sign_in_security::view(user),
            notifications::view(state),
            connected_apps::view(),
            master_vault::view(&state.vault, vault_status),
            webhooks::view(state.webhooks.as_ref(), &state.webhook_url, state.webhook_docs),
            sessions::view(user, state.sessions.as_deref(), state.busy.as_deref()),
        ];
        // Rows of equal-height cards, like the web's CSS grid (align-items: stretch).
        let mut grid = column![].spacing(GAP);
        while !cards.is_empty() {
            let take = columns.min(cards.len());
            let mut line: Vec<El<'a>> = cards.drain(..take).collect();
            line.extend((take..columns).map(|_| space().into()));
            grid = grid.push(EqualRow::new(line, GAP));
        }

        scrollable(container(column![header, grid, footer()].spacing(24)).padding([20, 24]).width(Fill))
            .id(SCROLL_ID)
            .height(Fill)
            .style(thin_scroll)
            .into()
    })
    .into()
}

/// Links under the cards (SocialLinks.tsx): X, email, legal pages, version.
fn footer<'a>() -> El<'a> {
    let link = |caption: &'static str, target: &'static str, size: f32| -> El<'a> {
        button(text(caption).size(size).color(TEXT_SECONDARY)).padding(0).style(link_button).on_press(Message::Settings(SettingsMsg::Open(target))).into()
    };
    container(
        column![
            row![
                link("𝕏", "https://x.com/AccordoMe", 14.0),
                button(row![icon(icons::mail(), 13, TEXT_SECONDARY), text("hello@accordo.me").size(12).color(TEXT_SECONDARY)].spacing(5).align_y(Alignment::Center))
                    .padding(0)
                    .style(link_button)
                    .on_press(Message::Settings(SettingsMsg::Open("mailto:hello@accordo.me"))),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
            row![link("Terms", "/terms", 12.0), link("Privacy", "/privacy", 12.0), link("Security", "/security", 12.0)].spacing(14),
            row![
                text(format!("Accordo Desktop · v{} ·", env!("CARGO_PKG_VERSION"))).size(11).color(FAINT),
                link("All links", "/socials", 11.0),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .width(Fill)
    .align_x(iced::alignment::Horizontal::Center)
    .padding([12, 0])
    .into()
}
