// SPDX-License-Identifier: GPL-3.0-or-later
//! The application: state, messages, routing between sign-in and the app (App.tsx).
//!
//! What lives here by design: `App` (its fields), the `Message` enum, the `update` dispatcher,
//! `view`, `subscription` — everything that is cross-cutting. Per-area handlers live in the
//! submodules, each one file for one subject:
//!
//! | file                | responsible for                                            |
//! |---------------------|------------------------------------------------------------|
//! | `keys.rs`           | keyboard input: hotkeys, Ctrl+K, Tab/Esc/F12, zoom, hiding  |
//! | `avatar.rs`         | the account's avatar (URL, upload, save)                    |
//! | `lists.rs`          | cheques / invoices lists, time window, chat's attach window |
//! | `settings.rs`       | app settings: the window's messages, load and save          |
//! | `wallet.rs`         | wallets, the vault (master password), key backup, locking   |
//! | `picker.rs`         | token picker, chain settings (RPC, explorers, faucets)      |
//! | `tx.rs`             | transaction review, effects, choosing the signing wallet    |
//! | `flows.rs`          | cheque / invoice / drop / crowdfund flows (the `start_*`)   |
//! | `session.rs`        | signing in and out, switching accounts, the privacy screen  |
//! | `windows.rs`        | what the window does: title bar actions, close/blur, drops  |
//! | `loads.rs`          | what the loaded data does: chains, tokens, wallets, store   |
//! | `msg/`              | the message types of the areas (`SessionMsg`, `WindowMsg`, `DataMsg`) |
//! | `module_console.rs` | the module API console in Developer tools                  |
//! | `orgs.rs`           | organizations on the rail                                  |
//!
//! Adding a handler: put it in the file of its subject; if the subject has no file yet, create
//! one (`mod <name>;` here) the way `docs/app-modules.md` describes.
//!
//! An area's messages live in the area's file too (`SessionMsg` in `session.rs`), and `Message`
//! keeps one variant that dispatches to its handler — the pattern for the rest is in
//! `docs/app-modules.md`, "Deep phase".

pub mod module_console;
pub use module_console::ModMsg;
pub mod orgs;
pub use orgs::OrgMsg;
mod avatar;
mod flows;
mod keys;
mod lists;
mod loads;
mod picker;
mod session;
mod settings;
mod tx;
mod wallet;
mod windows;

pub mod msg;
pub use msg::{DataMsg, SessionMsg, WindowMsg};

use iced::keyboard::{self, key::Named, Key};
use iced::widget::operation;
use iced::{window, Element, Subscription, Task};

use crate::api::socket::{self, SocketEvent, SocketKey};
use crate::api::{account, webhooks};
use crate::api::auth::{self, Session};
use crate::api::client::ApiError;
use std::collections::{HashMap, HashSet};

use crate::api::data::{self, Chain, Token, DEFAULT_CHAIN_ID};
use crate::api::session_store::{self, Saved};
use crate::components::file_picker::{self, PickerMsg, Purpose};
use crate::components::key_backup_modal::{self, KeyBackupMsg, Mode as KeyMode};
use crate::e2ee::account::{self as chat_key, ChatKey};
use crate::e2ee::crypto::DeviceKeypair;
use crate::components::app_settings_modal::{self, AppSettingsMsg};
use crate::components::chain_settings::{self, ChainSettingsMsg};
use crate::components::token_picker::{self, TokenPickerMsg};
use crate::components::devtools_panel::{self, DevMsg};
use crate::store::app_store::AppStore;
use crate::components::layout;
use crate::components::toast::{Kind, Toasts};
use crate::pages::chat_page::{self, ChatMsg};
use crate::pages::login::{self, LoginMsg, Outcome};
use crate::pages::cheques_page::{self, ChequesMsg};
#[cfg(feature = "crowdfund")]
use crate::pages::crowdfund_page::{self, CrowdfundMsg};
use crate::pages::invoices_page::{self, InvoicesMsg};
use crate::pages::settings_page::{self, SettingsMsg};
use crate::pages::Effect;
use crate::pages::splash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Chat,
    Cheques,
    Invoices,
    Drops,
    Crowdfund,
    Settings,
}

#[derive(Debug, Clone, Copy)]
pub enum WindowAction {
    Drag,
    ToggleMaximize,
    Minimize,
    Close,
}

/// The account's picture editor (Settings → Account Profile) and picked pictures.
#[derive(Debug, Clone)]
pub enum AvatarMsg {
    /// Open / close the editor.
    Edit,
    Url(String),
    Upload,
    Save,
    Reset,
    /// A picked file made into the data URL the server keeps (or why not).
    Ready(Purpose, Result<String, String>),
    Saved(Result<Box<crate::api::auth::User>, String>, &'static str),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The session and the accounts saved on this computer.
    Session(SessionMsg),
    Window(WindowMsg),
    Data(DataMsg),
    ToggleRail,
    Navigate(Nav),
    Login(LoginMsg),
    Chat(ChatMsg),
    /// The live connection (chat messages, room changes…).
    Socket(SocketEvent),
    /// Changing the account's or a room's picture.
    Avatar(AvatarMsg),
    /// Organizations: the rail's list, the new-organization and organization windows.
    Org(OrgMsg),
    Settings(SettingsMsg),
    KeyBackup(KeyBackupMsg),
    /// The in-app file picker.
    Picker(PickerMsg),
    Wallet(crate::components::wallet_panel::WalletMsg),
    /// A key press (`captured`: a field already used it). Shortcuts are matched in `update`.
    /// A key press in one of the windows (the main one, or the developer tools).
    Key { key: Key, physical: iced::keyboard::key::Physical, modifiers: keyboard::Modifiers, captured: bool, window: window::Id },
    Switcher(crate::components::quick_switcher::SwitcherMsg),
    /// Once a second while wallets are unlocked with a timer: lock at the deadline.
    WalletClock,
    /// The unlock this computer kept for the account (lock timer on), read at sign-in.
    WalletRestored(String, Option<std::sync::Arc<crate::e2ee::wallet_session::Restored>>),
    /// Keeping the unlock in the secret store: why it failed (the wallets then lock with the app).
    WalletKept(Result<(), String>),
    /// Once a second while App settings → About is open: measure what the app uses.
    UsageTick,
    /// The Networks window (the gear next to the network picker).
    ChainSettings(ChainSettingsMsg),
    TokenPicker(TokenPickerMsg),
    /// The contacts window (@ in an address field, or Ctrl+Shift+T).
    Contacts(crate::components::contact_picker::ContactsMsg),
    OpenWallet,
    CloseWallet,
    Cheques(ChequesMsg),
    Lists(crate::components::finance::records::ListsMsg),
    /// The date & time window (chat tokens, custom expiry / due / end dates).
    TimePicker(crate::components::time_picker::TimeMsg),
    Attach(crate::components::chat::attach_picker::AttachMsg),
    /// The review / signing window (components/tx_review.rs).
    Tx(crate::components::tx_review::TxMsg),
    /// Claim a cheque: its record, and the slot / recipient clicked (chat card), if any.
    ClaimCheque(Box<serde_json::Value>, Option<String>),
    /// Claim your share of a drop (from its chat card).
    #[cfg(feature = "drops")]
    ClaimDrop(Box<crate::api::drops::DropInfo>, Box<crate::api::drops::DropSlot>),
    /// Pay an invoice: its record.
    PayInvoice(Box<serde_json::Value>),
    /// A review read from the contract is ready (or why it can't be done).
    TxReady(Result<crate::flows::Handoff, String>),
    /// The module API console (Developer tools → API) and the windows its calls open.
    Module(ModMsg),
    Invoices(InvoicesMsg),
    #[cfg(feature = "crowdfund")]
    Crowdfund(CrowdfundMsg),
    #[cfg(feature = "drops")]
    Drops(crate::pages::drops_page::DropsMsg),
    /// The Admin Console window.
    Admin(crate::pages::admin_page::AdminMsg),
    /// Copy this ("Campaign ID" copied).
    CopyText(String, &'static str),
    SelectChain(Chain),
    /// A control whose feature isn't in the desktop client yet: says so instead of doing nothing.
    NotYet(&'static str),
    None,
    DismissToast(u64),
    /// The developer tools panel (F12).
    Dev(DevMsg),
    /// The app's own settings (gear by the window buttons).
    AppSettings(AppSettingsMsg),
    #[cfg(feature = "dev-screenshot")]
    Shoot,
    #[cfg(feature = "dev-screenshot")]
    Shot(window::Screenshot),
}

pub struct App {
    pub session: Option<Session>,
    /// A saved session is being checked: show the splash instead of the sign-in form.
    pub restoring: bool,
    /// The next session to enter goes into the credential store (a fresh sign-in, or one read
    /// from an older entry — named here — which moves under the account's key).
    persist_next: Option<Option<String>>,
    pub login: login::State,
    pub chat: chat_page::State,
    pub nav: Nav,
    pub rail_hidden: bool,
    pub profile_open: bool,
    pub wallet_open: bool,
    /// "Set your chat password" modal, when open.
    pub key_backup: Option<key_backup_modal::State>,
    /// This computer's chat key for the signed-in account.
    pub chat_key: ChatKey,
    pub picker: Option<file_picker::State>,
    pub settings: settings_page::State,
    /// Networks from /api/data/chains, and the one picked in the top bar.
    pub chains: Vec<Chain>,
    pub chain_id: String,
    /// Token lists by chain id, loaded when a network is first picked (searchable picker state).
    pub tokens: HashMap<String, Vec<Token>>,
    /// The token picker window, when open.
    pub token_picker: Option<token_picker::State>,
    /// The contacts window, when open, and the field it fills in.
    pub contact_picker: Option<crate::components::contact_picker::State>,
    /// Cheques and invoices from the server, shared by the pages and the chat's attach window.
    pub lists: crate::components::finance::records::Lists,
    pub time_picker: Option<crate::components::time_picker::State>,
    /// The chat's "attach a cheque / invoice" window.
    pub attach: Option<crate::components::chat::attach_picker::State>,
    /// Review → sign → confirm → save, for whichever form asked.
    pub tx: Option<crate::components::tx_review::State<crate::flows::Job>>,
    pub tx_kind: Option<crate::flows::Kind>,
    /// The cheque / invoice id the window is about (to refresh what shows it).
    pub tx_subject: Option<String>,
    /// A review is being read from the contract (one at a time).
    pub tx_preparing: bool,
    /// Kinds of the signed-in account's records that are live on-chain but not in Accordo yet
    /// (store/unsaved.rs) — the banner over the pages.
    pub unsaved: Vec<String>,
    /// Retrying them; `.1`: asked for (a failure is shown, not just logged).
    pub unsaved_retry: Option<bool>,
    /// The module API console: the sandbox module and its requests (app/module_console.rs).
    pub module_console: module_console::Console,
    /// Organizations on the rail and their window.
    pub orgs: orgs::Orgs,
    /// Token pictures by URL (fetched for the rows on screen).
    pub token_logos: HashMap<String, Option<iced::widget::image::Handle>>,
    tokens_loading: HashSet<String>,
    pub cheques: cheques_page::State,
    pub invoices: invoices_page::State,
    #[cfg(feature = "crowdfund")]
    pub crowdfund: crowdfund_page::State,
    #[cfg(feature = "drops")]
    pub drops: crate::pages::drops_page::State,
    pub admin: crate::pages::admin_page::State,
    /// The live connection is up and authenticated.
    pub socket_live: bool,
    pub devtools: devtools_panel::State,
    pub app_settings_open: bool,
    /// The app's own window (the developer tools have another; see main.rs).
    pub main_window: Option<window::Id>,
    /// Sections the server closed for everyone (none until it says so).
    pub server_closed: Vec<crate::modules::Module>,
    pub app_settings_tab: app_settings_modal::Tab,
    /// App settings → Shortcuts: the action waiting for its new keys, and a word under a row
    /// (why those keys can't be used, or which action lost them).
    pub hotkey_recording: Option<crate::hotkeys::Action>,
    pub hotkey_note: Option<(crate::hotkeys::Action, String)>,
    /// The wallets are being unlocked with the test password from state.json (dev_unlock).
    pub dev_unlocking: bool,
    /// Memory / CPU / disk of this app, for About.
    pub usage: crate::usage::Usage,
    /// The Networks window, when open.
    pub chain_settings: Option<chain_settings::State>,
    /// Ctrl+K, when open.
    pub switcher: Option<crate::components::quick_switcher::State>,
    /// The privacy screen covers the window; whether that locked the wallets.
    pub hidden: bool,
    pub hidden_locked: bool,
    /// Client-side state kept on this computer (store/app_store.rs): UI scale, Developer mode…
    pub store: AppStore,
    /// Wallet keys unlocked with the master password (in memory; with a lock timer also kept in
    /// the secret store until it runs out).
    pub wallet_session: crate::e2ee::wallet_session::WalletSession,
    /// When this run of the app began (ms): a chat key kept "until the app closes" from an
    /// earlier run is older than this.
    pub started_ms: i64,
    /// Accounts whose chat-key reminder was shown in this run ("Not now" lasts until the next start).
    pub chat_key_reminded: std::collections::HashSet<String>,
    pub wallet_ui: crate::components::wallet_panel::State,
    pub toasts: Toasts,
}

impl App {
    pub fn new(saved: Option<Saved>, store: AppStore) -> (Self, Task<Message>) {
        crate::devtools::set_enabled(store.dev_mode);
        let mut app = Self {
            session: None,
            restoring: saved.is_some(),
            persist_next: saved.as_ref().and_then(|s| s.moved_from.clone()).map(Some),
            login: login::State::default(),
            chat: chat_page::State::new(0, String::new()),
            nav: Nav::Chat,
            rail_hidden: false,
            profile_open: false,
            wallet_open: false,
            key_backup: None,
            chat_key: ChatKey::Loading,
            picker: None,
            settings: settings_page::State::default(),
            chains: Vec::new(),
            chain_id: DEFAULT_CHAIN_ID.to_string(),
            tokens: HashMap::new(),
            token_picker: None,
            contact_picker: None,
            tx: None,
            lists: Default::default(),
            time_picker: None,
            attach: None,
            tx_kind: None,
            tx_subject: None,
            tx_preparing: false,
            unsaved: Vec::new(),
            unsaved_retry: None,
            module_console: Default::default(),
            orgs: Default::default(),
            token_logos: HashMap::new(),
            tokens_loading: HashSet::new(),
            cheques: cheques_page::State::default(),
            invoices: invoices_page::State::default(),
            #[cfg(feature = "crowdfund")]
            crowdfund: crowdfund_page::State::default(),
            #[cfg(feature = "drops")]
            drops: Default::default(),
            admin: Default::default(),
            socket_live: false,
            devtools: devtools_panel::State { tab: store.devtools_tab, filter: store.devtools_filter.clone(), ..Default::default() },
            app_settings_open: false,
            main_window: None,
            server_closed: Vec::new(),
            app_settings_tab: Default::default(),
            hotkey_recording: None,
            hotkey_note: None,
            dev_unlocking: false,
            usage: Default::default(),
            chain_settings: None,
            switcher: None,
            hidden: false,
            hidden_locked: false,
            store,
            wallet_session: Default::default(),
            started_ms: chrono::Utc::now().timestamp_millis(),
            chat_key_reminded: Default::default(),
            wallet_ui: Default::default(),
            toasts: Toasts::default(),
        };
        let start = match saved {
            Some(saved) => {
                app.login.api_url = saved.base.clone();
                Task::perform(auth::restore(saved.base, saved.token), |r| Message::Session(SessionMsg::Restored(r.map(Box::new))))
            }
            None => operation::focus(login::FIELD_ID),
        };
        // Open when the app last closed: open again (as they were — docked or in their window).
        let start = if app.store.devtools_open && crate::devtools::enabled() { Task::batch([start, Task::done(Message::Dev(DevMsg::Toggle))]) } else { start };
        #[cfg(feature = "dev-screenshot")]
        let start = start.chain(crate::dev::boot());
        (app, start)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Window(m) => return self.window_update(m),
            Message::Data(m) => return self.data_update(m),
            Message::Session(m) => return self.session_update(m),
            Message::None => {}
            Message::ToggleRail => self.rail_hidden = !self.rail_hidden,
            Message::Navigate(nav) => {
                if !self.modules().page_available(nav) {
                    let what = crate::modules::Module::of_nav(nav).map(|m| m.label()).unwrap_or("This section");
                    return self.toasts.show(Kind::Info, format!("{what} isn't available on this server"));
                }
                self.nav = nav;
                self.profile_open = false;
                self.wallet_open = false;
                if nav == Nav::Settings {
                    return self.load_settings();
                }
                if nav == Nav::Invoices && self.invoices.merchant.trim().is_empty() {
                    if let Some(w) = self.active_wallet().filter(|w| crate::wallets::chain_of(&w.address) == "EVM") {
                        self.invoices.merchant = w.address.clone();
                    }
                }
                let lists = self.ensure_lists();
                let api = self.session.as_ref().filter(|s| !s.token.is_empty()).map(Session::api);
                let page = match (nav, api) {
                    #[cfg(feature = "drops")]
                    (Nav::Drops, Some(api)) if self.drops.drops.is_none() && !self.drops.loading => crate::pages::drops_page::load(&mut self.drops, &api),
                    #[cfg(feature = "crowdfund")]
                    (Nav::Crowdfund, Some(api)) => crowdfund_page::load_tab(&mut self.crowdfund, &api, false),
                    _ => Task::none(),
                };
                return Task::batch([lists, page]);
            }
            Message::Login(msg) => match login::update(&mut self.login, msg) {
                Outcome::None => {}
                Outcome::Task(task) => return task,
                Outcome::SignedIn(session) => {
                    self.persist_next = Some(self.persist_next.take().flatten());
                    return self.enter(session);
                }
                Outcome::Error(message) => return self.toasts.show(Kind::Error, message),
                Outcome::Notice(message) | Outcome::BackToSignIn(message) => return self.toasts.show(Kind::Success, message),
                Outcome::OpenWeb(url) => {
                    if open::that_detached(&url).is_err() {
                        return self.toasts.show(Kind::Info, format!("Open {url} in your browser"));
                    }
                }
            },
            Message::Chat(msg) => {
                let Some(api) = self.session.as_ref().map(Session::api) else { return Task::none() };
                let (task, error) = chat_page::update(&mut self.chat, msg, &api);
                let mut tasks = vec![task];
                if let Some(e) = error {
                    tasks.push(self.toasts.show(Kind::Error, e));
                }
                for note in std::mem::take(&mut self.chat.notices) {
                    tasks.push(self.toasts.show(Kind::Success, note));
                }
                return Task::batch(tasks);
            }
            Message::Avatar(m) => return self.avatar_update(m),
            Message::Org(m) => return self.org_update(m),
            Message::Socket(event) => {
                let Some(api) = self.session.as_ref().map(Session::api) else { return Task::none() };
                return match event {
                    // Events from the gap are lost: re-fetch what's on screen.
                    SocketEvent::Authenticated { reconnected } => {
                        tracing::info!("socket authenticated{}", if reconnected { " again: refreshing what's on screen" } else { "" });
                        self.socket_live = true;
                        if reconnected { chat_page::on_reconnect(&mut self.chat, &api) } else { Task::none() }
                    }
                    SocketEvent::Event(data) => {
                        // A cheque / invoice changed: the lists that show it load again.
                        let event = data.get("event").and_then(|e| e.as_str()).unwrap_or_default();
                        let reload: Vec<Task<Message>> = self.lists.touched_by(event).into_iter().map(|k| self.load_list(k)).collect();
                        // My own profile changed on another device: this one's top bar and card too.
                        let me = self.session.as_ref().map(|s| s.user.id);
                        let mine = if event == "user_profile_updated" && data.get("user").and_then(|u| u.get("id")).and_then(|v| v.as_i64()) == me {
                            Task::perform(auth::me(api.clone()), |v| Message::Data(DataMsg::UserRefreshed(v)))
                        } else {
                            Task::none()
                        };
                        Task::batch([chat_page::on_socket(&mut self.chat, &data, &api), Task::batch(reload), mine])
                    }
                    SocketEvent::Disconnected => {
                        self.socket_live = false;
                        Task::none()
                    }
                };
            }
            Message::Settings(SettingsMsg::OpenKeyBackup(mode)) => self.key_backup = Some(key_backup_modal::State::new(mode)),
            Message::Settings(msg) => return self.settings_update(msg),
            Message::KeyBackup(msg) => return self.key_backup_update(msg),
            Message::Picker(PickerMsg::Open(purpose)) => self.picker = Some(file_picker::State::new(purpose)),
            Message::Picker(PickerMsg::Close) => self.picker = None,
            Message::Picker(PickerMsg::Confirm) => return self.picked_file(),
            Message::Picker(msg) => {
                if let Some(p) = self.picker.as_mut() {
                    p.update(msg);
                }
            }
            Message::UsageTick => self.usage.sample(self.chat.media.stats()),
            Message::ChainSettings(m) => return self.chain_settings_update(m),
            Message::TokenPicker(m) => return self.token_picker_update(m),
            Message::Contacts(m) => return self.contacts_update(m),
            Message::WalletClock => {
                if self.wallet_session.expired(chrono::Utc::now().timestamp_millis()) {
                    let forget = self.lock_and_forget_wallets();
                    return Task::batch([forget, self.toasts.show(Kind::Info, "Wallets locked")]);
                }
            }
            Message::WalletRestored(account, restored) => {
                // Still the same account, not unlocked meanwhile, and the deadline still ahead.
                let current = self.session.as_ref().is_some_and(|s| s.key() == account);
                if let Some(r) = restored.filter(|r| current && !self.wallet_session.unlocked() && r.lock_at > chrono::Utc::now().timestamp_millis()) {
                    self.wallet_session.resume(&r);
                    self.store.set_wallet_session(true, self.wallet_session.lock_at);
                    tracing::info!("wallets: unlocked from this computer's secret store until the lock timer runs out");
                }
            }
            Message::WalletKept(result) => {
                if let Err(e) = result {
                    tracing::warn!("wallets: couldn't keep the unlock in the secret store: {e}");
                    return self.toasts.show(Kind::Info, "Couldn't keep the unlock in the system's secret store — the wallets will lock when the app closes");
                }
            }
            Message::OpenWallet => {
                self.wallet_open = true;
                self.profile_open = false;
            }
            Message::CloseWallet => {
                self.wallet_open = false;
                self.wallet_ui.menu = None;
                self.wallet_ui.clear_form();
            }
            Message::Wallet(m) => return self.wallet_update(m),
            Message::Key { key, physical, modifiers, captured, window } => {
                if self.devtools.window == Some(window) {
                    // The developer tools' own window: F12 / Ctrl+Shift+I / Esc close it; the rest is its fields'.
                    if crate::hotkeys::devtools(&key, physical, modifiers) || matches!(key, Key::Named(Named::Escape)) {
                        return Task::done(Message::Dev(DevMsg::Toggle));
                    }
                    return Task::none();
                }
                return self.key_pressed(key, physical, modifiers, captured);
            }
            Message::Switcher(m) => return self.switcher_update(m),
            Message::SelectChain(chain) => {
                // Tokens picked on another network don't exist here.
                self.cheques.token = None;
                self.invoices.token = None;
                #[cfg(feature = "crowdfund")]
                {
                    self.crowdfund.token = None;
                }
                self.chain_id = chain.id;
                return self.load_tokens();
            }
            Message::Cheques(msg) => {
                let effect = cheques_page::update(&mut self.cheques, msg);
                return Task::batch([self.apply(effect), self.ensure_lists()]);
            }
            Message::Lists(m) => return self.lists_update(m),
            Message::TimePicker(m) => return self.time_update(m),
            Message::Attach(m) => return self.attach_update(m),
            Message::Tx(m) => return self.tx_update(m),
            Message::ClaimCheque(c, recipient) => return self.start_claim(*c, recipient),
            Message::PayInvoice(v) => return self.start_pay(*v),
            #[cfg(feature = "drops")]
            Message::ClaimDrop(d, s) => return self.start_drop_claim(*d, *s),
            Message::TxReady(r) => {
                self.tx_preparing = false;
                return match r {
                    Ok(h) => {
                        let Some(ready) = h.take() else { return Task::none() };
                        self.tx_kind = Some(ready.job.kind());
                        self.tx = Some(crate::components::tx_review::State::new(ready.review, ready.explorer, ready.job));
                        self.module_mark_tx();
                        Task::none()
                    }
                    Err(e) => {
                        let module = self.module_tx_failed(&e);
                        Task::batch([self.toasts.show(Kind::Error, e), module])
                    }
                };
            }
            Message::Module(m) => return self.module_update(m),
            Message::Invoices(msg) => {
                let effect = invoices_page::update(&mut self.invoices, msg);
                return Task::batch([self.apply(effect), self.ensure_lists()]);
            }
            #[cfg(feature = "crowdfund")]
            Message::Crowdfund(msg) => {
                let api = self.session.as_ref().filter(|s| !s.token.is_empty()).map(Session::api);
                // "Back" on a chat card: to the form.
                if matches!(msg, CrowdfundMsg::Pick(_)) {
                    self.nav = Nav::Crowdfund;
                }
                let effect = crowdfund_page::update(&mut self.crowdfund, msg, api);
                return self.apply(effect);
            }
            #[cfg(feature = "drops")]
            Message::Drops(m) => {
                let touches = m.touches_plans();
                let ctx = crate::pages::drops_page::Ctx { api: self.session.as_ref().filter(|s| !s.token.is_empty()).map(Session::api), web_base: self.web_base() };
                let effect = crate::pages::drops_page::update(&mut self.drops, &mut self.store.drop_plans, m, ctx);
                let t = self.apply(effect);
                return if touches { Task::batch([t, self.save_store()]) } else { t };
            }
            Message::CopyText(value, what) => return self.apply(Effect::Copy(value, what)),
            Message::Admin(m) => {
                use crate::pages::admin_page::Note;
                let api = self.session.as_ref().filter(|s| !s.token.is_empty() && s.user.is_admin).map(Session::api);
                let (task, note) = crate::pages::admin_page::update(&mut self.admin, m, api);
                let toast = match note {
                    Note::None => Task::none(),
                    Note::Ok(t) => self.toasts.show(Kind::Success, t),
                    Note::Err(t) => self.toasts.show(Kind::Error, t),
                };
                return Task::batch([task, toast]);
            }
            Message::NotYet(what) => return self.apply(Effect::NotYet(what)),
            Message::DismissToast(id) => self.toasts.dismiss(id),
            Message::AppSettings(msg) => {
                match msg {
                    AppSettingsMsg::Open => {
                        self.app_settings_open = true;
                        if self.app_settings_tab == app_settings_modal::Tab::About {
                            self.usage.sample(self.chat.media.stats());
                        }
                    }
                    AppSettingsMsg::Tab(t) => {
                        self.app_settings_tab = t;
                        self.hotkey_recording = None;
                        self.hotkey_note = None;
                        if t == app_settings_modal::Tab::About {
                            self.usage.sample(self.chat.media.stats()); // numbers at once, not after a second
                        }
                    }
                    AppSettingsMsg::Module(m, on) => {
                        self.store.hidden_modules.retain(|x| *x != m);
                        if !on {
                            self.store.hidden_modules.push(m);
                            if self.nav == m.nav() {
                                self.nav = Nav::Chat;
                            }
                            if self.store.start_page == m.page_id() {
                                self.store.start_page = "chat".into();
                            }
                            if self.attach.as_ref().is_some_and(|a| a.tab.module() == m) {
                                self.attach = None;
                            }
                        }
                        self.chat.modules = self.modules();
                        return self.save_store();
                    }
                    AppSettingsMsg::StartPage(p) => {
                        self.store.start_page = p.to_string();
                        return self.save_store();
                    }
                    AppSettingsMsg::Toggle(pref, on) => {
                        use app_settings_modal::Pref;
                        let s = &mut self.store;
                        match pref {
                            Pref::AutoplayGifs => s.autoplay_gifs = on,
                            Pref::BlurMessages => s.blur_messages = on,
                            Pref::BlurPreviews => s.blur_previews = on,
                            Pref::HideOnBlur => s.hide_on_blur = on,
                            Pref::LockWalletsOnHide => s.lock_wallets_on_hide = on,
                            Pref::Shortcuts => s.shortcuts = on,
                        }
                        self.apply_prefs();
                        return self.save_store();
                    }
                    AppSettingsMsg::HideNow => {
                        self.close_app_settings();
                        return self.hide();
                    }
                    AppSettingsMsg::Close => self.close_app_settings(),
                    AppSettingsMsg::RecordKey(action) => {
                        self.hotkey_note = None;
                        self.hotkey_recording = if self.hotkey_recording == Some(action) { None } else { Some(action) };
                    }
                    AppSettingsMsg::ClearKey(action) => {
                        self.hotkey_recording = None;
                        self.hotkey_note = None;
                        crate::hotkeys::clear(&mut self.store.hotkeys, action);
                        return self.save_store();
                    }
                    AppSettingsMsg::ResetKey(action) => {
                        self.hotkey_recording = None;
                        self.hotkey_note = crate::hotkeys::reset(&mut self.store.hotkeys, action).map(|took| (action, format!("Taken from “{took}” — it has no shortcut now")));
                        return self.save_store();
                    }
                    AppSettingsMsg::ResetKeys => {
                        self.hotkey_recording = None;
                        self.hotkey_note = None;
                        self.store.hotkeys.clear();
                        return Task::batch([self.save_store(), self.toasts.show(Kind::Success, "Shortcuts are back to their defaults")]);
                    }
                    AppSettingsMsg::Scale(scale) => {
                        self.store.ui_scale = scale;
                        return self.save_store();
                    }
                    AppSettingsMsg::DevMode(on) => {
                        self.store.dev_mode = on;
                        crate::devtools::set_enabled(on);
                        if !crate::devtools::enabled() {
                            self.devtools.selected = None;
                            if let Some(id) = self.devtools.window {
                                return Task::batch([window::close(id), self.save_store()]);
                            }
                        }
                        return self.save_store();
                    }
                    AppSettingsMsg::Devtools(open) => {
                        // Already as asked: bring a detached window to the front.
                        if open == self.devtools.open {
                            return self.devtools.window.filter(|_| open).map(window::gain_focus).unwrap_or_else(Task::none);
                        }
                        return Task::done(Message::Dev(DevMsg::Toggle));
                    }
                    AppSettingsMsg::ForgetWalletKeys => {
                        // Every account signed in here, and the open one (its entry may predate the list).
                        let mut accounts: Vec<String> = self.store.accounts.iter().map(|a| a.key()).collect();
                        accounts.extend(self.session.as_ref().map(Session::key));
                        accounts.sort();
                        accounts.dedup();
                        self.lock_wallets();
                        let forget = Task::perform(crate::e2ee::wallet_session::forget(accounts), |()| Message::None);
                        return Task::batch([forget, self.toasts.show(Kind::Success, "Saved wallet keys deleted from this computer — wallets locked")]);
                    }
                    AppSettingsMsg::ForgetChatKey => {
                        // Only with a backup: the chat password brings it back.
                        let Some((account, keep)) = self.chat_key_keep() else { return Task::none() };
                        if !keep.has_backup || !matches!(self.chat_key, ChatKey::Ready(_) | ChatKey::Different(_)) {
                            return Task::none();
                        }
                        tracing::info!("chat key: deleted from this computer (Storage)");
                        let forget = Task::perform(chat_key::forget(account), |r| Message::Session(SessionMsg::ChatKeyForgotten(r)));
                        return Task::batch([forget, self.drop_chat_key("Your chat key was deleted from this computer.")]);
                    }
                    AppSettingsMsg::ChatKeyLifetime(lifetime) => {
                        if !self.chat_key_keep().is_some_and(|(_, k)| k.has_backup) {
                            return Task::none();
                        }
                        self.store.chat_key_lifetime = lifetime;
                        // Days: applied now (a key older than that goes). Until the app closes: at closing.
                        let check = if matches!(lifetime, crate::e2ee::keystore::Lifetime::Days(_)) { self.check_chat_key_lifetime() } else { Task::none() };
                        return Task::batch([self.save_store(), check]);
                    }
                    AppSettingsMsg::OpenDataFolder => {
                        if let Some(dir) = crate::api::client::config_dir() {
                            let _ = std::fs::create_dir_all(&dir);
                            if open::that_detached(&dir).is_err() {
                                return self.toasts.show(Kind::Info, format!("The app data is in {}", dir.display()));
                            }
                        }
                    }
                }
            }
            Message::Dev(msg) => {
                let d = &mut self.devtools;
                match msg {
                    DevMsg::Toggle => {
                        // Closing: WindowClosed tidies up a window (also when it's closed by its own ✕).
                        if let Some(id) = d.window {
                            return window::close(id); // WindowClosed remembers them closed
                        }
                        if d.open {
                            d.open = false;
                            self.store.devtools_open = false;
                            return self.save_store();
                        }
                        if !crate::devtools::enabled() {
                            return Task::none();
                        }
                        d.open = true;
                        d.seen = crate::devtools::version();
                        let remember = if self.store.devtools_open { Task::none() } else {
                            self.store.devtools_open = true;
                            self.save_store()
                        };
                        // Docked under the app unless it was last moved to a window of its own.
                        if self.store.devtools_detached {
                            return Task::batch([remember, Task::done(Message::Dev(DevMsg::Detach))]);
                        }
                        return remember;
                    }
                    DevMsg::Detach => {
                        if d.window.is_some() {
                            return Task::none();
                        }
                        let (id, open) = window::open(window::Settings {
                            size: iced::Size::new(1180.0, 720.0),
                            min_size: Some(iced::Size::new(760.0, 420.0)),
                            ..crate::window_base()
                        });
                        d.window = Some(id);
                        d.open = true;
                        self.store.devtools_detached = true;
                        return Task::batch([open.discard(), self.save_store()]);
                    }
                    DevMsg::Dock => {
                        let Some(id) = d.window else { return Task::none() };
                        d.docking = true;
                        self.store.devtools_detached = false;
                        return Task::batch([window::close(id), self.save_store()]);
                    }
                    DevMsg::Tab(tab) => {
                        d.tab = tab;
                        d.selected = None;
                        self.store.devtools_tab = tab;
                        self.store.devtools_filter = d.filter.clone();
                        return self.save_store();
                    }
                    DevMsg::Select(id) => d.selected = Some(id),
                    DevMsg::Filter(f) => d.filter = f,
                    DevMsg::Clear => {
                        crate::devtools::clear();
                        d.selected = None;
                    }
                    DevMsg::Copy(text) => return iced::clipboard::write(text),
                    DevMsg::Tick => d.seen = crate::devtools::version(),
                }
            }
            #[cfg(feature = "dev-screenshot")]
            Message::Shoot | Message::Shot(_) => return crate::dev::update(self, message),
        }
        Task::none()
    }

    /// What a window shows: the developer tools in theirs, the app in the main one.
    pub fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.devtools.window == Some(id) {
            return devtools_panel::view(&self.devtools, &self.module_console);
        }
        self.view()
    }

    pub fn title(&self, id: window::Id) -> String {
        let app = match crate::api::client::profile() {
            Some(p) => format!("Accordo ({p})"),
            None => "Accordo".into(),
        };
        if self.devtools.window == Some(id) { format!("{app} — Developer tools") } else { app }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let screen = match &self.session {
            None if self.restoring => layout::auth(splash::view()),
            None => layout::auth(login::view(&self.login, self.store.accounts_by_use())),
            Some(session) => layout::app(self, session),
        };
        // Windows over the screen, bottom to top. The screen is always the first layer of one
        // stack, however many are open: wrapping it only when something opens would change the
        // widget tree's shape, and iced would start the screen's widgets over — the chat would
        // jump to its newest message whenever a toast or a window appeared.
        let mut layers: Vec<Element<'_, Message>> = vec![screen];
        if let Some(st) = &self.token_picker {
            let listed: &[Token] = self.tokens.get(&st.chain_id).map(Vec::as_slice).unwrap_or(&[]);
            let custom = self.custom_tokens(&st.chain_id);
            let entries = token_picker::visible(st, &custom, listed);
            let chain = self.chains.iter().find(|c| c.id == st.chain_id);
            let view = token_picker::View {
                st,
                entries,
                listed,
                total: listed.len() + custom.len(),
                loading: !self.tokens.contains_key(&st.chain_id),
                chain_name: chain.map(|c| c.name.as_str()).unwrap_or(""),
                logos: &self.token_logos,
                wallet: self.picker_wallet(),
                explorer: chain.is_some_and(|c| self.store.explorer_override(&c.id).is_some() || c.explorer.is_some()),
            };
            layers.push(token_picker::view(view));
        } else if let Some(st) = &self.chain_settings {
            let view = chain_settings::View {
                st,
                chains: &self.chains,
                active: &self.chain_id,
                custom: &self.store.chain_overrides,
                saved: self.chain_saved(&st.chain_id),
                faucet_to: self.faucet_target(&st.chain_id),
            };
            layers.push(chain_settings::view(view));
        } else if self.app_settings_open {
            let view = app_settings_modal::View { tab: self.app_settings_tab, store: &self.store, server: self.session.as_ref().map(|s| s.base.as_str()), usage: &self.usage, modules: self.modules(), developer: self.session.as_ref().map(|s| s.user.is_developer), devtools_open: self.devtools.open, recording: self.hotkey_recording, note: self.hotkey_note.as_ref(), wallet_kept_until: self.wallet_session.lock_at.filter(|_| self.wallet_session.unlocked()), chat_key: self.session.as_ref().map(|s| app_settings_modal::ChatKeyInfo { here: matches!(self.chat_key, ChatKey::Ready(_) | ChatKey::Different(_)), has_backup: s.user.has_key_backup }) };
            layers.push(app_settings_modal::view(view));
        }
        if self.admin.open && self.session.as_ref().is_some_and(|s| s.user.is_admin) {
            layers.push(crate::components::admin_view::view(&self.admin));
        }
        if let Some(a) = &self.attach {
            layers.push(crate::components::chat::attach_picker::view(a, &self.lists, chrono::Utc::now().timestamp(), self.modules()));
        }
        if let Some(c) = &self.contact_picker {
            layers.push(crate::components::contact_picker::view(c, &self.store.contacts));
        }
        if let Some(t) = &self.time_picker {
            layers.push(crate::components::time_picker::view(t));
        }
        if let Some(tx) = &self.tx {
            layers.push(crate::components::tx_review::view(tx));
        }
        if let (Some(sw), true) = (&self.switcher, self.session.is_some()) {
            layers.push(crate::components::quick_switcher::view(sw, &self.chat));
        }
        if !self.toasts.is_empty() {
            layers.push(self.toasts.view());
        }
        // The developer tools, docked under the app (as a browser's are) unless moved to a window.
        // The app stays the column's first child either way, for the same reason as above.
        let mut docked = iced::widget::column![iced::widget::container(iced::widget::Stack::with_children(layers)).height(iced::Fill)];
        if self.devtools.open && self.devtools.window.is_none() {
            docked = docked.push(crate::components::ui::hline()).push(iced::widget::container(devtools_panel::view(&self.devtools, &self.module_console)).height(340).padding(6).style(crate::theme::panel(crate::theme::RAIL)));
        }
        // The privacy screen covers everything, toasts included (they can quote a message).
        let mut top: Vec<Element<'_, Message>> = vec![docked.into()];
        if self.hidden {
            top.push(crate::components::privacy_screen::view(self.hidden_locked));
        }
        iced::widget::Stack::with_children(top).into()
    }

    fn switcher_update(&mut self, m: crate::components::quick_switcher::SwitcherMsg) -> Task<Message> {
        use crate::components::quick_switcher::{matches, SwitcherMsg};
        let Some(sw) = self.switcher.as_mut() else { return Task::none() };
        match m {
            SwitcherMsg::Query(q) => {
                sw.query = q;
                sw.index = 0;
            }
            SwitcherMsg::Move(d) => {
                let n = matches(&self.chat, &sw.query).len();
                if n > 0 {
                    sw.index = (sw.index.min(n - 1) as i32 + d).rem_euclid(n as i32) as usize;
                }
            }
            SwitcherMsg::Close => self.switcher = None,
            SwitcherMsg::Open(key) => {
                self.switcher = None;
                if let Some(key) = key {
                    self.nav = Nav::Chat;
                    return Task::done(Message::Chat(chat_page::ChatMsg::OpenRoom(key)));
                }
            }
        }
        Task::none()
    }

    /// Loads the picked network's tokens once (kept for later switches back).
    fn load_tokens(&mut self) -> Task<Message> {
        let Some(session) = &self.session else { return Task::none() };
        if self.chain_id.is_empty() || self.tokens.contains_key(&self.chain_id) || !self.tokens_loading.insert(self.chain_id.clone()) {
            return Task::none();
        }
        Task::perform(data::tokens(session.api(), self.chain_id.clone()), |v| Message::Data(DataMsg::TokensLoaded(v)))
    }

    /// A file was chosen in the picker: act on it for what it was asked for.
    fn picked_file(&mut self) -> Task<Message> {
        let (Some(picker), Some(session)) = (self.picker.as_mut(), self.session.as_ref()) else { return Task::none() };
        let Some(path) = picker.selected.clone() else { return Task::none() };
        match picker.purpose {
            Purpose::ChatKeyFile => {
                let contents = match std::fs::metadata(&path) {
                    Ok(m) if m.len() > 256 * 1024 => Err("That file is too big to be a key file.".to_string()),
                    Ok(_) => std::fs::read_to_string(&path).map_err(|e| format!("Can't read the file: {e}")),
                    Err(e) => Err(format!("Can't read the file: {e}")),
                };
                match contents {
                    Ok(text) => Task::perform(
                        chat_key::import_key_file(session.api(), session.key(), session.user.e2ee_public_key.clone(), text),
                        |v| Message::Session(SessionMsg::ChatKeyInstalled(v)),
                    ),
                    Err(e) => {
                        picker.error = Some(e);
                        Task::none()
                    }
                }
            }
            Purpose::RoomAvatar | Purpose::ProfileAvatar | Purpose::OrgLogo => {
                let purpose = picker.purpose;
                self.picker = None;
                Task::perform(
                    async move { tokio::task::spawn_blocking(move || crate::avatars::file_to_data_url(&path, 160)).await.map_err(|e| e.to_string())? },
                    move |r| Message::Avatar(AvatarMsg::Ready(purpose, r)),
                )
            }
            Purpose::Attachment => {
                self.picker = None;
                Task::done(Message::Chat(chat_page::ChatMsg::AttachPath(path)))
            }
            #[cfg(feature = "drops")]
            Purpose::DropList => {
                let read = match std::fs::metadata(&path) {
                    Ok(m) if m.len() > 20 * 1024 * 1024 => Err("That file is too big for a wallet list (20 MB at most).".to_string()),
                    Ok(_) => std::fs::read_to_string(&path).map_err(|e| format!("Can't read the file: {e}")),
                    Err(e) => Err(format!("Can't read the file: {e}")),
                };
                match read {
                    Ok(text) => {
                        self.picker = None;
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "the file".into());
                        Task::done(Message::Drops(crate::pages::drops_page::DropsMsg::FileLoaded(name, text)))
                    }
                    Err(e) => {
                        picker.error = Some(e);
                        Task::none()
                    }
                }
            }
        }
    }

    /// The RPC endpoints to use for a network: the user's own if set, else the server's.
    pub fn rpc_urls(&self, chain: &Chain) -> Vec<String> {
        self.store.rpc_override(&chain.id).unwrap_or_else(|| chain.rpc_urls.clone())
    }

    /// The wallet picked in the wallet drawer (the web's activeWalletId: an id or an address).
    /// The one place that answers "which wallet" — the top bar, the faucet, payments read it.
    /// Which sections are on: the build, the server's closed ones and the user's hidden ones.
    pub fn modules(&self) -> crate::modules::Modules {
        crate::modules::Modules::new(&self.server_closed, &self.store.hidden_modules)
    }

    pub fn active_wallet(&self) -> Option<&crate::store::app_store::SecureWallet> {
        self.store.active_wallet()
    }

    #[cfg(feature = "drops")]
    /// The web app's address (claim links point there).
    pub fn web_base(&self) -> String {
        self.session.as_ref().map(|s| crate::api::client::normalize_base(&s.base)).unwrap_or_default()
    }

    /// Tokens of the network picked in the top bar (None while loading).
    pub fn chain_tokens(&self) -> Option<&Vec<Token>> {
        self.tokens.get(&self.chain_id)
    }

    /// The form's token field: still loading / the picked token's picture.
    pub fn token_field(&self, picked: Option<&Token>) -> crate::components::finance::token_select::Field {
        crate::components::finance::token_select::Field { loading: self.chain_tokens().is_none(), logo: picked.and_then(|t| self.logo_of(t)) }
    }

    pub fn logo_of(&self, t: &Token) -> Option<iced::widget::image::Handle> {
        t.logo.as_ref().and_then(|u| self.token_logos.get(u).cloned().flatten())
    }

    /// Tokens you added for a network (the store's customTokens), as list entries.
    fn custom_tokens(&self, chain_id: &str) -> Vec<Token> {
        self.store
            .custom_tokens
            .get(chain_id)
            .map(|l| l.iter().map(|t| Token { symbol: t.symbol.clone(), name: t.name.clone(), address: t.address.clone(), decimals: t.decimals.unwrap_or(18), logo: t.logo_uri.clone().filter(|u| !u.is_empty()) }).collect())
            .unwrap_or_default()
    }

    #[cfg(feature = "crowdfund")]
    pub fn native_symbol(&self) -> &str {
        self.selected_chain().map(|c| c.native_symbol.as_str()).unwrap_or("ETH")
    }

    pub fn selected_chain(&self) -> Option<&Chain> {
        self.chains.iter().find(|c| c.id == self.chain_id)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Every key press, even ones a field used (Ctrl+K while typing must still work); what
        // they do is decided in `key_pressed`.
        let keys = iced::event::listen_with(|event, status, window| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, physical_key, modifiers, .. }) => {
                Some(Message::Key { key, physical: physical_key, modifiers, captured: status == iced::event::Status::Captured, window })
            }
            _ => None,
        });
        // Closing the main window quits; the developer tools' window just goes.
        let closed = Some(window::close_events().map(|v| Message::Window(WindowMsg::Closed(v))));
        // A view asked for an avatar after the last update: load it soon.
        let avatars = crate::avatars::has_wanted().then(|| iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Data(DataMsg::AvatarPump)));
        // Another window in front: the privacy screen, if that's on.
        let blur = (self.store.hide_on_blur && self.session.is_some()).then(|| {
            iced::event::listen_with(|event, _, window| match event {
                iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::Window(WindowMsg::Blurred(window))),
                _ => None,
            })
        });
        // The open panel redraws when something new was recorded.
        let dev = self.devtools.open.then(|| iced::time::every(std::time::Duration::from_millis(400)).map(|_| Message::Dev(DevMsg::Tick)));
        // Signed in: the live connection (restarts when the account or server changes).
        let live = self.session.as_ref().filter(|s| !s.token.is_empty()).map(|s| {
            let key = SocketKey { base: s.base.clone(), token: s.token.clone(), user_id: s.user.id, username: s.user.username.clone() };
            Subscription::run_with(key, socket::connect).map(Message::Socket)
        });
        // GIFs and stickers move; files dropped on the window are attached.
        let anim = if self.nav != Nav::Chat {
            None
        } else if self.chat.recording.is_some() {
            // A recording runs: its time moves (the levels and the picture come from the recorder).
            Some(iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::Chat(chat_page::ChatMsg::Tick)))
        } else if self.chat.player.is_some() {
            // A voice message / video note plays: its waveform, ring and time move.
            Some(iced::time::every(std::time::Duration::from_millis(50)).map(|_| Message::Chat(chat_page::ChatMsg::Tick)))
        } else if self.chat.animating && self.store.autoplay_gifs {
            Some(iced::time::every(std::time::Duration::from_millis(60)).map(|_| Message::Chat(chat_page::ChatMsg::Tick)))
        } else if self.chat.live_times {
            Some(iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Chat(chat_page::ChatMsg::Tick)))
        } else {
            None
        };
        // Someone is typing: a second's clock clears them when they stop and moves the dots.
        let typing_clock = (self.nav == Nav::Chat && self.chat.anyone_typing())
            .then(|| iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Chat(chat_page::ChatMsg::TypingTick)));
        let reveal_clock = self.wallet_ui.reveal.as_ref().is_some_and(|r| r.key.is_some()).then(|| iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Wallet(crate::components::wallet_panel::WalletMsg::RevealTick)));
        let wallet_clock = self.wallet_session.lock_at.is_some().then(|| iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::WalletClock));
        // A chat key kept for some days: looked at every few minutes while it's here.
        let chat_key_clock = (matches!(self.store.chat_key_lifetime, crate::e2ee::keystore::Lifetime::Days(_))
            && matches!(self.chat_key, ChatKey::Ready(_))
            && self.session.as_ref().is_some_and(|s| s.user.has_key_backup))
        .then(|| iced::time::every(std::time::Duration::from_secs(300)).map(|_| Message::Session(SessionMsg::ChatKeyClock)));
        let usage_clock = (self.app_settings_open && self.app_settings_tab == app_settings_modal::Tab::About)
            .then(|| iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::UsageTick));
        let drops = self.session.is_some().then(|| {
            iced::event::listen_with(|event, _, window| match event {
                iced::Event::Window(iced::window::Event::FileDropped(path)) => Some(Message::Window(WindowMsg::FileDropped(window, path))),
                _ => None,
            })
        });
        Subscription::batch([Some(keys), closed, avatars, live, dev, anim, typing_clock, drops, wallet_clock, chat_key_clock, reveal_clock, blur, usage_clock].into_iter().flatten())
    }
}

/// The account's wallets as saved with it (each with its sealed key).
fn account_wallets(user: &crate::api::auth::User) -> Vec<crate::store::app_store::SecureWallet> {
    user.wallets.iter().flatten().filter_map(|w| serde_json::from_value(w.clone()).ok()).collect()
}
