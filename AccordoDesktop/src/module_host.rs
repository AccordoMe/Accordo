// SPDX-License-Identifier: GPL-3.0-or-later
//! The host side of the module API: what Accordo answers a module (a mini-app) that asks it for
//! something. The same protocol as the web's mini-apps (docs/MINIAPP_API_RU.md, the web's
//! src/components/miniapps/bridge/): one module can run in both.
//!
//! Pure logic, no UI: a call either gets its answer right away, or waits for the user (a window:
//! consent, a permission, a link, a token, a payment). The app shows the window and hands the
//! user's answer back with `Host::answered`, which finishes the call — or asks for the next step.
//!
//! For now the only module is the sandbox the developer tools drive ("Test module": the
//! permissions its registration lists are switches in the console, what the user grants lives in
//! memory, nothing goes to the server). Modules in their own window come next, on the same host.
//!
//! What a module never gets: keys, signatures, balances, addresses it could send money to, a
//! cheque's link or claim key. Money moves only through Accordo's own windows (payments.*).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::api::mini_apps::{all_permissions, expand_permissions, legacy_permission, PERMISSION_GROUPS};

pub const PROTOCOL: u32 = 1;
/// Requests of one module waiting on the user at once (§13); more get 5005.
pub const MAX_WAITING: usize = 3;

/// A failure the module is told about (§12): EIP-1193 / Accordo codes, an English message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    pub code: i32,
    pub message: String,
}

impl BridgeError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn rejected() -> Self {
        Self::new(4001, "User rejected the request")
    }
    pub fn unauthorized(why: impl Into<String>) -> Self {
        Self::new(4100, why)
    }
    pub fn unsupported(what: &str) -> Self {
        Self::new(4200, format!("{what} isn't supported"))
    }
    pub fn no_handshake() -> Self {
        Self::new(4900, "Call accordo_handshake first")
    }
    pub fn internal(why: impl Into<String>) -> Self {
        Self::new(5000, why)
    }
    pub fn chat_only() -> Self {
        Self::new(5004, "Only when the app is opened from a chat")
    }
    pub fn busy() -> Self {
        Self::new(5005, "Too many requests")
    }
    pub fn no_invoice() -> Self {
        Self::new(5006, "No such invoice")
    }
    pub fn closed(status: &str) -> Self {
        Self::new(5007, format!("This invoice is {} — nothing to pay", status.to_lowercase()))
    }
    pub fn params(why: impl Into<String>) -> Self {
        Self::new(-32602, why)
    }
}

/// What the user said about one permission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Grant {
    pub granted: bool,
    /// Answered at all (a "no" is remembered: methods needing it get 4100 without a window).
    pub decided: bool,
}

/// What each permission lets a module do, as the user reads it (the web's PERMISSION_TEXT).
pub fn permission_text(p: &str) -> &'static str {
    match p {
        "profile.nick" => "See your nickname",
        "profile.avatar" => "See your avatar",
        "profile.email" => "See your email address",
        "profile.wallet" => "See your wallet address and network — it can't sign or send anything",
        "payments.pay" => "Ask you to pay someone — every payment is confirmed separately",
        "payments.invoice" => "Ask you to issue an invoice — each one is confirmed separately",
        "history.invoices" => "See your invoices — amounts, status, notes and nicknames, never addresses",
        "history.cheques" => "See your cheques — amounts and status, never addresses or links",
        "notify.push" => "Send you notifications (at most 3 a day)",
        "notify.messages" => "Write to you in its own chat (in your Apps folder)",
        "chat.send" => "Post in this chat — only the chat's owner can allow it",
        "chat.members" => "See who's in this chat (nicknames only, no addresses)",
        _ => "",
    }
}

/// The switch's label on the consent screen (the web's TOGGLE_LABEL).
pub fn toggle_label(p: &str) -> &'static str {
    match p {
        "profile.nick" => "Your nickname",
        "profile.avatar" => "Your avatar",
        "profile.email" => "Your email",
        "profile.wallet" => "Your wallet & network",
        "payments.pay" => "Ask you to pay",
        "payments.invoice" => "Ask you to issue invoices",
        "history.invoices" => "Your invoices",
        "history.cheques" => "Your cheques",
        "notify.push" => "Notifications",
        "notify.messages" => "Messages to you",
        "chat.send" => "Post in this chat",
        "chat.members" => "This chat's members",
        _ => "",
    }
}

/// The group a permission is shown under (Profile, Payments…).
pub fn group_of(p: &str) -> &'static str {
    PERMISSION_GROUPS.iter().find(|(_, _, ps)| ps.contains(&p)).map_or("Other", |(_, title, _)| *title)
}

/// A network as a module sees it: CAIP-2 id, name, test or not (§6).
#[derive(Debug, Clone, PartialEq)]
pub struct ChainView {
    pub id: String,
    pub name: String,
    pub testnet: bool,
}

impl ChainView {
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "testnet": self.testnet })
    }
}

/// What the module may learn of the user and their Accordo right now — read fresh on every call
/// (they switch wallets and networks while it's open).
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub username: String,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    /// The current network (None: none picked / not one a module can use).
    pub chain: Option<ChainView>,
    /// The wallet that's on for that network (the active one if it fits, else the first one).
    pub wallet: Option<String>,
    /// The network's tokens as modules see them (`token_view`), no balances.
    pub tokens: Vec<Value>,
}

/// A token as a module sees it. Native coin: address null. Never a balance.
pub fn token_view(symbol: &str, name: &str, address: &str, decimals: u8, icon: Option<&str>, chain: Option<&ChainView>) -> Value {
    let a = address.trim();
    let native = a.is_empty() || a.eq_ignore_ascii_case("native") || a.trim_start_matches("0x").chars().all(|c| c == '0');
    json!({
        "symbol": symbol,
        "name": if name.is_empty() { symbol } else { name },
        "address": if native { Value::Null } else { Value::String(a.to_string()) },
        "decimals": decimals,
        "iconUrl": icon,
        "chain": chain.map(ChainView::to_json),
    })
}

/// A cheque the module asked for (payments.requestCheque), checked.
#[derive(Debug, Clone, PartialEq)]
pub struct ChequeAsk {
    pub amount: String,
    /// A symbol, a token address, or "native".
    pub token: String,
    pub note: String,
    /// "@nick", or None: the user picks.
    pub to: Option<String>,
}

/// What a call waits for. The app shows the matching window (or runs the flow) and answers
/// with the matching `Answer`.
#[derive(Debug, Clone, PartialEq)]
pub enum Wait {
    /// The handshake's consent screen: a switch for each permission not answered yet, all on.
    Consent(Vec<String>),
    /// "Allow <module>?": the permissions it doesn't hold yet, and rows to show with them.
    Allow { perms: Vec<String>, extra: Vec<(String, String)>, then: Then },
    /// "Open a link?"
    Link(String),
    /// Accordo's token picker, without balances.
    Token,
    /// The invoice, from the server (GET /api/invoices/{id}) — no window.
    Invoice(String),
    /// Accordo's own pay window for this invoice (its record).
    Pay(String, Box<Value>),
    /// Accordo's own cheque form, filled in.
    Cheque(ChequeAsk),
}

impl Wait {
    /// Whether this needs the user (counts against MAX_WAITING); fetching doesn't.
    fn asks_user(&self) -> bool {
        !matches!(self, Wait::Invoice(_))
    }
}

/// What to do once a permission window was answered "yes".
#[derive(Debug, Clone, PartialEq)]
pub enum Then {
    Profile,
    Wallet,
    Permissions,
    Pay(String, Box<Value>),
    Cheque(ChequeAsk),
}

/// The user's answer to a `Wait`.
#[derive(Debug, Clone)]
pub enum Answer {
    /// The consent screen: the switches (None: "Deny all" / closed).
    Consent(Option<BTreeMap<String, bool>>),
    Allow(bool),
    Link(bool),
    /// The token picked (its `token_view`), None: closed.
    Token(Option<Value>),
    /// The invoice record, or why it couldn't be read.
    Invoice(Result<Value, String>),
    /// Paid: { txHash, invoiceStatus }; None: the window was closed without paying.
    Paid(Option<Value>),
    /// Written: { chequeId, amount, token, chain, txHash }; None: closed without writing it.
    Cheque(Option<Value>),
    /// The app couldn't do what the call waits for (no such token, another payment open…).
    Failed(BridgeError),
}

#[derive(Debug, Clone)]
pub enum Reply {
    Done(Result<Value, BridgeError>),
    Wait(Wait),
}

/// Something the app does for a call besides answering it.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Copy(String),
    Toast(String),
    /// An event to the module ({ accordo: 1, event, data }).
    Emit(&'static str, Value),
    OpenLink(String),
}

/// One module's side of the bridge: who it is, what its registration lists, what the user
/// granted, its session.
#[derive(Debug, Clone)]
pub struct Host {
    pub name: String,
    pub origin: String,
    /// The permissions its registration (manifest) lists.
    pub registered: Vec<String>,
    pub grants: BTreeMap<String, Grant>,
    /// Issued in the handshake.
    pub session: Option<String>,
    /// The panel: what ui.* set.
    pub title: Option<String>,
    pub ready: bool,
    pub expanded: bool,
    pub closed: bool,
    pub start_param: Option<String>,
    /// The user's pseudo-id for this module (stable per user and module).
    pub user_ref: String,
    /// Calls waiting on the user.
    pub waiting: usize,
    /// Side effects for the app to carry out (drained after every call / answer).
    pub acts: Vec<Act>,
}

impl Host {
    /// The developer tools' sandbox for user `user_id`: every permission registered, none granted.
    pub fn sandbox(user_id: i64) -> Self {
        let digest = alloy_primitives::keccak256(format!("accordo-sandbox:{user_id}"));
        Host {
            name: "Test module".into(),
            origin: "sandbox.accordo.local".into(),
            registered: all_permissions().map(str::to_string).collect(),
            grants: BTreeMap::new(),
            session: None,
            title: None,
            ready: false,
            expanded: false,
            closed: false,
            start_param: None,
            user_ref: format!("u_{}", &alloy_primitives::hex::encode(digest)[..24]),
            waiting: 0,
            acts: Vec::new(),
        }
    }

    /// "Requested by Test module · sandbox.accordo.local" — every confirmation window starts so (§9).
    pub fn asked_by(&self) -> String {
        format!("Requested by {} · {}", self.name, self.origin)
    }

    pub fn has(&self, p: &str) -> bool {
        self.grants.get(p).is_some_and(|g| g.granted)
    }

    fn declined(&self, p: &str) -> bool {
        self.grants.get(p).is_some_and(|g| g.decided && !g.granted)
    }

    fn record(&mut self, p: &str, granted: bool) {
        self.grants.insert(p.to_string(), Grant { granted, decided: true });
    }

    /// Forget every answer (Settings → Connected apps → Disconnect, or the console's reset).
    pub fn reset_grants(&mut self) {
        self.grants.clear();
        self.acts.push(Act::Emit("permissionsChanged", self.granted_map()));
    }

    /// The methods it can call (the handshake's list, `supports()`).
    pub fn methods() -> &'static [&'static str] {
        &[
            "ui.ready",
            "ui.close",
            "ui.expand",
            "ui.setTitle",
            "ui.openLink",
            "ui.copyText",
            "context.get",
            "user.getRef",
            "user.getProfile",
            "user.requestWallet",
            "permissions.get",
            "permissions.request",
            "payments.getTokens",
            "payments.selectTokenModal",
            "payments.payInvoice",
            "payments.requestCheque",
        ]
    }

    /// The context object (§7.2). The sandbox is opened from the catalog, not a chat.
    pub fn context(&self) -> Value {
        json!({
            "protocol": PROTOCOL,
            "platform": "desktop",
            "locale": "en",
            "theme": { "scheme": "dark", "accent": "#6366f1" },
            "launch": { "source": "catalog", "startParam": self.start_param, "roomRef": null, "roomRole": null },
            // The server signs it for registered apps (§10.1); the sandbox has no server side.
            "launchData": "",
        })
    }

    /// permissions.get: each registered permission, plus the old names for older apps.
    pub fn granted_map(&self) -> Value {
        let mut m = serde_json::Map::new();
        for p in &self.registered {
            m.insert(p.clone(), Value::Bool(self.has(p)));
        }
        m.insert("wallet".into(), Value::Bool(self.has("profile.wallet")));
        m.insert("profile".into(), Value::Bool(self.has("profile.nick")));
        Value::Object(m)
    }

    /// The wallet view (§6): `{ address, chain }`.
    fn wallet_view(live: &Live) -> Value {
        match &live.chain {
            None => json!({ "address": null, "chain": null }),
            Some(c) => json!({ "address": live.wallet, "chain": c.to_json() }),
        }
    }

    /// One request from the module. `session`: what it sent with it (the handshake issues one).
    pub fn call(&mut self, method: &str, params: &Value, session: Option<&str>, live: &Live) -> Reply {
        if method == "accordo_handshake" {
            return self.handshake();
        }
        if self.session.is_none() || session != self.session.as_deref() {
            return Reply::Done(Err(BridgeError::no_handshake()));
        }
        let reply = self.dispatch(method, params, live);
        if let Reply::Wait(w) = &reply {
            if w.asks_user() {
                if self.waiting >= MAX_WAITING {
                    return Reply::Done(Err(BridgeError::busy()));
                }
                self.waiting += 1;
            }
        }
        reply
    }

    /// The user answered what `wait` asked; the call goes on.
    pub fn answered(&mut self, wait: Wait, answer: Answer, live: &Live) -> Reply {
        if wait.asks_user() {
            self.waiting = self.waiting.saturating_sub(1);
        }
        let reply = self.resume(wait, answer, live);
        if let Reply::Wait(w) = &reply {
            if w.asks_user() {
                self.waiting += 1;
            }
        }
        reply
    }

    fn handshake(&mut self) -> Reply {
        // A new opening: a new session (a request with the old one gets 4900).
        self.session = Some(format!("s_{}", &alloy_primitives::hex::encode(crate::evm::client::random_b256())[..24]));
        self.closed = false;
        let open: Vec<String> = self.askable().into_iter().filter(|p| !self.grants.get(p).is_some_and(|g| g.decided)).collect();
        if open.is_empty() {
            return Reply::Done(Ok(self.handshake_result()));
        }
        // The SDK stops its "is anyone there" timer while the user reads the screen.
        self.acts.push(Act::Emit("handshake.waiting", Value::Null));
        self.waiting += 1;
        Reply::Wait(Wait::Consent(open))
    }

    fn handshake_result(&self) -> Value {
        let granted: Vec<&String> = self.registered.iter().filter(|p| self.has(p)).collect();
        json!({ "protocol": PROTOCOL, "methods": Self::methods(), "context": self.context(), "session": self.session, "granted": granted })
    }

    /// What it may ask for here: chat ones only in a chat (the sandbox isn't in one).
    fn askable(&self) -> Vec<String> {
        self.registered.iter().filter(|p| !p.starts_with("chat.")).cloned().collect()
    }

    /// One window for the permissions in `asked` it doesn't hold yet (requestPermissions in the web).
    /// Ok(None): all held, go on. `explicit`: permissions.request — the way back from a "no".
    fn need(&self, asked: &[&str], extra: Vec<(String, String)>, explicit: bool, then: Then) -> Result<Option<Wait>, BridgeError> {
        let removed: Vec<&str> = asked.iter().copied().filter(|p| legacy_permission(p).is_some_and(<[_]>::is_empty)).collect();
        if !removed.is_empty() {
            return Err(BridgeError::new(4200, format!("{} no longer exist{}: apps can't sign or send with the user's wallet — ask for money with payments.pay", removed.join(", "), if removed.len() > 1 { "" } else { "s" })));
        }
        // Old coarse names mean the finer ones; unknown names stay, to be refused.
        let mut perms: Vec<String> = Vec::new();
        for p in asked {
            let e = expand_permissions(&[*p]);
            let list: Vec<String> = if e.is_empty() || legacy_permission(p).is_none() && !all_permissions().any(|x| x == *p) { vec![p.to_string()] } else { e.iter().map(|s| s.to_string()).collect() };
            for x in list {
                if !perms.contains(&x) {
                    perms.push(x);
                }
            }
        }
        let outside: Vec<&str> = perms.iter().filter(|p| !self.registered.contains(p)).map(String::as_str).collect();
        if !outside.is_empty() {
            return Err(BridgeError::unauthorized(format!("{} isn't registered to ask for {}", self.name, outside.join(", "))));
        }
        if perms.iter().any(|p| p.starts_with("chat.")) {
            return Err(BridgeError::chat_only());
        }
        let no: Vec<&str> = perms.iter().filter(|p| self.declined(p)).map(String::as_str).collect();
        if !no.is_empty() && !explicit {
            return Err(BridgeError::unauthorized(format!("You declined {} for {}", no.join(", "), self.name)));
        }
        let missing: Vec<String> = perms.into_iter().filter(|p| !self.has(p)).collect();
        if missing.is_empty() {
            return Ok(None);
        }
        Ok(Some(Wait::Allow { perms: missing, extra, then }))
    }

    /// `need`, then `then` — right away if nothing's missing.
    fn ask_then(&mut self, asked: &[&str], extra: Vec<(String, String)>, explicit: bool, then: Then, live: &Live) -> Reply {
        match self.need(asked, extra, explicit, then.clone()) {
            Err(e) => Reply::Done(Err(e)),
            Ok(Some(w)) => Reply::Wait(w),
            Ok(None) => self.go_on(then, live),
        }
    }

    fn dispatch(&mut self, method: &str, p: &Value, live: &Live) -> Reply {
        let done = |v: Value| Reply::Done(Ok(v));
        match method {
            "ui.ready" => {
                self.ready = true;
                done(Value::Null)
            }
            "ui.close" => {
                self.closed = true;
                done(Value::Null)
            }
            "ui.expand" => {
                self.expanded = true;
                done(Value::Null)
            }
            "ui.setTitle" => {
                let t: String = p.get("text").and_then(Value::as_str).unwrap_or("").chars().take(40).collect();
                self.title = (!t.trim().is_empty()).then_some(t);
                done(Value::Null)
            }
            "ui.copyText" => {
                let t: String = p.get("text").map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default().chars().take(10_000).collect();
                self.acts.push(Act::Copy(t));
                self.acts.push(Act::Toast(format!("{} copied text", self.name)));
                done(Value::Null)
            }
            "ui.openLink" => {
                let url = p.get("url").and_then(Value::as_str).unwrap_or("").trim().to_string();
                match reqwest::Url::parse(&url) {
                    Err(_) => Reply::Done(Err(BridgeError::params("Not a URL"))),
                    Ok(u) if u.scheme() != "https" => Reply::Done(Err(BridgeError::params("Only https: links"))),
                    Ok(_) => Reply::Wait(Wait::Link(url)),
                }
            }
            "context.get" => done(self.context()),
            "user.getRef" => done(Value::String(self.user_ref.clone())),
            "user.getProfile" => {
                let parts: Vec<&str> = ["profile.nick", "profile.avatar", "profile.email"].into_iter().filter(|x| self.registered.iter().any(|r| r == x)).collect();
                if parts.is_empty() {
                    return Reply::Done(Err(BridgeError::unauthorized(format!("{} isn't registered to ask for profile.nick, profile.avatar or profile.email", self.name))));
                }
                // Ask only what's neither held nor declined; declined parts are skipped, not asked again.
                let ask: Vec<&str> = parts.iter().copied().filter(|x| !self.has(x) && !self.declined(x)).collect();
                if ask.is_empty() {
                    return self.go_on(Then::Profile, live);
                }
                let extra = match (&live.email, ask.contains(&"profile.email")) {
                    (Some(e), true) => vec![("Email".to_string(), e.clone())],
                    _ => Vec::new(),
                };
                Reply::Wait(Wait::Allow { perms: ask.into_iter().map(str::to_string).collect(), extra, then: Then::Profile })
            }
            "user.requestWallet" => {
                let extra = live.chain.as_ref().map(|c| vec![("Wallet".to_string(), format!("{} · {}", live.wallet.as_deref().map(crate::wallets::short_address).unwrap_or_else(|| "none yet".into()), c.name))]).unwrap_or_default();
                self.ask_then(&["profile.wallet"], extra, false, Then::Wallet, live)
            }
            "history.get" => {
                let limit = p.get("limit").map_or(Some(50), Value::as_i64);
                if !limit.is_some_and(|l| (1..=100).contains(&l)) {
                    return Reply::Done(Err(BridgeError::params("limit: a whole number from 1 to 100")));
                }
                // The server builds it for a registered app (no addresses, no links); the sandbox
                // isn't one.
                Reply::Done(Err(BridgeError::new(4200, "history.get needs an app registered on the server — the sandbox isn't one")))
            }
            "permissions.get" => done(self.granted_map()),
            "permissions.request" => {
                let list: Vec<String> = p.get("permissions").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                if list.is_empty() {
                    return Reply::Done(Err(BridgeError::params("permissions.request({ permissions: [...] })")));
                }
                let refs: Vec<&str> = list.iter().map(String::as_str).collect();
                self.ask_then(&refs, Vec::new(), true, Then::Permissions, live)
            }
            "payments.getTokens" => done(json!({ "chain": live.chain.as_ref().map(ChainView::to_json), "tokens": live.tokens.iter().take(200).cloned().collect::<Vec<_>>() })),
            "payments.selectTokenModal" => Reply::Wait(Wait::Token),
            "payments.payInvoice" => {
                let id = p.get("invoiceId").and_then(Value::as_str).unwrap_or("").trim().to_string();
                if !is_invoice_id(&id) {
                    return Reply::Done(Err(BridgeError::params("payments.payInvoice({ invoiceId }) — the invoice id from Accordo")));
                }
                Reply::Wait(Wait::Invoice(id))
            }
            "payments.requestCheque" => match cheque_ask(p) {
                Err(e) => Reply::Done(Err(e)),
                Ok(ask) => self.ask_then(&["payments.pay"], Vec::new(), false, Then::Cheque(ask), live),
            },
            m if m.starts_with("evm.") || m.starts_with("solana.") => Reply::Done(Err(BridgeError::new(4200, format!("{m} isn't supported: Accordo isn't a wallet provider — read with your own RPC, ask for money with payments.*")))),
            m => Reply::Done(Err(BridgeError::unsupported(m))),
        }
    }

    fn go_on(&mut self, then: Then, live: &Live) -> Reply {
        match then {
            Then::Profile => {
                let email = if self.has("profile.email") { live.email.clone() } else { None };
                Reply::Done(Ok(json!({
                    "nick": self.has("profile.nick").then(|| live.username.clone()),
                    "avatarUrl": if self.has("profile.avatar") { live.avatar.clone() } else { None },
                    "emailVerified": email.as_ref().map(|_| live.email_verified),
                    "email": email,
                })))
            }
            Then::Wallet => Reply::Done(Ok(Self::wallet_view(live))),
            Then::Permissions => Reply::Done(Ok(self.granted_map())),
            Then::Pay(id, record) => Reply::Wait(Wait::Pay(id, record)),
            Then::Cheque(ask) => Reply::Wait(Wait::Cheque(ask)),
        }
    }

    fn resume(&mut self, wait: Wait, answer: Answer, live: &Live) -> Reply {
        match (wait, answer) {
            (_, Answer::Failed(e)) => Reply::Done(Err(e)),
            (Wait::Consent(open), Answer::Consent(values)) => {
                for p in &open {
                    let on = values.as_ref().and_then(|v| v.get(p)).copied().unwrap_or(false);
                    self.record(p, on);
                }
                Reply::Done(Ok(self.handshake_result()))
            }
            (Wait::Allow { perms, then, .. }, Answer::Allow(yes)) => {
                if !yes {
                    // getProfile still answers with what was shared before — unless that's nothing.
                    if then == Then::Profile && ["profile.nick", "profile.avatar", "profile.email"].iter().any(|p| self.has(p)) {
                        return self.go_on(Then::Profile, live);
                    }
                    return Reply::Done(Err(BridgeError::rejected()));
                }
                for p in &perms {
                    self.record(p, true);
                }
                self.acts.push(Act::Emit("permissionsChanged", self.granted_map()));
                self.go_on(then, live)
            }
            (Wait::Link(url), Answer::Link(yes)) => {
                if !yes {
                    return Reply::Done(Err(BridgeError::rejected()));
                }
                self.acts.push(Act::OpenLink(url));
                Reply::Done(Ok(Value::Bool(true)))
            }
            (Wait::Token, Answer::Token(t)) => match t {
                Some(v) => Reply::Done(Ok(v)),
                None => Reply::Done(Err(BridgeError::rejected())),
            },
            (Wait::Invoice(id), Answer::Invoice(r)) => {
                let Ok(inv) = r else { return Reply::Done(Err(BridgeError::no_invoice())) };
                let inv = inv.get("data").cloned().unwrap_or(inv);
                if inv.get("invoice_id").or_else(|| inv.pointer("/detail/invoice_id")).is_none() {
                    return Reply::Done(Err(BridgeError::no_invoice()));
                }
                let status = inv.get("status").or_else(|| inv.pointer("/detail/status")).and_then(Value::as_str).unwrap_or("").to_uppercase();
                if status == "PAID" || status == "CANCELLED" {
                    return Reply::Done(Err(BridgeError::closed(&status)));
                }
                self.ask_then(&["payments.pay"], Vec::new(), false, Then::Pay(id, Box::new(inv)), live)
            }
            (Wait::Pay(id, _), Answer::Paid(r)) => match r {
                Some(v) => Reply::Done(Ok(json!({ "status": "paid", "invoiceId": id, "invoiceStatus": v.get("invoiceStatus").cloned().unwrap_or(json!("paid")), "txHash": v.get("txHash") }))),
                None => Reply::Done(Err(BridgeError::rejected())),
            },
            (Wait::Cheque(_), Answer::Cheque(r)) => match r {
                Some(mut v) => {
                    // Only ids and a status — never the link or the claim key.
                    if let Some(o) = v.as_object_mut() {
                        o.retain(|k, _| matches!(k.as_str(), "chequeId" | "amount" | "token" | "chain" | "txHash"));
                        o.insert("status".into(), json!("created"));
                    }
                    Reply::Done(Ok(v))
                }
                None => Reply::Done(Err(BridgeError::rejected())),
            },
            (w, a) => Reply::Done(Err(BridgeError::internal(format!("unexpected answer {a:?} to {w:?}")))),
        }
    }
}

/// EVM bytes32, or a Solana account (base58, 32–44).
fn is_invoice_id(s: &str) -> bool {
    let evm = s.len() == 66 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit());
    let b58 = (32..=44).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() && !"0OIl".contains(c));
    evm || b58
}

/// payments.requestCheque's params, checked like the web (payments.ts).
fn cheque_ask(p: &Value) -> Result<ChequeAsk, BridgeError> {
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(|v| v.trim().to_string());
    let amount = s("amount").unwrap_or_default();
    let ok_amount = {
        let (w, f) = amount.split_once('.').unwrap_or((&amount, ""));
        !w.is_empty() && w.len() <= 18 && w.chars().all(|c| c.is_ascii_digit()) && (w == "0" || !w.starts_with('0')) && f.len() <= 18 && f.chars().all(|c| c.is_ascii_digit()) && (!amount.contains('.') || !f.is_empty())
    };
    if !ok_amount || amount.parse::<f64>().map_or(true, |a| a <= 0.0) {
        return Err(BridgeError::params("amount: a positive number as a string, in token units (\"5\", \"0.25\")"));
    }
    let token = s("token").unwrap_or_default();
    if token.is_empty() || token.len() > 64 {
        return Err(BridgeError::params("token: a symbol, a token address (e.g. from payments.selectTokenModal / getTokens), or 'native'"));
    }
    let note = match p.get("note") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(n)) if n.trim().chars().count() <= 200 => n.trim().to_string(),
        _ => return Err(BridgeError::params("note: text, up to 200 characters")),
    };
    let to = match p.get("to") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => {
            let t = t.trim();
            let nick = t.strip_prefix('@').filter(|n| (2..=32).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'));
            if nick.is_none() {
                return Err(BridgeError::params("to: an Accordo nickname like '@alice' — never an address"));
            }
            Some(t.to_string())
        }
        _ => return Err(BridgeError::params("to: an Accordo nickname like '@alice' — never an address")),
    };
    Ok(ChequeAsk { amount, token, note, to })
}

/// The API described in a file (assets/module-api.json, like a Postman / Hoppscotch collection):
/// each method's summary, permission, whether it opens a window, its params with their types and
/// examples, what it returns and the errors it can give. The console builds its list and its
/// params table from it; a test keeps it in step with what the host answers.
pub mod spec {
    use std::collections::BTreeMap;
    use std::sync::LazyLock;

    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Debug, Clone, Deserialize)]
    pub struct Spec {
        /// Error code → what it means.
        pub errors: BTreeMap<String, String>,
        pub methods: Vec<MethodSpec>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct MethodSpec {
        pub method: String,
        pub group: String,
        pub summary: String,
        pub permission: Option<String>,
        /// It may open a window (the user answers).
        pub window: bool,
        pub params: Vec<ParamSpec>,
        pub returns: Value,
        pub errors: Vec<i32>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ParamSpec {
        pub name: String,
        /// string, integer, number, boolean, string[], object
        #[serde(rename = "type")]
        pub kind: String,
        pub required: bool,
        pub description: String,
        pub example: Value,
    }

    static SPEC: LazyLock<Spec> = LazyLock::new(|| serde_json::from_str(include_str!("../assets/module-api.json")).expect("assets/module-api.json is valid"));

    pub fn get() -> &'static Spec {
        &SPEC
    }

    pub fn find(method: &str) -> Option<&'static MethodSpec> {
        SPEC.methods.iter().find(|m| m.method == method)
    }

    /// A value as the console's field shows it: strings bare, lists comma-separated, the rest JSON.
    pub fn field_text(v: &Value, kind: &str) -> String {
        match (v, kind) {
            (Value::Null, _) => String::new(),
            (Value::String(s), _) => s.clone(),
            (Value::Array(a), "string[]") => a.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", "),
            (v, _) => v.to_string(),
        }
    }

    /// The fields → the params object, by each param's type. Empty optional fields are left out.
    pub fn build(params: &[ParamSpec], values: &BTreeMap<String, String>) -> Result<Value, String> {
        let mut out = serde_json::Map::new();
        for p in params {
            let raw = values.get(&p.name).map(|v| v.trim()).unwrap_or("");
            if raw.is_empty() {
                if p.required {
                    return Err(format!("{} is required", p.name));
                }
                continue;
            }
            let v = match p.kind.as_str() {
                "string" => Value::String(raw.to_string()),
                "integer" => raw.parse::<i64>().map(Value::from).map_err(|_| format!("{}: a whole number", p.name))?,
                "number" => raw.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).ok_or(format!("{}: a number", p.name))?,
                "boolean" => match raw {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return Err(format!("{}: true or false", p.name)),
                },
                "string[]" => Value::Array(raw.split(',').map(str::trim).filter(|x| !x.is_empty()).map(|x| Value::String(x.to_string())).collect()),
                _ => serde_json::from_str(raw).map_err(|e| format!("{}: not JSON ({e})", p.name))?,
            };
            out.insert(p.name.clone(), v);
        }
        Ok(Value::Object(out))
    }

    /// The example params object of a method.
    pub fn example(method: &str) -> Value {
        let mut out = serde_json::Map::new();
        for p in find(method).map(|m| m.params.as_slice()).unwrap_or_default() {
            out.insert(p.name.clone(), p.example.clone());
        }
        Value::Object(out)
    }

    /// The example values, as the fields show them.
    pub fn example_fields(method: &str) -> BTreeMap<String, String> {
        find(method).map(|m| m.params.iter().map(|p| (p.name.clone(), field_text(&p.example, &p.kind))).collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live() -> Live {
        Live {
            username: "dazay".into(),
            email: Some("d@example.com".into()),
            email_verified: true,
            chain: Some(ChainView { id: "eip155:8453".into(), name: "Base".into(), testnet: false }),
            wallet: Some("0x70997970C51812dc3A010C7d01b50e0d17dc79C8".into()),
            ..Default::default()
        }
    }

    /// Handshake, everything allowed on the consent screen; the session.
    fn opened(host: &mut Host, allow: bool) -> String {
        let l = live();
        let Reply::Wait(w @ Wait::Consent(_)) = host.call("accordo_handshake", &json!({}), None, &l) else { panic!("consent first") };
        let values = match &w {
            Wait::Consent(open) => open.iter().map(|p| (p.clone(), allow)).collect(),
            _ => unreachable!(),
        };
        let Reply::Done(Ok(r)) = host.answered(w, Answer::Consent(Some(values)), &l) else { panic!("handshake answers") };
        assert_eq!(r["protocol"], 1);
        r["session"].as_str().unwrap().to_string()
    }

    #[test]
    fn nothing_before_the_handshake() {
        let mut h = Host::sandbox(1);
        let Reply::Done(Err(e)) = h.call("context.get", &json!({}), None, &live()) else { panic!() };
        assert_eq!(e.code, 4900);
        let s = opened(&mut h, false);
        let Reply::Done(Err(e)) = h.call("context.get", &json!({}), Some("s_wrong"), &live()) else { panic!() };
        assert_eq!(e.code, 4900, "another session's requests are refused");
        let Reply::Done(Ok(c)) = h.call("context.get", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(c["platform"], "desktop");
        assert_eq!(c["launch"]["source"], "catalog");
    }

    #[test]
    fn the_consent_screen_skips_chat_and_isnt_shown_twice() {
        let mut h = Host::sandbox(1);
        let Reply::Wait(Wait::Consent(open)) = h.call("accordo_handshake", &json!({}), None, &live()) else { panic!() };
        assert!(!open.iter().any(|p| p.starts_with("chat.")), "outside a chat, chat permissions aren't asked");
        let w = Wait::Consent(open);
        h.answered(w, Answer::Consent(None), &live()); // Deny all — remembered
        assert!(matches!(h.call("accordo_handshake", &json!({}), None, &live()), Reply::Done(Ok(_))));
    }

    #[test]
    fn a_declined_permission_answers_4100_until_asked_explicitly() {
        let mut h = Host::sandbox(1);
        let s = opened(&mut h, false);
        let Reply::Done(Err(e)) = h.call("user.requestWallet", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 4100);
        // permissions.request is the way back from a "no": it shows the window.
        let Reply::Wait(w) = h.call("permissions.request", &json!({"permissions": ["wallet"]}), Some(&s), &live()) else { panic!() };
        assert!(matches!(&w, Wait::Allow { perms, .. } if perms == &vec!["profile.wallet".to_string()]), "old names mean the new ones");
        let Reply::Done(Ok(m)) = h.answered(w, Answer::Allow(true), &live()) else { panic!() };
        assert_eq!(m["profile.wallet"], true);
        assert_eq!(m["wallet"], true);
        let Reply::Done(Ok(v)) = h.call("user.requestWallet", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(v["chain"]["id"], "eip155:8453");
        assert!(h.acts.iter().any(|a| matches!(a, Act::Emit("permissionsChanged", _))));
    }

    #[test]
    fn the_profile_has_only_what_was_shared() {
        let mut h = Host::sandbox(1);
        let s = opened(&mut h, true);
        // Take the email back: it comes back null.
        h.grants.insert("profile.email".into(), Grant { granted: false, decided: true });
        let Reply::Done(Ok(p)) = h.call("user.getProfile", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(p["nick"], "dazay");
        assert_eq!(p["email"], Value::Null);
        assert_eq!(p["emailVerified"], Value::Null);
    }

    #[test]
    fn removed_and_unregistered_permissions() {
        let mut h = Host::sandbox(1);
        h.registered.retain(|p| p != "payments.pay");
        let s = opened(&mut h, true);
        let Reply::Done(Err(e)) = h.call("permissions.request", &json!({"permissions": ["wallet.sign"]}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 4200);
        let Reply::Done(Err(e)) = h.call("payments.requestCheque", &json!({"amount": "1", "token": "native"}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 4100, "payments.pay isn't in its registration");
        let Reply::Done(Err(e)) = h.call("permissions.request", &json!({"permissions": ["chat.send"]}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 5004);
        let Reply::Done(Err(e)) = h.call("evm.sendTransaction", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 4200);
    }

    #[test]
    fn cheque_params_are_checked_like_the_web() {
        assert!(cheque_ask(&json!({"amount": "5", "token": "USDC", "to": "@alice"})).is_ok());
        for bad in [json!({"amount": "05", "token": "USDC"}), json!({"amount": "0", "token": "USDC"}), json!({"amount": "1.", "token": "x"}), json!({"amount": "1", "token": ""}), json!({"amount": "1", "token": "x", "to": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"})] {
            assert_eq!(cheque_ask(&bad).unwrap_err().code, -32602, "{bad}");
        }
    }

    #[test]
    fn a_cheque_answer_never_carries_its_link() {
        let mut h = Host::sandbox(1);
        let s = opened(&mut h, true);
        let Reply::Wait(w @ Wait::Cheque(_)) = h.call("payments.requestCheque", &json!({"amount": "1", "token": "native"}), Some(&s), &live()) else { panic!() };
        let Reply::Done(Ok(v)) = h.answered(w, Answer::Cheque(Some(json!({"chequeId": "0x1", "txHash": "0x2", "secret": "0xdead", "link": "https://…"}))), &live()) else { panic!() };
        assert_eq!(v["status"], "created");
        assert!(v.get("secret").is_none() && v.get("link").is_none());
    }

    #[test]
    fn invoices_closed_or_missing() {
        let mut h = Host::sandbox(1);
        let s = opened(&mut h, true);
        let id = format!("0x{}", "ab".repeat(32));
        let Reply::Wait(w) = h.call("payments.payInvoice", &json!({"invoiceId": id}), Some(&s), &live()) else { panic!() };
        let Reply::Done(Err(e)) = h.answered(w.clone(), Answer::Invoice(Ok(json!({"invoice_id": id, "status": "PAID"}))), &live()) else { panic!() };
        assert_eq!(e.code, 5007);
        let Reply::Done(Err(e)) = h.answered(w.clone(), Answer::Invoice(Err("404".into())), &live()) else { panic!() };
        assert_eq!(e.code, 5006);
        let Reply::Wait(Wait::Pay(..)) = h.answered(w, Answer::Invoice(Ok(json!({"invoice_id": id, "status": "PENDING"}))), &live()) else { panic!("payments.pay is held: straight to the pay window") };
    }

    #[test]
    fn at_most_three_windows_wait() {
        let mut h = Host::sandbox(1);
        let s = opened(&mut h, true);
        for _ in 0..MAX_WAITING {
            assert!(matches!(h.call("payments.selectTokenModal", &json!({}), Some(&s), &live()), Reply::Wait(_)));
        }
        let Reply::Done(Err(e)) = h.call("payments.selectTokenModal", &json!({}), Some(&s), &live()) else { panic!() };
        assert_eq!(e.code, 5005);
    }

    #[test]
    fn the_spec_file_and_the_host_agree() {
        let listed: Vec<&str> = spec::get().methods.iter().map(|m| m.method.as_str()).collect();
        for m in Host::methods() {
            assert!(listed.contains(m), "{m} is answered but not in assets/module-api.json");
        }
        for m in &listed {
            assert!(*m == "accordo_handshake" || *m == "history.get" || Host::methods().contains(m), "{m} is in the spec but the host doesn't answer it");
        }
        // Every error a method lists is described.
        for m in &spec::get().methods {
            for e in &m.errors {
                assert!(spec::get().errors.contains_key(&e.to_string()), "{}: error {e} isn't described", m.method);
            }
        }
    }

    #[test]
    fn fields_become_typed_params() {
        let m = spec::find("payments.requestCheque").unwrap();
        let mut v = spec::example_fields("payments.requestCheque");
        v.insert("note".into(), String::new()); // optional and empty: left out
        let p = spec::build(&m.params, &v).unwrap();
        assert_eq!(p, json!({"amount": "1", "token": "native", "to": "@alice"}));
        v.insert("amount".into(), String::new());
        assert!(spec::build(&m.params, &v).unwrap_err().contains("amount"));
        let r = spec::find("permissions.request").unwrap();
        let p = spec::build(&r.params, &spec::example_fields("permissions.request")).unwrap();
        assert_eq!(p, json!({"permissions": ["profile.nick", "profile.wallet"]}));
        let h = spec::find("history.get").unwrap();
        let bad = std::collections::BTreeMap::from([("limit".to_string(), "lots".to_string())]);
        assert!(spec::build(&h.params, &bad).is_err());
    }
}
