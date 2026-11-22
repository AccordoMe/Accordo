// SPDX-License-Identifier: GPL-3.0-or-later
//! The module API console (Developer tools → API): calls the module protocol the way a module
//! would — as the sandbox "Test module" — and shows what Accordo answers. The windows are the
//! real ones a module's user would see (consent, permissions, links, the token picker, the pay
//! and cheque windows), opened in the main window.
//!
//! The host's logic is in module_host.rs; this is the app side: the queue of requests waiting on
//! the user (one window at a time, §13), the live data a module may read, and the log.

use std::collections::VecDeque;

use iced::widget::text_editor;
use iced::Task;
use serde_json::{json, Value};

use super::{App, Message};
use crate::components::module_window::{self, ModuleWinMsg};
use crate::components::toast::Kind;
use crate::module_host::{spec, token_view, Act, Answer, ChainView, Host, Live, Reply, Wait};

#[derive(Debug, Clone)]
pub enum ModMsg {
    /// A method picked in the list: its example params into the editor.
    Pick(String),
    Params(text_editor::Action),
    /// A field of the params table (by the param's name).
    Field(String, String),
    /// The params as a table (false) or raw JSON (true).
    Raw(bool),
    Call,
    /// The sandbox's registration lists this permission (or not).
    Register(String, bool),
    ResetGrants,
    ClearLog,
    /// Reject the request at the front of the queue (a window left open somewhere).
    CancelFront,
    Window(ModuleWinMsg),
    /// The invoice a payments.payInvoice names, from the server.
    Invoice(u64, Result<Value, String>),
}

/// One line of the log.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: u64,
    /// "→ method" for calls, the event's name for events.
    pub method: String,
    pub params: String,
    /// None: waiting. Ok(json) / Err((code, message)).
    pub result: Option<Result<String, (i32, String)>>,
    pub event: bool,
    pub started: std::time::Instant,
    pub millis: Option<u128>,
}

#[derive(Debug)]
pub struct Console {
    /// The sandbox module of the signed-in account (made at sign-in).
    pub host: Option<Host>,
    pub method: String,
    /// The params table's fields, by param name (assets/module-api.json describes them).
    pub fields: std::collections::BTreeMap<String, String>,
    /// Raw JSON instead of the table.
    pub raw: bool,
    pub params: text_editor::Content,
    pub log: Vec<LogEntry>,
    /// Requests waiting on the user, oldest first; the front one's window is open.
    pub waits: VecDeque<(u64, Wait)>,
    pub window: module_window::State,
    next_id: u64,
    /// The front request runs Accordo's pay / cheque window (answered when it's done or closed).
    pub in_tx: bool,
    /// What the cheque window was opened with (for the answer): amount, token, network.
    pub cheque_info: Option<(String, String, String)>,
}

impl Default for Console {
    fn default() -> Self {
        Console {
            host: None,
            method: "accordo_handshake".into(),
            fields: spec::example_fields("accordo_handshake"),
            raw: false,
            params: text_editor::Content::with_text(&pretty(&spec::example("accordo_handshake"))),
            log: Vec::new(),
            waits: VecDeque::new(),
            window: Default::default(),
            next_id: 1,
            in_tx: false,
            cheque_info: None,
        }
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// The log keeps the last 200 lines; results are cut at 8 KB.
const LOG_MAX: usize = 200;
const RESULT_MAX: usize = 8 * 1024;

impl Console {
    /// The front request's window, if it takes one of module_window's.
    pub fn front(&self) -> Option<&Wait> {
        self.waits.front().map(|(_, w)| w)
    }
}

impl App {
    /// A new account: a fresh sandbox (its grants were the previous account's).
    pub(super) fn reset_module_console(&mut self, user_id: Option<i64>) {
        self.module_console = Console { host: user_id.map(Host::sandbox), ..Default::default() };
    }

    /// What a module may read right now (read fresh on every call).
    fn module_live(&self) -> Live {
        let Some(s) = self.session.as_ref() else { return Live::default() };
        let chain = self.selected_chain().cloned();
        let view = chain.as_ref().and_then(|c| {
            let id = if c.solana {
                if c.id == "solana-local" { "solana:localnet".to_string() } else if c.testnet { "solana:devnet".into() } else { "solana:mainnet".into() }
            } else {
                format!("eip155:{}", c.evm_id()?)
            };
            Some(ChainView { id, name: c.name.clone(), testnet: c.testnet })
        });
        let family = if chain.as_ref().is_some_and(|c| c.solana) { "Solana" } else { "EVM" };
        let fits = |a: &str| crate::wallets::chain_of(a) == family;
        let wallet = self.active_wallet().map(|w| w.address.clone()).filter(|a| fits(a)).or_else(|| self.store.wallets.iter().map(|w| w.address.clone()).find(|a| fits(a)));
        let mut tokens: Vec<Value> = Vec::new();
        if let Some(c) = &chain {
            let listed = self.tokens.get(&c.id).cloned().unwrap_or_default();
            for t in listed.iter().chain(self.custom_tokens(&c.id).iter()) {
                tokens.push(token_view(&t.symbol, &t.name, &t.address, t.decimals, t.logo.as_deref(), view.as_ref()));
            }
        }
        Live {
            username: s.user.username.clone(),
            avatar: s.user.avatar.clone(),
            email: s.user.email.clone().filter(|e| !e.is_empty()),
            email_verified: !s.user.email_verification_required,
            chain: view,
            wallet,
            tokens,
        }
    }

    pub(super) fn module_update(&mut self, m: ModMsg) -> Task<Message> {
        let c = &mut self.module_console;
        match m {
            ModMsg::Pick(method) => {
                c.params = text_editor::Content::with_text(&pretty(&spec::example(&method)));
                c.fields = spec::example_fields(&method);
                c.method = method;
            }
            ModMsg::Params(a) => c.params.perform(a),
            ModMsg::Field(name, v) => {
                c.fields.insert(name, v);
            }
            ModMsg::Raw(raw) => {
                let params = spec::find(&c.method).map(|m| m.params.as_slice()).unwrap_or_default();
                if raw && !c.raw {
                    // Table → JSON: what the fields make (as typed, even if not valid yet).
                    let v = spec::build(params, &c.fields).unwrap_or_else(|_| {
                        Value::Object(c.fields.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect())
                    });
                    c.params = text_editor::Content::with_text(&pretty(&v));
                } else if !raw && c.raw {
                    // JSON → table: the known params' values; anything else stays in the JSON only.
                    match serde_json::from_str::<Value>(&c.params.text()) {
                        Ok(v) => {
                            for p in params {
                                c.fields.insert(p.name.clone(), spec::field_text(v.get(&p.name).unwrap_or(&Value::Null), &p.kind));
                            }
                        }
                        Err(e) => return self.toasts.show(Kind::Error, format!("The JSON doesn't parse: {e}")),
                    }
                }
                c.raw = raw;
            }
            ModMsg::Register(p, on) => {
                if let Some(h) = c.host.as_mut() {
                    h.registered.retain(|x| x != &p);
                    if on {
                        h.registered.push(p);
                        // Keep the registry's order.
                        let order: Vec<&str> = crate::api::mini_apps::all_permissions().collect();
                        h.registered.sort_by_key(|x| order.iter().position(|o| o == x).unwrap_or(usize::MAX));
                    }
                }
            }
            ModMsg::ResetGrants => {
                if let Some(h) = c.host.as_mut() {
                    h.reset_grants();
                }
                return self.module_acts();
            }
            ModMsg::ClearLog => c.log.clear(),
            ModMsg::CancelFront => {
                if c.in_tx && (self.tx.is_some() || self.tx_preparing) {
                    return self.toasts.show(Kind::Info, "Close the transaction window to cancel it");
                }
                let answer = match c.front() {
                    Some(Wait::Consent(_)) => Answer::Consent(None),
                    Some(Wait::Allow { .. }) => Answer::Allow(false),
                    Some(Wait::Link(_)) => Answer::Link(false),
                    Some(Wait::Token) => {
                        self.token_picker = None;
                        Answer::Token(None)
                    }
                    Some(Wait::Pay(..)) => Answer::Paid(None),
                    Some(Wait::Cheque(_)) => Answer::Cheque(None),
                    Some(Wait::Invoice(_)) | None => return Task::none(),
                };
                return self.module_answer(answer);
            }
            ModMsg::Call => return self.module_call(),
            ModMsg::Window(w) => {
                match w {
                    ModuleWinMsg::Toggle(p, on) => {
                        c.window.toggles.insert(p, on);
                    }
                    ModuleWinMsg::Confirm | ModuleWinMsg::Cancel => {
                        let yes = matches!(w, ModuleWinMsg::Confirm);
                        let answer = match c.front() {
                            Some(Wait::Consent(_)) => Answer::Consent(yes.then(|| c.window.toggles.clone())),
                            Some(Wait::Allow { .. }) => Answer::Allow(yes),
                            Some(Wait::Link(_)) => Answer::Link(yes),
                            _ => return Task::none(),
                        };
                        return self.module_answer(answer);
                    }
                }
            }
            ModMsg::Invoice(id, r) => {
                // Not a window: answered where it is in the queue.
                let Some(pos) = c.waits.iter().position(|(i, w)| *i == id && matches!(w, Wait::Invoice(_))) else { return Task::none() };
                let (_, wait) = c.waits.remove(pos).expect("found");
                let live = self.module_live();
                let Some(h) = self.module_console.host.as_mut() else { return Task::none() };
                let reply = h.answered(wait, Answer::Invoice(r), &live);
                return self.module_reply(id, reply, pos == 0);
            }
        }
        Task::none()
    }

    fn module_call(&mut self) -> Task<Message> {
        let c = &mut self.module_console;
        let params: Value = if c.raw || spec::find(&c.method).is_none() {
            let text = c.params.text();
            if text.trim().is_empty() {
                json!({})
            } else {
                match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => return self.toasts.show(Kind::Error, format!("Params aren't JSON: {e}")),
                }
            }
        } else {
            match spec::build(&spec::find(&c.method).expect("checked").params, &c.fields) {
                Ok(v) => v,
                Err(e) => return self.toasts.show(Kind::Error, e),
            }
        };
        let id = c.next_id;
        c.next_id += 1;
        let method = c.method.clone();
        let live = self.module_live();
        let c = &mut self.module_console;
        let Some(h) = c.host.as_mut() else { return Task::none() };
        // The console is the module: it sends the session the handshake gave it.
        let session = h.session.clone();
        c.log.push(LogEntry { id, method: method.clone(), params: serde_json::to_string(&params).unwrap_or_default(), result: None, event: false, started: std::time::Instant::now(), millis: None });
        if c.log.len() > LOG_MAX {
            c.log.remove(0);
        }
        let reply = h.call(&method, &params, session.as_deref(), &live);
        self.module_reply(id, reply, false)
    }

    /// A call's reply: logged when done; queued when it waits on the user (or fetched, for an
    /// invoice). `was_front`: it was at the front — the next request's window comes up.
    fn module_reply(&mut self, id: u64, reply: Reply, was_front: bool) -> Task<Message> {
        let mut tasks = vec![self.module_acts()];
        match reply {
            Reply::Done(r) => {
                if let Some(e) = self.module_console.log.iter_mut().rev().find(|e| e.id == id && !e.event) {
                    e.millis = Some(e.started.elapsed().as_millis());
                    e.result = Some(match r {
                        Ok(v) => {
                            let mut s = pretty(&v);
                            if s.len() > RESULT_MAX {
                                s.truncate(s.floor_char_boundary(RESULT_MAX));
                                s.push_str("\n… (cut)");
                            }
                            Ok(s)
                        }
                        Err(e) => Err((e.code, e.message)),
                    });
                }
                if was_front {
                    tasks.push(self.module_front());
                }
            }
            Reply::Wait(Wait::Invoice(inv)) => {
                let at_front = was_front || self.module_console.waits.is_empty();
                // Fetching isn't a window: it waits at the same place in the queue.
                if at_front {
                    self.module_console.waits.push_front((id, Wait::Invoice(inv.clone())));
                } else {
                    self.module_console.waits.push_back((id, Wait::Invoice(inv.clone())));
                }
                if let Some(api) = self.session.as_ref().map(|s| s.api()) {
                    tasks.push(Task::perform(crate::api::invoices::get(api, inv), move |r| Message::Module(ModMsg::Invoice(id, r.map_err(|e| e.message)))));
                }
            }
            Reply::Wait(w) => {
                let c = &mut self.module_console;
                if was_front {
                    c.waits.push_front((id, w));
                    tasks.push(self.module_front());
                } else {
                    let first = c.waits.is_empty();
                    c.waits.push_back((id, w));
                    if first {
                        tasks.push(self.module_front());
                    }
                }
            }
        }
        Task::batch(tasks)
    }

    /// The user answered the front request.
    fn module_answer(&mut self, answer: Answer) -> Task<Message> {
        let Some((id, wait)) = self.module_console.waits.pop_front() else { return Task::none() };
        self.module_console.in_tx = false;
        let live = self.module_live();
        let Some(h) = self.module_console.host.as_mut() else { return Task::none() };
        let reply = h.answered(wait, answer, &live);
        self.module_reply(id, reply, true)
    }

    /// Opens what the front request waits for: its window, the token picker, the pay window or
    /// the cheque form.
    fn module_front(&mut self) -> Task<Message> {
        let Some(w) = self.module_console.front().cloned() else { return Task::none() };
        let main = self.main_window;
        let focus = main.map(iced::window::gain_focus).unwrap_or_else(Task::none);
        match w {
            Wait::Consent(_) | Wait::Allow { .. } | Wait::Link(_) => {
                self.module_console.window = module_window::State::for_wait(&w);
                focus
            }
            Wait::Token => {
                use crate::components::token_picker::{Purpose, TokenPickerMsg};
                Task::batch([focus, self.token_picker_update(TokenPickerMsg::Open(Purpose::Module))])
            }
            Wait::Invoice(_) => Task::none(),
            Wait::Pay(_, record) => {
                if let Err(t) = self.can_start(true) {
                    return Task::batch([t, self.module_fail("The user can't pay right now (sign-in or another payment open)")]);
                }
                self.module_console.in_tx = true;
                let start = self.start_pay(*record);
                if !self.tx_preparing && self.tx.is_none() {
                    // Refused on the spot (its toast says why): the module isn't left waiting.
                    return Task::batch([start, self.module_fail("This invoice can't be paid here")]);
                }
                Task::batch([focus, start])
            }
            Wait::Cheque(ask) => {
                if self.tx.is_some() || self.tx_preparing {
                    return self.module_fail("Another payment window is open");
                }
                // The form, filled in: the user can change anything before signing (§7.4.1).
                let chain_id = self.chain_id.clone();
                let listed = self.tokens.get(&chain_id).cloned().unwrap_or_default();
                let custom = self.custom_tokens(&chain_id);
                let want = ask.token.trim().to_lowercase();
                let native = |a: &str| a.trim().is_empty() || a.eq_ignore_ascii_case("native") || a.trim_start_matches("0x").chars().all(|c| c == '0');
                let token = listed.iter().chain(custom.iter()).find(|t| if want == "native" { native(&t.address) } else { t.address.to_lowercase() == want || t.symbol.to_lowercase() == want }).cloned();
                let Some(token) = token else {
                    let e = crate::module_host::BridgeError { code: 5002, message: format!("{} isn't a token on this network", ask.token) };
                    return self.module_answer(Answer::Failed(e));
                };
                let st = &mut self.cheques;
                *st = crate::pages::cheques_page::State { token: Some(token), amount: ask.amount.clone(), note: ask.note.clone(), fee_bps: st.fee_bps, expiry: st.expiry, ..Default::default() };
                if let Some(to) = &ask.to {
                    st.recipients[0].name = to.clone();
                }
                self.module_console.in_tx = true;
                let name = self.module_console.host.as_ref().map(|h| h.name.clone()).unwrap_or_default();
                Task::batch([
                    focus,
                    Task::done(Message::Navigate(super::Nav::Cheques)),
                    self.toasts.show(Kind::Info, format!("{name} asks you to write a cheque — check it, then Write cheque")),
                ])
            }
        }
    }

    /// The front request can't go on: the module gets 5000 with why.
    fn module_fail(&mut self, why: &str) -> Task<Message> {
        self.module_answer(Answer::Failed(crate::module_host::BridgeError::internal(why)))
    }

    /// The host's side effects: clipboard, toasts, links, events (logged).
    fn module_acts(&mut self) -> Task<Message> {
        let acts = self.module_console.host.as_mut().map(|h| std::mem::take(&mut h.acts)).unwrap_or_default();
        let mut tasks = Vec::new();
        for a in acts {
            match a {
                Act::Copy(t) => tasks.push(iced::clipboard::write(t)),
                Act::Toast(t) => tasks.push(self.toasts.show(Kind::Info, t)),
                Act::OpenLink(url) => {
                    if open::that_detached(&url).is_err() {
                        tasks.push(self.toasts.show(Kind::Info, format!("Open {url} in your browser")));
                    }
                }
                Act::Emit(event, data) => {
                    let c = &mut self.module_console;
                    let id = c.next_id;
                    c.next_id += 1;
                    c.log.push(LogEntry { id, method: event.to_string(), params: String::new(), result: Some(Ok(pretty(&data))), event: true, started: std::time::Instant::now(), millis: None });
                }
            }
        }
        Task::batch(tasks)
    }

    // ── hooks from the payment flows ──

    /// A pay / cheque window opened for the module: it says who asked.
    pub(super) fn module_mark_tx(&mut self) {
        let c = &self.module_console;
        if !c.in_tx || !matches!(c.front(), Some(Wait::Pay(..) | Wait::Cheque(_))) {
            return;
        }
        let by = c.host.as_ref().map(|h| format!("{} · {}", h.name, h.origin)).unwrap_or_default();
        if let Some(tx) = self.tx.as_mut() {
            tx.review.items.insert(0, crate::components::tx_review::Item::strong("Requested by", by));
        }
    }

    /// The cheque review is about to open: remember what for the answer.
    pub(super) fn module_cheque_opening(&mut self, amount: &str, symbol: &str, chain: &str) {
        if self.module_console.in_tx && matches!(self.module_console.front(), Some(Wait::Cheque(_))) {
            self.module_console.cheque_info = Some((amount.to_string(), symbol.to_string(), chain.to_string()));
        }
    }

    /// The review couldn't be prepared.
    pub(super) fn module_tx_failed(&mut self, why: &str) -> Task<Message> {
        if !self.module_console.in_tx || !matches!(self.module_console.front(), Some(Wait::Pay(..))) {
            return Task::none();
        }
        self.module_fail(why)
    }

    /// The payment went through.
    pub(super) fn module_tx_done(&mut self, kind: Option<crate::flows::Kind>, f: &crate::components::tx_review::Finished) -> Task<Message> {
        use crate::flows::Kind as K;
        let c = &mut self.module_console;
        if !c.in_tx {
            return Task::none();
        }
        let hash = format!("{:#x}", f.hash);
        let answer = match (c.front(), kind) {
            (Some(Wait::Pay(..)), Some(K::PayInvoice)) => Answer::Paid(Some(json!({ "txHash": hash, "invoiceStatus": "paid" }))),
            (Some(Wait::Cheque(_)), Some(K::Cheque)) => {
                let (amount, token, chain) = c.cheque_info.take().unwrap_or_default();
                Answer::Cheque(Some(json!({ "chequeId": f.id, "amount": amount, "token": token, "chain": chain, "txHash": hash })))
            }
            _ => return Task::none(),
        };
        self.module_answer(answer)
    }

    /// The window was closed without the payment going through.
    pub(super) fn module_tx_closed(&mut self) -> Task<Message> {
        let c = &self.module_console;
        if !c.in_tx {
            return Task::none();
        }
        let answer = match c.front() {
            Some(Wait::Pay(..)) => Answer::Paid(None),
            Some(Wait::Cheque(_)) => Answer::Cheque(None),
            _ => return Task::none(),
        };
        self.module_answer(answer)
    }

    /// The token picker, opened for the module: what was picked (None: closed).
    pub(super) fn module_token(&mut self, t: Option<&crate::api::data::Token>) -> Task<Message> {
        if !matches!(self.module_console.front(), Some(Wait::Token)) {
            return Task::none();
        }
        let live = self.module_live();
        let v = t.map(|t| token_view(&t.symbol, &t.name, &t.address, t.decimals, t.logo.as_deref(), live.chain.as_ref()));
        self.module_answer(Answer::Token(v))
    }
}
