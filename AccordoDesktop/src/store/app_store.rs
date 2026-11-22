// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's own state (store/useAppStore.ts): wallets and their encrypted vault, the wallet
//! unlock session, contacts, locally kept cheques / invoices, embed templates, the notification
//! centre, network RPC overrides, custom tokens, and whose data is loaded on this computer.
//!
//! Saved to `<config dir>/state.json` (the web keeps it in localStorage, "saas-global-storage"),
//! leaving out what the web leaves out: the decrypted key and the unlock session.
//!
//! Not ported: the bot-commands form (commandForm / commandsBotId) — its page and API
//! (api/commands.ts) are gone from the web app.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::client::{config_dir, uuid_v4};
use crate::api::mini_apps::AppNotificationItem;

pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
/// An open cheque slot ("anyone holding the secret") is addressed to zero.
pub const BEARER_ADDRESS: &str = ZERO_ADDRESS;

/// The bell keeps this many.
const MAX_NOTIFICATIONS: usize = 50;
/// Remembered ids of announcements / app notifications already put in the bell, per user.
const MAX_INGESTED: usize = 500;

/// What the web store did on its own after a change; the app runs it with the API.
#[derive(Debug, Clone)]
pub enum StoreEffect {
    /// `auth::update_wallets` with this list (`AppStore::wallets_json`).
    SyncWallets(Vec<Value>),
    /// `auth::patch_data` with this object.
    PatchData(Value),
}

// The sealed-wallet types live in the shared core: the phone stores them the same way.
pub use accordo_core::vault::{SecureWallet, Vault, WalletVault};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenData {
    pub symbol: String,
    pub name: String,
    pub address: String,
    #[serde(default, rename = "logoURI", skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedTemplate {
    pub id: String,
    pub name: String,
    pub data: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    ChequeCreated,
    ChequeClaimed,
    ChequeRefunded,
    InvoicePaid,
    Announcement,
    App,
    #[default]
    General,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppNotification {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    /// Milliseconds since the epoch.
    pub timestamp: i64,
    pub read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A notification to add (addNotification): id, kind and time are filled in when missing.
#[derive(Debug, Clone, Default)]
pub struct NewNotification {
    pub id: Option<String>,
    pub kind: Option<NotificationKind>,
    pub title: String,
    pub body: String,
    pub timestamp: Option<i64>,
    pub data: Option<Value>,
}

/// An admin announcement as the bell takes it in.
#[derive(Debug, Clone, Default)]
pub struct Announcement {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub level: String,
    pub link: Option<String>,
    pub created_at: String,
}

/// Data kept on this computer that belongs to one account (see `account_owner`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountLocalData {
    pub encrypted_vault: Option<Vault>,
    pub contacts: Vec<Contact>,
    pub local_cheques: Vec<Value>,
    pub local_invoices: Vec<Value>,
    pub active_wallet_id: Option<String>,
    pub embed_templates: Vec<EmbedTemplate>,
}

impl AccountLocalData {
    fn is_empty(&self) -> bool {
        self.encrypted_vault.is_none() && self.contacts.is_empty() && self.local_cheques.is_empty() && self.local_invoices.is_empty() && self.embed_templates.is_empty()
    }
}

/// Data from before it was tied to an account: goes to the first account with wallets.
const LEGACY: &str = "legacy";

/// An account signed in on this computer (the switcher in the profile menu). Not a secret: its
/// token is in the OS credential store under `session:<key>` (api/session_store.rs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedAccount {
    pub id: i64,
    pub username: String,
    /// The server it's on.
    pub base: String,
    /// When it was last opened (seconds since the epoch): the switcher's order.
    #[serde(default)]
    pub last_used: i64,
    /// Its token was refused (expired, revoked): shown, but it needs signing in again.
    #[serde(default)]
    pub signed_out: bool,
    /// Its picture as the server last gave it (a public profile picture — a data URL or a link),
    /// for the switcher and the sign-in list without asking that account's server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

impl SavedAccount {
    pub fn key(&self) -> String {
        account_key(self.id, &self.username, &self.base)
    }

    /// Its key before the nick was part of it (moved on its next sign-in).
    pub fn previous_key(&self) -> String {
        previous_account_key(self.id, &self.base)
    }

    /// "accordo.me", "127.0.0.1:8000": which server, for the switcher.
    pub fn server(&self) -> String {
        let b = crate::api::client::normalize_base(&self.base);
        b.split_once("://").map(|(_, h)| h.to_string()).unwrap_or(b)
    }
}

/// `account_owner` as saved: a key ("12@https://accordo.me"), or a bare id from older versions.
fn owner_from_id_or_key<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

/// One account on one server, readable: "1:dazay@https://accordo.me". User ids repeat across
/// servers; the nick is there to read (in the OS secret store too) — usernames don't change in
/// Accordo, so it stays stable.
pub fn account_key(id: i64, username: &str, base: &str) -> String {
    format!("{id}:{username}@{}", crate::api::client::normalize_base(base))
}

/// The key's earlier form, "1@https://accordo.me": entries under it move to `account_key`.
pub fn previous_account_key(id: i64, base: &str) -> String {
    format!("{id}@{}", crate::api::client::normalize_base(base))
}

/// The test master password (AppStore::dev_wallet_password): a plain string in state.json, never
/// printed — a `{:?}` of the store shows it as `DevPassword(..)`. Written by hand, so it reads
/// leniently: `1` is the password "1", anything else odd is no password — never an error, which
/// would make the whole state.json unreadable (and the next save would replace it with a fresh one).
#[derive(Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DevPassword(pub String);

impl<'de> Deserialize<'de> for DevPassword {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(DevPassword(match Value::deserialize(d)? {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        }))
    }
}

impl std::fmt::Debug for DevPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DevPassword(..)")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppStore {
    pub user_data: Option<Value>,

    // cheques / invoices pages
    pub cheques_mode: String,
    pub cheques_selected_token: TokenData,
    pub invoices_mode: String,
    pub invoices_selected_token: TokenData,

    /// Tokens the user added by address, per chain id.
    pub custom_tokens: HashMap<String, Vec<TokenData>>,
    /// Per chain id: the user's own RPC / explorer settings over the server's.
    pub chain_overrides: HashMap<String, Value>,
    /// The drops' payout plans, reused across drops (the web's accordo:drop-plans:v1).
    #[cfg(feature = "drops")]
    pub drop_plans: Vec<crate::flows::drops::Plan>,
    /// A build without drops keeps the saved plans as they are, for a build with them.
    #[cfg(not(feature = "drops"))]
    pub drop_plans: Vec<Value>,

    /// The master-password check: wallets are sealed with the same password.
    pub encrypted_vault: Option<Vault>,
    /// For testing only: the master password, in plain text, added to state.json by hand
    /// ("dev_wallet_password": "…"). A debug build unlocks the wallets with it instead of asking;
    /// a release build ignores it. The app never writes it — it only keeps what's in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_wallet_password: Option<DevPassword>,
    /// Whose data is loaded (vault check, contacts, local cheques / invoices, active wallet,
    /// templates): id@server (`account_key`). Another account signing in gets its own; the
    /// previous one's waits in the stash. Older files have a bare user id here (a number).
    #[serde(deserialize_with = "owner_from_id_or_key")]
    pub account_owner: Option<String>,
    pub account_stash: HashMap<String, AccountLocalData>,

    /// Wallet unlock session — never saved (the unlock lasts until restart or its timer).
    #[serde(skip)]
    pub wallets_unlocked: bool,
    /// When the wallets lock again (ms since the epoch); None = until restart.
    #[serde(skip)]
    pub wallet_lock_at: Option<i64>,
    /// The decrypted key in memory — never saved.
    #[serde(skip)]
    pub decrypted_key: Option<String>,

    pub wallets: Vec<SecureWallet>,
    pub active_wallet_id: Option<String>,
    pub active_chain: Option<String>,
    pub active_chain_id: Option<String>,

    /// Address book for cheques and invoices.
    pub contacts: Vec<Contact>,
    pub local_cheques: Vec<Value>,
    pub local_invoices: Vec<Value>,
    pub embed_templates: Vec<EmbedTemplate>,

    /// Accounts signed in on this computer, and the one open (its key). The tokens are in the OS
    /// credential store, not here.
    pub accounts: Vec<SavedAccount>,
    pub active_account: Option<String>,

    /// The signed-in account's bell; other accounts' lists wait in `notifications_by_user`.
    pub notifications: Vec<AppNotification>,
    pub notification_owner: Option<String>,
    pub notifications_by_user: HashMap<String, Vec<AppNotification>>,
    /// Announcement / app-notification ids already put in the bell, per user ("apps:<id>" for
    /// apps), so one the user dismissed isn't added again on the next sync.
    pub ingested_announcements: HashMap<String, Vec<i64>>,

    /// Embeds, widgets and experimental tools; on the desktop also the developer tools (F12).
    pub dev_mode: bool,
    /// Desktop only: interface scale (1.0 = 100 %).
    pub ui_scale: f32,
    /// "Keep unlocked" picked last time, in minutes (0: until restart); None: 1 hour.
    #[serde(default)]
    pub wallet_lock_minutes: Option<u32>,

    // ── app preferences (App settings window) ──
    /// The page shown after signing in: chat, cheques, invoices, drops, crowdfund, settings.
    pub start_page: String,
    /// GIFs and stickers move (off: their first frame).
    pub autoplay_gifs: bool,
    /// Messages veiled until the pointer is on them.
    pub blur_messages: bool,
    /// The last message under each chat in the list is hidden.
    pub blur_previews: bool,
    /// Switching to another window covers this one.
    pub hide_on_blur: bool,
    /// Covering the window also locks the wallets.
    pub lock_wallets_on_hide: bool,
    /// Keyboard shortcuts on.
    pub shortcuts: bool,
    /// Keys the user gave the shortcuts (hotkeys.rs): action id → "Ctrl+Shift+K", "" = none.
    /// Only the ones that differ from the defaults.
    pub hotkeys: crate::hotkeys::Overrides,
    /// Sections the user hid (App settings → Modules). Cards sent to them still work.
    pub hidden_modules: Vec<crate::modules::Module>,
    /// The developer tools window: its last section and filter (the entries stay in memory only).
    pub devtools_tab: crate::components::devtools_panel::Tab,
    pub devtools_filter: String,
    /// The developer tools open in a window of their own (false: docked under the app).
    #[serde(default)]
    pub devtools_detached: bool,
    /// They were open when the app last closed: they open again at start (Developer mode on).
    #[serde(default)]
    pub devtools_open: bool,
}

impl Default for AppStore {
    fn default() -> Self {
        Self {
            user_data: None,
            cheques_mode: "cheque".into(),
            cheques_selected_token: TokenData { symbol: "select token".into(), name: "USD Coin".into(), address: String::new(), logo_uri: None, decimals: None },
            invoices_mode: "issue".into(),
            invoices_selected_token: TokenData { symbol: "USDC".into(), name: "USD Coin".into(), address: String::new(), logo_uri: None, decimals: None },
            custom_tokens: HashMap::new(),
            #[cfg(feature = "drops")]
            drop_plans: vec![crate::flows::drops::Plan::default()],
            #[cfg(not(feature = "drops"))]
            drop_plans: Vec::new(),
            chain_overrides: HashMap::new(),
            encrypted_vault: None,
            dev_wallet_password: None,
            account_owner: None,
            account_stash: HashMap::new(),
            wallets_unlocked: false,
            wallet_lock_at: None,
            decrypted_key: None,
            wallets: Vec::new(),
            active_wallet_id: None,
            active_chain: None,
            active_chain_id: None,
            contacts: Vec::new(),
            local_cheques: Vec::new(),
            local_invoices: Vec::new(),
            embed_templates: Vec::new(),
            accounts: Vec::new(),
            active_account: None,
            notifications: Vec::new(),
            notification_owner: None,
            notifications_by_user: HashMap::new(),
            ingested_announcements: HashMap::new(),
            dev_mode: false,
            ui_scale: 1.0,
            wallet_lock_minutes: None,
            start_page: "chat".into(),
            autoplay_gifs: true,
            blur_messages: false,
            blur_previews: false,
            hide_on_blur: false,
            lock_wallets_on_hide: false,
            shortcuts: true,
            hotkeys: Default::default(),
            hidden_modules: Vec::new(),
            devtools_tab: Default::default(),
            devtools_filter: String::new(),
            devtools_detached: false,
            devtools_open: false,
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn new_id() -> String {
    uuid_v4().unwrap_or_else(|| format!("{}-{}", now_ms(), std::process::id()))
}

/// JS `new Date(iso).getTime()`: RFC 3339, or a date-time without a zone read as local time.
fn parse_ms(iso: &str) -> Option<i64> {
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(iso) {
        return Some(t.timestamp_millis());
    }
    let naive = chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    naive.and_local_timezone(chrono::Local).single().map(|t| t.timestamp_millis())
}

/// Newest first, at most the bell's size.
fn newest(mut list: Vec<AppNotification>) -> Vec<AppNotification> {
    list.sort_by_key(|n| std::cmp::Reverse(n.timestamp));
    list.truncate(MAX_NOTIFICATIONS);
    list
}

/// Adds ids to a remembered list (insertion order, no duplicates), keeping the last 500.
fn remember(list: &mut Vec<i64>, ids: impl IntoIterator<Item = i64>) {
    let mut seen: HashSet<i64> = list.iter().copied().collect();
    for id in ids {
        if seen.insert(id) {
            list.push(id);
        }
    }
    if list.len() > MAX_INGESTED {
        list.drain(..list.len() - MAX_INGESTED);
    }
}

impl AppStore {
    // ── saving ──

    pub fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("state.json"))
    }

    /// The saved state, or a fresh one (missing or unreadable file).
    pub fn load() -> Self {
        let read = |p: Option<PathBuf>| p.and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Self>(&b).ok());
        let mut s: Self = match read(Self::path()) {
            Some(s) => s,
            // A new ACCORDO_PROFILE starts with the usual profile's account list (none open): their
            // sign-ins are in the shared secret store, so each is a click away on the sign-in screen.
            None if crate::api::client::profile().is_some() => {
                let accounts = read(crate::api::client::default_config_dir().map(|d| d.join("state.json"))).map(|d| d.accounts).unwrap_or_default();
                Self { accounts, ..Default::default() }
            }
            None => Self::default(),
        };
        s.upgrade_keys();
        s
    }

    /// Account keys saved in their earlier form (id@server) → id:nick@server, for the accounts
    /// on the list (the open one, whose data is loaded, the stashed data).
    fn upgrade_keys(&mut self) {
        let map: Vec<(String, String)> = self.accounts.iter().map(|a| (a.previous_key(), a.key())).collect();
        let up = |k: &str| map.iter().find(|(old, _)| old == k).map(|(_, new)| new.clone());
        if let Some(new) = self.active_account.as_deref().and_then(up) {
            self.active_account = Some(new);
        }
        if let Some(new) = self.account_owner.as_deref().and_then(up) {
            self.account_owner = Some(new);
        }
        for (old, new) in &map {
            if let Some(d) = self.account_stash.remove(old) {
                self.account_stash.entry(new.clone()).or_insert(d);
            }
        }
    }

    /// Writes it (to a temporary file first, so a crash never leaves half a file). On Unix the
    /// file is readable by the user only: it holds the encrypted vault and the address book.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("no config directory"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(tmp, path)
    }

    // ── tokens, networks ──

    pub fn add_custom_token(&mut self, chain_id: &str, token: TokenData) {
        self.custom_tokens.entry(chain_id.to_string()).or_default().push(token);
    }

    /// Merges the fields of `data` into the network's overrides.
    pub fn update_chain_override(&mut self, chain_id_dec: &str, data: Value) {
        let entry = self.chain_overrides.entry(chain_id_dec.to_string()).or_insert_with(|| json!({}));
        match (entry.as_object_mut(), data) {
            (Some(map), Value::Object(new)) => map.extend(new),
            (_, other) => *entry = other,
        }
    }

    /// The user's own RPC URLs for a network (the web's chainOverrides[id].rpcUrls), if set.
    pub fn rpc_override(&self, chain_id: &str) -> Option<Vec<String>> {
        let list: Vec<String> = self.chain_overrides.get(chain_id)?.get("rpcUrls")?.as_array()?.iter().filter_map(|u| u.as_str().map(str::to_string)).collect();
        (!list.is_empty()).then_some(list)
    }

    /// The user's own block explorer for a network (blockExplorerUrls[0]), if set.
    pub fn explorer_override(&self, chain_id: &str) -> Option<String> {
        let url = self.chain_overrides.get(chain_id)?.get("blockExplorerUrls")?.get(0)?.as_str()?;
        (!url.is_empty()).then(|| url.to_string())
    }

    /// Sets (Some) or clears (None → the server's default) both settings of a network; a network
    /// with nothing left of its own is dropped from the overrides. Other keys the web may have
    /// put there are kept.
    pub fn set_chain_settings(&mut self, chain_id: &str, rpc: Option<Vec<String>>, explorer: Option<String>) {
        let entry = self.chain_overrides.entry(chain_id.to_string()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        let map = entry.as_object_mut().expect("an object");
        match rpc {
            Some(list) => map.insert("rpcUrls".into(), json!(list)),
            None => map.remove("rpcUrls"),
        };
        match explorer {
            Some(url) => map.insert("blockExplorerUrls".into(), json!([url])),
            None => map.remove("blockExplorerUrls"),
        };
        if map.is_empty() {
            self.chain_overrides.remove(chain_id);
        }
    }

    // ── accounts on this computer ──

    /// An account is open: first in the list, signed in.
    pub fn remember_account(&mut self, id: i64, username: &str, base: &str) -> String {
        let key = account_key(id, username, base);
        let now = chrono::Utc::now().timestamp();
        let server = crate::api::client::normalize_base(base);
        match self.accounts.iter_mut().find(|a| a.id == id && crate::api::client::normalize_base(&a.base) == server) {
            Some(a) => {
                a.username = username.to_string();
                a.last_used = now;
                a.signed_out = false;
            }
            None => {
                tracing::info!("accounts: added {key}");
                self.accounts.push(SavedAccount { id, username: username.to_string(), base: crate::api::client::normalize_base(base), last_used: now, signed_out: false, avatar: None });
            }
        }
        self.active_account = Some(key.clone());
        key
    }

    /// Keeps the account's picture with it; true if it changed (then save).
    pub fn set_account_avatar(&mut self, id: i64, base: &str, avatar: Option<&str>) -> bool {
        let server = crate::api::client::normalize_base(base);
        let avatar = avatar.map(str::trim).filter(|a| !a.is_empty() && a.len() <= 256 * 1024).map(str::to_string);
        match self.accounts.iter_mut().find(|a| a.id == id && crate::api::client::normalize_base(&a.base) == server) {
            Some(a) if a.avatar != avatar => {
                a.avatar = avatar;
                true
            }
            _ => false,
        }
    }

    pub fn account(&self, key: &str) -> Option<&SavedAccount> {
        self.accounts.iter().find(|a| a.key() == key)
    }

    /// Its token was refused: it stays in the list, marked.
    pub fn mark_signed_out(&mut self, key: &str) {
        tracing::info!("accounts: {key} needs signing in again");
        if let Some(a) = self.accounts.iter_mut().find(|a| a.key() == key) {
            a.signed_out = true;
        }
    }

    /// Signed out of for good: off the list.
    pub fn forget_account(&mut self, key: &str) {
        tracing::info!("accounts: removed {key} from this computer");
        self.accounts.retain(|a| a.key() != key);
        if self.active_account.as_deref() == Some(key) {
            self.active_account = None;
        }
    }

    /// The switcher's list: most recent first.
    pub fn accounts_by_use(&self) -> Vec<&SavedAccount> {
        let mut v: Vec<&SavedAccount> = self.accounts.iter().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.last_used));
        v
    }

    /// Where to go after signing out of one: the most recent other account that's still signed in.
    pub fn next_account(&self, leaving: &str) -> Option<String> {
        self.accounts_by_use().into_iter().find(|a| a.key() != leaving && !a.signed_out).map(SavedAccount::key)
    }

    // ── whose data is loaded ──

    fn account_data(&self) -> AccountLocalData {
        AccountLocalData {
            encrypted_vault: self.encrypted_vault.clone(),
            contacts: self.contacts.clone(),
            local_cheques: self.local_cheques.clone(),
            local_invoices: self.local_invoices.clone(),
            active_wallet_id: self.active_wallet_id.clone(),
            embed_templates: self.embed_templates.clone(),
        }
    }

    fn put_account_data(&mut self, d: AccountLocalData) {
        self.encrypted_vault = d.encrypted_vault;
        self.contacts = d.contacts;
        self.local_cheques = d.local_cheques;
        self.local_invoices = d.local_invoices;
        self.active_wallet_id = d.active_wallet_id;
        self.embed_templates = d.embed_templates;
    }

    /// An account signs in (`account`: id@server): its own data comes back, the previous
    /// account's is set aside. Data kept under its bare id (older versions, before the server was
    /// part of it) goes to the first account with that id; data from before it was tied to an
    /// account at all goes to the first account with wallets (`has_wallets`).
    pub fn set_account_owner(&mut self, account: &str, user_id: i64, has_wallets: bool) {
        let bare = user_id.to_string();
        if self.account_owner.as_deref() == Some(account) {
            return;
        }
        if self.account_owner.as_deref() == Some(bare.as_str()) {
            // What's loaded is this id's from before: it becomes this account's.
            self.account_owner = Some(account.to_string());
            return;
        }
        let current = self.account_data();
        match self.account_owner.take() {
            Some(prev) => {
                self.account_stash.insert(prev, current);
            }
            None if !current.is_empty() => {
                self.account_stash.insert(LEGACY.into(), current);
            }
            None => {}
        }
        let previous = account.split_once(':').map(|(id, rest)| format!("{id}@{}", rest.split_once('@').map_or("", |(_, s)| s)));
        let mut mine = self.account_stash.remove(account).or_else(|| previous.and_then(|p| self.account_stash.remove(&p))).or_else(|| self.account_stash.remove(&bare));
        if mine.is_none() && has_wallets {
            mine = self.account_stash.remove(LEGACY);
        }
        self.put_account_data(mine.unwrap_or_default());
        self.account_owner = Some(account.to_string());
    }

    // ── wallet session ──

    pub fn set_wallet_session(&mut self, unlocked: bool, lock_at: Option<i64>) {
        self.wallets_unlocked = unlocked;
        self.wallet_lock_at = if unlocked { lock_at } else { None };
    }

    // ── wallets ──

    /// The list as the account stores it (`auth::update_wallets`).
    pub fn wallets_json(&self) -> Vec<Value> {
        self.wallets.iter().filter_map(|w| serde_json::to_value(w).ok()).collect()
    }

    fn sync_wallets(&self) -> StoreEffect {
        StoreEffect::SyncWallets(self.wallets_json())
    }

    /// The account's wallets, cleaned up: wallets created before ids were random could share one
    /// (then rename / remove hit both) — every duplicate gets a fresh id; and the Tron twins that
    /// were added automatically next to EVM wallets (same key vault) are dropped — Tron is gone and
    /// nothing is lost; a Tron wallet with its own key stays. When anything changed, the fixed list
    /// should be saved back to the account once (the returned effect).
    pub fn set_wallets(&mut self, wallets: Vec<SecureWallet>) -> Option<StoreEffect> {
        let evm_vaults: HashSet<String> = wallets.iter().filter(|w| w.chain.as_deref() != Some("Tron")).map(|w| w.vault.canonical()).collect();
        let before = wallets.len();
        let kept: Vec<SecureWallet> = wallets.into_iter().filter(|w| !(w.chain.as_deref() == Some("Tron") && evm_vaults.contains(&w.vault.canonical()))).collect();
        let mut fixed = kept.len() != before;
        let mut seen = HashSet::new();
        self.wallets = kept
            .into_iter()
            .map(|mut w| {
                if w.id.is_empty() || seen.contains(&w.id) {
                    fixed = true;
                    w.id = new_id();
                }
                seen.insert(w.id.clone());
                w
            })
            .collect();
        fixed.then(|| self.sync_wallets())
    }

    pub fn add_wallet(&mut self, name: String, address: String, vault: Vault, chain: String) -> StoreEffect {
        self.wallets.push(SecureWallet { id: new_id(), name, address, vault: WalletVault::Sealed(vault), chain: Some(chain) });
        self.sync_wallets()
    }

    pub fn remove_wallet(&mut self, id: &str) -> StoreEffect {
        self.wallets.retain(|w| w.id != id);
        if self.active_wallet_id.as_deref() == Some(id) {
            self.active_wallet_id = None;
        }
        self.sync_wallets()
    }

    pub fn rename_wallet(&mut self, id: &str, name: String) -> StoreEffect {
        if let Some(w) = self.wallets.iter_mut().find(|w| w.id == id) {
            w.name = name;
        }
        self.sync_wallets()
    }

    pub fn set_active_wallet(&mut self, id: Option<String>, chain: Option<String>) {
        self.active_wallet_id = id;
        self.active_chain = chain;
    }

    pub fn active_wallet(&self) -> Option<&SecureWallet> {
        let id = self.active_wallet_id.as_deref()?;
        self.wallets.iter().find(|w| w.id == id || w.address.eq_ignore_ascii_case(id))
    }

    // ── contacts, local records, templates ──

    /// One contact per address: adding an address again replaces its name.
    pub fn add_contact(&mut self, name: String, address: String) {
        self.contacts.retain(|c| !c.address.eq_ignore_ascii_case(&address));
        self.contacts.push(Contact { id: new_id(), name, address });
    }

    pub fn remove_contact(&mut self, id: &str) {
        self.contacts.retain(|c| c.id != id);
    }

    pub fn add_local_cheque(&mut self, cheque: Value) {
        self.local_cheques.insert(0, cheque);
    }

    pub fn add_local_invoice(&mut self, invoice: Value) {
        self.local_invoices.insert(0, invoice);
    }

    fn sync_templates(&self) -> StoreEffect {
        StoreEffect::PatchData(json!({ "embedTemplates": self.embed_templates }))
    }

    pub fn add_embed_template(&mut self, name: String, data: Value) -> StoreEffect {
        self.embed_templates.push(EmbedTemplate { id: new_id(), name, data });
        self.sync_templates()
    }

    pub fn remove_embed_template(&mut self, id: &str) -> StoreEffect {
        self.embed_templates.retain(|t| t.id != id);
        self.sync_templates()
    }

    // ── notifications ──

    /// The signed-in account changed: its own list comes back, the previous one's is set aside.
    /// Anything in the list while no one owned it arrived for the account now signing in (e.g. a
    /// sync that raced the switch) — it's handed to that account, not dropped.
    pub fn set_notification_owner(&mut self, user_id: Option<String>) {
        if user_id == self.notification_owner {
            return;
        }
        if let Some(prev) = &self.notification_owner {
            self.notifications_by_user.insert(prev.clone(), std::mem::take(&mut self.notifications));
        }
        let mut active = user_id.as_ref().and_then(|u| self.notifications_by_user.get(u).cloned()).unwrap_or_default();
        if self.notification_owner.is_none() && user_id.is_some() && !self.notifications.is_empty() {
            let ids: HashSet<String> = active.iter().map(|n| n.id.clone()).collect();
            let unowned = std::mem::take(&mut self.notifications).into_iter().filter(|n| !ids.contains(&n.id));
            active = newest(unowned.chain(active).collect());
        }
        self.notification_owner = user_id;
        self.notifications = active;
    }

    /// Adds one to the top of the bell (once per id), keeping the last 50.
    pub fn add_notification(&mut self, n: NewNotification) {
        if n.id.as_ref().is_some_and(|id| self.notifications.iter().any(|x| &x.id == id)) {
            return;
        }
        let id = n.id.unwrap_or_else(|| format!("{}-{}", now_ms(), &new_id()[..5]));
        self.notifications.truncate(MAX_NOTIFICATIONS - 1);
        self.notifications.insert(
            0,
            AppNotification { id, kind: n.kind.unwrap_or_default(), title: n.title, body: n.body, timestamp: n.timestamp.unwrap_or_else(now_ms), read: false, data: n.data },
        );
    }

    /// Notifications from mini-apps (live or caught up); each is added once, so a dismissed one stays gone.
    pub fn ingest_app_notifications(&mut self, user_id: &str, items: &[AppNotificationItem]) {
        let key = format!("apps:{user_id}");
        let seen: HashSet<i64> = self.ingested_announcements.get(&key).map(|v| v.iter().copied().collect()).unwrap_or_default();
        let fresh: Vec<&AppNotificationItem> = items.iter().filter(|n| !seen.contains(&n.id)).collect();
        if fresh.is_empty() {
            return;
        }
        let added: Vec<AppNotification> = fresh
            .iter()
            .map(|n| AppNotification {
                id: format!("app-{}", n.id),
                kind: NotificationKind::App,
                title: format!("{}: {}", n.app.name, n.title),
                body: n.body.clone(),
                timestamp: n.created_at.as_deref().and_then(parse_ms).unwrap_or_else(now_ms),
                read: false,
                data: Some(json!({ "slug": n.app.slug, "icon_url": n.app.icon_url, "start_param": n.start_param })),
            })
            .collect();
        remember(self.ingested_announcements.entry(key).or_default(), fresh.iter().map(|n| n.id));
        self.notifications = newest(added.into_iter().chain(std::mem::take(&mut self.notifications)).collect());
    }

    /// Admin announcements into the bell, once each; retracted ones are taken out.
    pub fn ingest_announcements(&mut self, user_id: &str, announcements: &[Announcement], retracted_ids: &[i64]) {
        let seen: HashSet<i64> = self.ingested_announcements.get(user_id).map(|v| v.iter().copied().collect()).unwrap_or_default();
        let retracted: HashSet<String> = retracted_ids.iter().map(|id| format!("ann-{id}")).collect();
        let fresh: Vec<&Announcement> = announcements.iter().filter(|a| !seen.contains(&a.id)).collect();
        let added: Vec<AppNotification> = fresh
            .iter()
            .map(|a| AppNotification {
                id: format!("ann-{}", a.id),
                kind: NotificationKind::Announcement,
                title: a.title.clone(),
                body: a.body.clone(),
                timestamp: parse_ms(&a.created_at).unwrap_or_else(now_ms),
                read: false,
                data: Some(json!({ "level": a.level, "link": a.link, "announcement_id": a.id })),
            })
            .collect();
        remember(self.ingested_announcements.entry(user_id.to_string()).or_default(), fresh.iter().map(|a| a.id));
        let kept = std::mem::take(&mut self.notifications).into_iter().filter(|n| !retracted.contains(&n.id));
        self.notifications = newest(added.into_iter().chain(kept).collect());
    }

    pub fn mark_notification_as_read(&mut self, id: &str) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    }

    pub fn mark_all_notifications_as_read(&mut self) {
        self.notifications.iter_mut().for_each(|n| n.read = true);
    }

    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
    }

    pub fn remove_notification(&mut self, id: &str) {
        self.notifications.retain(|n| n.id != id);
    }

    pub fn unread_notifications(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_is_kept_as_written_and_never_printed() {
        // Added by hand: read, written back on every save, absent when it wasn't there.
        let store: AppStore = serde_json::from_str(r#"{"dev_wallet_password": "hunter2"}"#).unwrap();
        assert_eq!(store.dev_wallet_password, Some(DevPassword("hunter2".into())));
        assert_eq!(serde_json::to_value(&store).unwrap()["dev_wallet_password"], "hunter2");
        assert!(serde_json::to_value(AppStore::default()).unwrap().get("dev_wallet_password").is_none());
        assert!(!format!("{store:?}").contains("hunter2"));
        // Typed without quotes, or nonsense: the file still reads (a number is the password).
        let store: AppStore = serde_json::from_str(r#"{"dev_wallet_password": 1, "shortcuts": false}"#).unwrap();
        assert_eq!(store.dev_wallet_password, Some(DevPassword("1".into())));
        assert!(!store.shortcuts, "the rest of the file was read");
        let store: AppStore = serde_json::from_str(r#"{"dev_wallet_password": [true]}"#).unwrap();
        assert_eq!(store.dev_wallet_password, Some(DevPassword(String::new())));
    }

    fn vault(c: &str) -> Vault {
        Vault { salt: "s".into(), iv: "i".into(), cipher: c.into(), iter: None }
    }

    fn wallet(id: &str, chain: &str, cipher: &str) -> SecureWallet {
        SecureWallet { id: id.into(), name: id.into(), address: format!("0x{id}"), vault: WalletVault::Sealed(vault(cipher)), chain: Some(chain.into()) }
    }

    #[test]
    fn wallets_lose_tron_twins_and_duplicate_ids() {
        let mut s = AppStore::default();
        let text_vault = SecureWallet { vault: WalletVault::Text(serde_json::to_string(&vault("a")).unwrap()), ..wallet("t", "Tron", "") };
        let effect = s.set_wallets(vec![wallet("1", "EVM", "a"), text_vault, wallet("1", "EVM", "b"), wallet("t2", "Tron", "own")]);
        assert!(matches!(effect, Some(StoreEffect::SyncWallets(ref l)) if l.len() == 3));
        assert_eq!(s.wallets.len(), 3); // the Tron twin of "a" is gone, the Tron wallet with its own key stays
        assert_ne!(s.wallets[0].id, s.wallets[1].id);
        assert!(s.set_wallets(s.wallets.clone()).is_none()); // clean list: nothing to save
    }

    #[test]
    fn accounts_keep_their_own_data() {
        let mut s = AppStore::default();
        s.add_contact("old".into(), "0xold".into()); // before any account
        s.set_account_owner("1@https://a", 1, false);
        assert!(s.contacts.is_empty()); // legacy data waits for an account with wallets
        s.add_contact("one".into(), "0x1".into());
        s.set_account_owner("2@https://a", 2, true);
        assert_eq!(s.contacts[0].name, "old");
        s.set_account_owner("1@https://a", 1, true);
        assert_eq!(s.contacts[0].name, "one");
        // The same id on another server is another account.
        s.set_account_owner("1@http://local", 1, true);
        assert!(s.contacts.is_empty());
        s.set_account_owner("1@https://a", 1, true);
        assert_eq!(s.contacts[0].name, "one");
    }

    #[test]
    fn data_under_a_bare_id_goes_to_that_id() {
        // An older state.json: owner 7 (a number), and user 8's data stashed under "8".
        let mut s: AppStore = serde_json::from_value(json!({"account_owner": 7, "account_stash": {"8": {"contacts": [{"id": "c8", "name": "eight", "address": "0x8"}]}}})).unwrap();
        s.add_contact("seven".into(), "0x7".into());
        s.set_account_owner("7@https://a", 7, false);
        assert_eq!(s.account_owner.as_deref(), Some("7@https://a"));
        assert_eq!(s.contacts[0].name, "seven", "what's loaded stays with its id");
        s.set_account_owner("8@https://a", 8, false);
        assert_eq!(s.contacts[0].name, "eight");
    }

    #[test]
    fn bell_ingests_once_and_drops_retracted() {
        let mut s = AppStore::default();
        let ann = |id, at: &str| Announcement { id, title: format!("t{id}"), body: String::new(), level: "info".into(), link: None, created_at: at.into() };
        s.ingest_announcements("7", &[ann(1, "2026-01-01T00:00:00+00:00"), ann(2, "2026-02-01T00:00:00+00:00")], &[]);
        assert_eq!(s.notifications.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["ann-2", "ann-1"]);
        s.remove_notification("ann-2");
        s.ingest_announcements("7", &[ann(2, "2026-02-01T00:00:00+00:00")], &[1]);
        assert!(s.notifications.is_empty()); // 2 was dismissed (not re-added), 1 retracted
        s.add_notification(NewNotification { id: Some("x".into()), title: "a".into(), ..Default::default() });
        s.add_notification(NewNotification { id: Some("x".into()), title: "b".into(), ..Default::default() });
        assert_eq!(s.notifications.len(), 1);
    }

    #[test]
    fn notifications_follow_the_account() {
        let mut s = AppStore::default();
        s.add_notification(NewNotification { title: "early".into(), ..Default::default() }); // nobody signed in yet
        s.set_notification_owner(Some("1".into()));
        assert_eq!(s.notifications.len(), 1); // handed to the account signing in
        s.set_notification_owner(Some("2".into()));
        assert!(s.notifications.is_empty());
        s.set_notification_owner(Some("1".into()));
        assert_eq!(s.notifications[0].title, "early");
    }

    #[test]
    fn saved_state_leaves_out_the_session() {
        let mut s = AppStore::default();
        s.decrypted_key = Some("secret".into());
        s.set_wallet_session(true, Some(1));
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret") && !json.contains("wallets_unlocked"));
        let back: AppStore = serde_json::from_str(&json).unwrap();
        assert!(back.decrypted_key.is_none() && !back.wallets_unlocked);
    }
}

#[cfg(test)]
mod account_tests {
    use super::*;

    #[test]
    fn accounts_are_per_server_and_ordered_by_use() {
        let mut s = AppStore::default();
        let a = s.remember_account(12, "dazay", "https://accordo.me/");
        let b = s.remember_account(12, "zuko", "http://127.0.0.1:8000");
        assert_ne!(a, b, "the same id on two servers is two accounts");
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.active_account.as_deref(), Some(b.as_str()));
        // Signing in again updates, doesn't add.
        s.accounts[0].last_used = 0;
        s.remember_account(12, "dazay", "https://accordo.me");
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.accounts_by_use()[0].username, "dazay");
        assert_eq!(s.next_account(&a).as_deref(), Some(b.as_str()));
        s.mark_signed_out(&b);
        assert_eq!(s.next_account(&a), None, "an account that needs signing in isn't switched to by itself");
        s.forget_account(&a);
        assert_eq!(s.active_account, None);
        assert_eq!(s.accounts.len(), 1);
    }

    #[test]
    fn keys_carry_the_nick_and_old_ones_move() {
        assert_eq!(account_key(1, "dazay", "https://accordo.me/"), "1:dazay@https://accordo.me");
        // A state.json from before the nick was in the key.
        let mut s: AppStore = serde_json::from_value(json!({
            "accounts": [{"id": 1, "username": "dazay", "base": "https://accordo.me"}, {"id": 1, "username": "dazay", "base": "http://0.0.0.0:8000"}],
            "active_account": "1@https://accordo.me",
            "account_owner": "1@https://accordo.me",
            "account_stash": {"1@http://0.0.0.0:8000": {"contacts": [{"id": "c", "name": "local", "address": "0x1"}]}},
        }))
        .unwrap();
        s.upgrade_keys();
        assert_eq!(s.active_account.as_deref(), Some("1:dazay@https://accordo.me"));
        assert_eq!(s.account_owner.as_deref(), Some("1:dazay@https://accordo.me"));
        s.set_account_owner("1:dazay@http://0.0.0.0:8000", 1, false);
        assert_eq!(s.contacts[0].name, "local");
        assert_eq!(s.accounts.len(), 2, "the same nick on two servers is two accounts");
    }
}
