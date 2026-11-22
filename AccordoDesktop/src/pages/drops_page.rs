// SPDX-License-Identifier: GPL-3.0-or-later
//! Drops (DropsPage.tsx, CreateDropCard.tsx): airdrops and mass payouts — one deposit, a list of
//! wallets, each claims its own amount, at once or in parts over time (payout plans).
//!
//! On the left the new drop (name, token, how it pays out, the wallets — typed, pasted or from a
//! CSV / JSON file —, until when, the fee, a note), on the right the payout plans; below, your
//! drops and one drop's wallets, with "check the chain" and "take back the rest".

use iced::widget::{button, column, container, row, scrollable, space, text, text_editor, text_input, Column, Row};
use iced::{Alignment, Color, Fill, Font, Length};
use iced::Task;

use super::Effect;
use crate::api::client::Api;
use crate::api::data::Token;
use crate::api::drops::{DropInfo, SlotPage};
use crate::app::Message;
use crate::components::finance::duration_picker::{self, Duration};
use crate::components::finance::{fee_picker, token_select};
use crate::components::form::{input, label};
use crate::components::ui::{icon, El};
use crate::flows::drops::{self as fd, Plan, Row as DropRow, Unit};
use crate::icons;
use crate::theme::*;

const ROWS_PAGE: usize = 50;
const SLOTS_PAGE: u64 = 50;
const CLAIM_PRESETS: [Duration; 5] = [Duration::Hours(24), Duration::Days(7), Duration::Days(14), Duration::Days(30), Duration::Days(90)];
const WINDOWS: [u32; 4] = [1, 3, 6, 12];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotFilter {
    #[default]
    All,
    Unclaimed,
    Claimed,
}

#[derive(Debug, Clone)]
pub enum DropsMsg {
    ToggleNew,
    Title(String),
    Note(String),
    Token(Token),
    Parts(bool),
    RowAddress(usize, String),
    /// A contact picked for a row's wallet (its address).
    Contact(usize, String),
    RowAmount(usize, String),
    RowPlan(usize, u64),
    /// Every wallet on this plan.
    AllPlan(u64),
    RemoveRow(usize),
    AddRow,
    RowsPage(i64),
    PasteToggle,
    PasteEdit(text_editor::Action),
    PasteAdd,
    PickFile,
    FileLoaded(String, String),
    Fee(u32),
    Expiry(Duration),
    Create,
    PlanName(u64, String),
    PlanPayouts(u64, String),
    PlanUnit(u64, Unit),
    PlanLater(u64, bool),
    PlanDelay(u64, String),
    PlanUpfront(u64, String),
    PlanWindow(u64, u32),
    PlanAdd,
    PlanRemove(u64),
    Refresh,
    Loaded(Result<Vec<DropInfo>, String>),
    Open(Option<String>),
    Slots(Result<SlotPage, String>),
    Query(String),
    Filter(SlotFilter),
    SlotsPage(i64),
    Sync,
    Synced(Result<String, String>),
    Refund,
    CopyLink,
    OpenLink,
}

impl DropsMsg {
    /// The payout plans changed (they're kept with the app's state).
    pub fn touches_plans(&self) -> bool {
        matches!(self, DropsMsg::PlanName(..) | DropsMsg::PlanPayouts(..) | DropsMsg::PlanUnit(..) | DropsMsg::PlanLater(..) | DropsMsg::PlanDelay(..) | DropsMsg::PlanUpfront(..) | DropsMsg::PlanWindow(..) | DropsMsg::PlanAdd | DropsMsg::PlanRemove(..) | DropsMsg::PasteAdd | DropsMsg::FileLoaded(..))
    }
}

pub struct Form {
    pub title: String,
    pub note: String,
    pub token: Option<Token>,
    pub rows: Vec<DropRow>,
    pub parts: bool,
    pub fee_bps: u32,
    pub expiry: Duration,
    pub page: usize,
    pub paste_open: bool,
    pub paste: text_editor::Content,
}

impl Default for Form {
    fn default() -> Self {
        Self { title: String::new(), note: String::new(), token: None, rows: vec![DropRow::default()], parts: false, fee_bps: fee_picker::DEFAULT_BPS, expiry: Duration::Days(30), page: 0, paste_open: false, paste: text_editor::Content::new() }
    }
}

#[derive(Default)]
pub struct State {
    pub creating: bool,
    pub form: Form,
    pub drops: Option<Vec<DropInfo>>,
    pub loading: bool,
    pub error: Option<String>,
    pub open: Option<String>,
    pub slots: Option<SlotPage>,
    pub q: String,
    pub filter: SlotFilter,
    pub slots_page: u64,
    /// The web app's address (claim links), set at sign-in.
    pub web_base: String,
    /// A plan's number fields as typed ("", "1.", "05") until they read as a number: the plan
    /// itself keeps the last number that did.
    pub plan_text: std::collections::HashMap<(u64, PlanField), String>,
}

/// A plan's number fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanField {
    Payouts,
    Delay,
    Upfront,
}

/// A plan in the Plan column's list.
#[derive(Debug, Clone, PartialEq)]
struct PlanPick {
    id: u64,
    name: String,
}

impl std::fmt::Display for PlanPick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.name.trim().is_empty() { "Unnamed" } else { self.name.trim() })
    }
}

fn picks(plans: &[Plan]) -> Vec<PlanPick> {
    plans.iter().map(|p| PlanPick { id: p.id, name: p.name.clone() }).collect()
}

/// A typed number for a plan's field: kept as typed, applied once it reads as one. Only digits
/// (and one point for the unlock share) get in.
fn plan_number(state: &mut State, plans: &mut [Plan], id: u64, field: PlanField, v: String) {
    let t = v.trim().replace(',', ".");
    let points = if field == PlanField::Upfront { 1 } else { 0 };
    if !t.chars().all(|c| c.is_ascii_digit() || c == '.') || t.matches('.').count() > points {
        return;
    }
    state.plan_text.insert((id, field), v);
    let (Ok(n), Some(p)) = (t.parse::<f64>(), plans.iter_mut().find(|p| p.id == id)) else { return };
    match field {
        PlanField::Payouts => p.payouts = n.clamp(0.0, 1000.0) as u32,
        PlanField::Delay => p.delay_months = n.clamp(0.0, 600.0) as u32,
        PlanField::Upfront => p.upfront_pct = n.clamp(0.0, 100.0),
    }
}

/// What `update` needs from the app.
pub struct Ctx {
    pub api: Option<Api>,
    /// The web app's address (claim links point there).
    pub web_base: String,
}

fn msg(m: DropsMsg) -> Message {
    Message::Drops(m)
}

fn now() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

pub fn load(state: &mut State, api: &Api) -> Task<Message> {
    state.loading = true;
    Task::perform(crate::api::drops::mine(api.clone()), |r| msg(DropsMsg::Loaded(r.map_err(|e| e.message))))
}

fn load_slots(state: &mut State, api: &Api) -> Task<Message> {
    let Some(id) = state.open.clone() else { return Task::none() };
    let claimed = match state.filter {
        SlotFilter::All => None,
        SlotFilter::Claimed => Some(true),
        SlotFilter::Unclaimed => Some(false),
    };
    let q = Some(state.q.trim().to_string()).filter(|q| !q.is_empty());
    Task::perform(crate::api::drops::slots(api.clone(), id, state.slots_page * SLOTS_PAGE, SLOTS_PAGE, q, claimed), |r| msg(DropsMsg::Slots(r.map_err(|e| e.message))))
}

/// After a refund: the open drop's wallets again.
pub fn reload_open(state: &mut State, api: &Api) -> Task<Message> {
    load_slots(state, api)
}

/// After a drop went out: the next one starts clean (token, fee, mode stay).
pub fn sent(state: &mut State) {
    state.form.rows = vec![DropRow::default()];
    state.form.title.clear();
    state.form.note.clear();
    state.creating = false;
}

pub fn open_drop(state: &State) -> Option<&DropInfo> {
    let id = state.open.as_deref()?;
    state.drops.as_ref()?.iter().find(|d| d.drop_id == id)
}

pub fn link(base: &str, d: &DropInfo) -> String {
    format!("{}/drop/{}/{}", base.trim_end_matches('/'), crate::api::client::enc(&d.chain), d.drop_id)
}

/// Pasted / loaded lines into the form: plans named there are created (copies of the first one).
fn add_lines(form: &mut Form, plans: &mut Vec<Plan>, lines: Vec<fd::Line>, source: &str) -> Effect {
    if lines.is_empty() {
        return Effect::Error(format!("Nothing to add from {source}"));
    }
    let mut created: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for l in lines {
        let plan = if norm_empty(&l.plan) {
            None
        } else {
            let key = fd::norm_plan(&l.plan);
            match plans.iter().find(|p| fd::norm_plan(&p.name) == key) {
                Some(p) => Some(p.id),
                None => {
                    let base = plans.first().cloned().unwrap_or_default();
                    let id = plans.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                    plans.push(Plan { id, name: l.plan.trim().to_string(), ..base });
                    created.push(l.plan.trim().to_string());
                    Some(id)
                }
            }
        };
        rows.push(DropRow { address: l.address, amount: l.amount, plan });
    }
    let n = rows.len();
    form.rows.retain(|r| !r.blank());
    form.rows.extend(rows);
    form.page = 0;
    if !created.is_empty() {
        form.parts = true;
        return Effect::Info(format!("Added {n} wallet{} from {source}. New plan{} from the list: {} — set {} terms in Payout plans", if n == 1 { "" } else { "s" }, if created.len() > 1 { "s" } else { "" }, created.join(", "), if created.len() > 1 { "their" } else { "its" }));
    }
    Effect::Info(format!("Added {n} wallet{} from {source}", if n == 1 { "" } else { "s" }))
}

fn norm_empty(s: &str) -> bool {
    fd::norm_plan(s).is_empty()
}

/// The form checked, as CreateDropCard.create does before the review window.
pub fn draft(form: &Form, plans: &[Plan]) -> Result<fd::Draft, String> {
    let Some(token) = form.token.clone() else { return Err("Choose a token first".into()) };
    let start = now();
    let c = fd::collect(&form.rows, plans, form.parts, token.decimals, start);
    if c.recipients.is_empty() || !c.errors.is_empty() {
        return Err(c.errors.first().cloned().unwrap_or_else(|| "Add the wallets to pay".into()));
    }
    let expires_at = if form.parts {
        let used: Vec<&Plan> = c.rows.iter().filter_map(|i| fd::plan_of(plans, &form.rows[*i])).collect();
        used.iter().map(|p| p.closes_at(start)).max().unwrap_or(0)
    } else {
        form.expiry.ends_at(start as i64).unwrap_or(0).max(0) as u64
    };
    if expires_at <= start + 60 {
        return Err("Pick an end date in the future".into());
    }
    let total = c.recipients.iter().map(|r| r.amount).fold(alloy_primitives::U256::ZERO, |a, b| a + b);
    Ok(fd::Draft { title: form.title.clone(), note: form.note.clone(), token, recipients: c.recipients, total, fee_bps: form.fee_bps.min(u32::from(u16::MAX)) as u16, expires_at })
}

pub fn update(state: &mut State, plans: &mut Vec<Plan>, m: DropsMsg, ctx: Ctx) -> Effect {
    if plans.is_empty() {
        plans.push(Plan::default());
    }
    let f = &mut state.form;
    fn with(plans: &mut [Plan], id: u64, change: impl FnOnce(&mut Plan)) {
        if let Some(p) = plans.iter_mut().find(|p| p.id == id) {
            change(p);
        }
    }
    match m {
        DropsMsg::ToggleNew => state.creating = !state.creating,
        DropsMsg::Title(v) => f.title = v,
        DropsMsg::Note(v) => f.note = v,
        DropsMsg::Token(t) => f.token = Some(t),
        DropsMsg::Parts(v) => f.parts = v,
        DropsMsg::RowAddress(i, v) => {
            if let Some(r) = f.rows.get_mut(i) {
                let at = crate::components::contact_picker::opens(&r.address, &v);
                r.address = v.trim().to_string();
                if at {
                    return Effect::Contacts(crate::components::contact_picker::Target::DropWallet(i));
                }
            }
        }
        DropsMsg::Contact(i, address) => if let Some(r) = f.rows.get_mut(i) { r.address = address },
        DropsMsg::RowAmount(i, v) => if let Some(r) = f.rows.get_mut(i) { r.amount = v },
        DropsMsg::RowPlan(i, id) => if let Some(r) = f.rows.get_mut(i) { r.plan = Some(id) },
        DropsMsg::AllPlan(id) => f.rows.iter_mut().for_each(|r| r.plan = Some(id)),
        DropsMsg::RemoveRow(i) => {
            if f.rows.len() > 1 {
                f.rows.remove(i);
            } else {
                f.rows = vec![DropRow::default()];
            }
        }
        DropsMsg::AddRow => {
            let amount = f.rows.last().map(|r| r.amount.clone()).unwrap_or_default();
            f.rows.push(DropRow { amount, ..Default::default() });
            f.page = (f.rows.len() - 1) / ROWS_PAGE;
        }
        DropsMsg::RowsPage(d) => {
            let pages = f.rows.len().div_ceil(ROWS_PAGE).max(1) as i64;
            f.page = (f.page as i64 + d).clamp(0, pages - 1) as usize;
        }
        DropsMsg::PasteToggle => f.paste_open = !f.paste_open,
        DropsMsg::PasteEdit(a) => f.paste.perform(a),
        DropsMsg::PasteAdd => {
            let text = f.paste.text();
            return match fd::parse_list(&text) {
                Ok(lines) => {
                    let e = add_lines(f, plans, lines, "the pasted list");
                    if matches!(e, Effect::Info(_)) {
                        f.paste = text_editor::Content::new();
                        f.paste_open = false;
                    }
                    e
                }
                Err(e) => Effect::Error(e),
            };
        }
        DropsMsg::PickFile => return Effect::Run(Task::done(Message::Picker(crate::components::file_picker::PickerMsg::Open(crate::components::file_picker::Purpose::DropList)))),
        DropsMsg::FileLoaded(name, content) => {
            let parsed = if name.to_lowercase().ends_with(".json") { fd::parse_json(&content) } else { fd::parse_list(&content) };
            return match parsed {
                Ok(lines) => add_lines(f, plans, lines, &name),
                Err(_) => Effect::Error("Couldn't read that file — use CSV lines `address, amount` (for payout plans, a header line `address, amount, plan`) or JSON [{address, amount, plan}]".into()),
            };
        }
        DropsMsg::Fee(b) => f.fee_bps = b,
        DropsMsg::Expiry(Duration::Custom) => return Effect::PickTime(crate::components::time_picker::Purpose::DropStart, match f.expiry { Duration::At(t) => Some(t), _ => None }),
        DropsMsg::Expiry(d) => f.expiry = d,
        DropsMsg::Create => {
            return match draft(f, plans) {
                Ok(d) => Effect::CreateDrop(Box::new(d)),
                Err(e) => Effect::Error(e),
            }
        }
        DropsMsg::PlanName(id, v) => with(plans, id, |p| p.name = v.chars().take(40).collect()),
        DropsMsg::PlanPayouts(id, v) => plan_number(state, plans, id, PlanField::Payouts, v),
        DropsMsg::PlanUnit(id, u) => with(plans, id, |p| p.unit = u),
        DropsMsg::PlanLater(id, later) => {
            state.plan_text.remove(&(id, PlanField::Delay));
            with(plans, id, |p| p.delay_months = if later { p.delay_months.max(6) } else { 0 });
        }
        DropsMsg::PlanDelay(id, v) => plan_number(state, plans, id, PlanField::Delay, v),
        DropsMsg::PlanUpfront(id, v) => plan_number(state, plans, id, PlanField::Upfront, v),
        DropsMsg::PlanWindow(id, w) => with(plans, id, |p| p.window_months = w),
        DropsMsg::PlanAdd => {
            let id = plans.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            let n = plans.len() + 1;
            plans.push(Plan { id, name: format!("Plan {n}"), ..Plan::default() });
        }
        DropsMsg::PlanRemove(id) => {
            if plans.len() > 1 {
                state.plan_text.retain(|(plan, _), _| *plan != id);
                plans.retain(|p| p.id != id);
                for r in &mut f.rows {
                    if r.plan == Some(id) {
                        r.plan = None;
                    }
                }
            }
        }
        DropsMsg::Refresh => return ctx.api.map(|a| Effect::Run(load(state, &a))).unwrap_or(Effect::None),
        DropsMsg::Loaded(r) => {
            state.loading = false;
            match r {
                Ok(list) => {
                    state.drops = Some(list);
                    state.error = None;
                }
                Err(e) => state.error = Some(e),
            }
        }
        DropsMsg::Open(id) => {
            state.open = id;
            state.slots = None;
            state.q.clear();
            state.filter = SlotFilter::All;
            state.slots_page = 0;
            return ctx.api.map(|a| Effect::Run(load_slots(state, &a))).unwrap_or(Effect::None);
        }
        DropsMsg::Slots(r) => state.slots = Some(r.unwrap_or_default()),
        DropsMsg::Query(q) => {
            state.q = q;
            state.slots_page = 0;
            return ctx.api.map(|a| Effect::Run(load_slots(state, &a))).unwrap_or(Effect::None);
        }
        DropsMsg::Filter(fl) => {
            state.filter = fl;
            state.slots_page = 0;
            return ctx.api.map(|a| Effect::Run(load_slots(state, &a))).unwrap_or(Effect::None);
        }
        DropsMsg::SlotsPage(d) => {
            let total = state.slots.as_ref().map_or(0, |s| s.total);
            let last = total.saturating_sub(1) / SLOTS_PAGE;
            state.slots_page = (state.slots_page as i64 + d).clamp(0, last as i64) as u64;
            return ctx.api.map(|a| Effect::Run(load_slots(state, &a))).unwrap_or(Effect::None);
        }
        DropsMsg::Sync => {
            let (Some(api), Some(id)) = (ctx.api, state.open.clone()) else { return Effect::None };
            let summarise = |v: serde_json::Value| {
                let n = v.get("newly_claimed").and_then(|n| n.as_u64()).unwrap_or(0);
                if n > 0 { format!("Found {n} new claim{}", if n > 1 { "s" } else { "" }) } else { "Up to date".to_string() }
            };
            return Effect::Run(Task::perform(crate::api::drops::sync(api, id, None), move |r| msg(DropsMsg::Synced(r.map(summarise).map_err(|e| e.message)))));
        }
        DropsMsg::Synced(r) => {
            let api = ctx.api;
            let reload = api.map(|a| Task::batch([load(state, &a), load_slots(state, &a)])).unwrap_or_else(Task::none);
            return match r {
                Ok(t) => Effect::Batch(vec![Effect::Info(t), Effect::Run(reload)]),
                Err(e) => Effect::Error(format!("Couldn't check the chain: {e}")),
            };
        }
        DropsMsg::Refund => {
            if let Some(d) = open_drop(state) {
                return Effect::RefundDrop(Box::new(d.clone()));
            }
        }
        DropsMsg::CopyLink => {
            if let Some(d) = open_drop(state) {
                return Effect::Copy(link(&ctx.web_base, d), "Link");
            }
        }
        DropsMsg::OpenLink => {
            if let Some(d) = open_drop(state) {
                let url = link(&ctx.web_base, d);
                let _ = open::that_detached(&url);
            }
        }
    }
    Effect::None
}

// ── view ──

fn status_of(d: &DropInfo) -> (&'static str, Color) {
    match d.status.as_str() {
        "completed" => ("All claimed", INDIGO_SOFT),
        "expired" => ("Ended", AMBER_TEXT),
        "refunded" => ("Refunded", TEXT_SECONDARY),
        _ => ("Active", GREEN),
    }
}

fn when(unix: u64) -> String {
    chrono::DateTime::from_timestamp(unix as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y, %H:%M").to_string()).unwrap_or_default()
}

fn progress<'a>(d: &DropInfo) -> El<'a> {
    let (n, of) = (d.claimed_count.unwrap_or(0), d.recipients_count.unwrap_or(0));
    let pct = if of > 0 { n as f32 / of as f32 } else { 0.0 };
    column![
        iced::widget::progress_bar(0.0..=1.0, pct).girth(6).style(|_| iced::widget::progress_bar::Style { background: rgba(0xffffff, 0.06).into(), bar: INDIGO.into(), border: border(Color::TRANSPARENT, 0.0, 3.0) }),
        row![text(format!("{n} / {of} wallets paid in full")).size(11.5).color(TEXT_SECONDARY), space().width(Fill), text(format!("{} / {} {}", d.claimed.clone().unwrap_or_else(|| "0".into()), d.total, d.token)).size(11.5).color(TEXT_SECONDARY)],
    ]
    .spacing(4)
    .into()
}

fn chip_btn<'a>(caption: String, on: bool, m: DropsMsg) -> El<'a> {
    button(text(caption).size(11.5).font(if on { BOLD } else { INTER })).padding([3, 8]).style(chip(on)).on_press(msg(m)).into()
}

fn small_input<'a>(placeholder: &str, value: &str, width: f32, on: impl Fn(String) -> Message + 'a) -> El<'a> {
    text_input(placeholder, value).on_input(on).size(12.5).padding([5, 8]).width(width).style(field).into()
}

fn form_view<'a>(st: &'a State, plans: &'a [Plan], tokens: token_select::Field) -> El<'a> {
    let f = &st.form;
    let dec = f.token.as_ref().map_or(18, |t| t.decimals);
    let sym = f.token.as_ref().map(|t| t.symbol.clone()).unwrap_or_default();
    let c = fd::collect(&f.rows, plans, f.parts, dec, now());
    let total = c.recipients.iter().map(|r| r.amount).fold(alloy_primitives::U256::ZERO, |a, b| a + b);
    let total_text = format!("{} {sym}", crate::evm::client::format_units(total, dec));

    let mode = row![
        button(container(text("All at once").size(12.5).font(SEMIBOLD)).center_x(Fill)).width(Fill).padding([8, 0]).style(if f.parts { tone_button(Tone::Dark) } else { tone_button(Tone::Accent) }).on_press(msg(DropsMsg::Parts(false))),
        button(container(text("In parts over time").size(12.5).font(SEMIBOLD)).center_x(Fill)).width(Fill).padding([8, 0]).style(if f.parts { tone_button(Tone::Accent) } else { tone_button(Tone::Dark) }).on_press(msg(DropsMsg::Parts(true))),
    ]
    .spacing(6);

    // ── wallets ──
    // "In parts over time": each wallet's plan (one plan: it's just shown).
    let show_plan = f.parts;
    let errors: std::collections::HashMap<usize, &str> = c.row_errors.iter().copied().collect();
    let mut list: Column<'a, Message> = column![row![
        text("#").size(11).color(FAINT).width(24),
        text("Wallet").size(11).color(FAINT).width(Length::FillPortion(5)),
        text("Amount").size(11).color(FAINT).width(110),
        text(if show_plan { "Plan" } else { "" }).size(11).color(FAINT).width(if show_plan { 130 } else { 0 }),
        space().width(28),
    ]
    .spacing(6)]
    .spacing(4);
    let start = f.page * ROWS_PAGE;
    for (i, r) in f.rows.iter().enumerate().skip(start).take(ROWS_PAGE) {
        let mut line = row![
            text((i + 1).to_string()).size(11.5).color(TEXT_SECONDARY).width(24),
            container(small_input("0x… (@ contacts)", &r.address, 1000.0, move |v| msg(DropsMsg::RowAddress(i, v)))).width(Length::FillPortion(5)),
            small_input("0.0", &r.amount, 110.0, move |v| msg(DropsMsg::RowAmount(i, v))),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if show_plan {
            let cur = fd::plan_of(plans, r).map(|p| PlanPick { id: p.id, name: p.name.clone() });
            let cell: El<'a> = if plans.len() > 1 {
                iced::widget::pick_list(picks(plans), cur, move |p: PlanPick| msg(DropsMsg::RowPlan(i, p.id))).text_size(12).padding([5, 8]).width(130).style(crate::theme::token_select).menu_style(chain_menu).into()
            } else {
                container(text(cur.map(|p| p.to_string()).unwrap_or_default()).size(12).color(TEXT_SECONDARY)).width(130).padding([5, 8]).into()
            };
            line = line.push(cell);
        }
        line = line.push(button(icon(icons::trash(), 13, DANGER_TEXT)).padding(5).style(ghost(5.0, rgba(0xef4444, 0.12))).on_press(msg(DropsMsg::RemoveRow(i))));
        list = list.push(line);
        if let Some(e) = errors.get(&i) {
            list = list.push(container(text(*e).size(11).color(DANGER_TEXT)).padding(iced::Padding { top: 0.0, right: 0.0, bottom: 2.0, left: 30.0 }));
        }
    }
    let pages = f.rows.len().div_ceil(ROWS_PAGE).max(1);
    let mut under = row![button(row![icon(icons::plus(), 12, TEXT_BODY), text("Add wallet").size(11.5)].spacing(4).align_y(Alignment::Center)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::AddRow))].spacing(8).align_y(Alignment::Center);
    if pages > 1 {
        under = under.push(space().width(Fill)).push(
            row![
                button(text("‹").size(13)).padding([2, 8]).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press_maybe((f.page > 0).then(|| msg(DropsMsg::RowsPage(-1)))),
                text(format!("{}–{} of {}", start + 1, (start + ROWS_PAGE).min(f.rows.len()), f.rows.len())).size(11.5).color(TEXT_SECONDARY),
                button(text("›").size(13)).padding([2, 8]).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press_maybe((f.page + 1 < pages).then(|| msg(DropsMsg::RowsPage(1)))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    let wallets_head = row![
        text(format!("Wallets ({})", c.recipients.len())).size(12.5).font(MEDIUM).color(LABEL),
        space().width(Fill),
    ]
    .push((f.parts && plans.len() > 1).then(|| {
        iced::widget::pick_list(picks(plans), None::<PlanPick>, |p: PlanPick| msg(DropsMsg::AllPlan(p.id))).placeholder("Plan for all").text_size(11.5).padding([3, 8]).style(crate::theme::token_select).menu_style(chain_menu)
    }))
    .push(button(row![icon(icons::file_text(), 11, TEXT_BODY), text("Paste list").size(11)].spacing(4).align_y(Alignment::Center)).padding([3, 8]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::PasteToggle)))
    .push(button(row![icon(icons::upload(), 11, TEXT_BODY), text("CSV / JSON").size(11)].spacing(4).align_y(Alignment::Center)).padding([3, 8]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::PickFile)))
    .spacing(6)
    .align_y(Alignment::Center);
    let mut wallets = column![wallets_head].spacing(8);
    if f.paste_open {
        wallets = wallets.push(
            column![
                text("One wallet per line: `address, amount` — or a JSON list. For payout plans, start with a header line: `address, amount, plan`.").size(11).color(TEXT_SECONDARY),
                text_editor(&f.paste).on_action(|a| msg(DropsMsg::PasteEdit(a))).height(110).size(12.5).font(Font::MONOSPACE),
                row![space().width(Fill), button(text("Add to list").size(12).font(SEMIBOLD)).padding([5, 12]).style(tone_button(Tone::Accent)).on_press(msg(DropsMsg::PasteAdd))],
            ]
            .spacing(6),
        );
    }
    wallets = wallets.push(list).push(under);

    // ── until when ──
    let until: El = if f.parts {
        let used: Vec<&Plan> = if c.rows.is_empty() { plans.iter().collect() } else { c.rows.iter().filter_map(|i| fd::plan_of(plans, &f.rows[*i])).collect() };
        let closes = used.iter().map(|p| p.closes_at(now())).max().unwrap_or(0);
        container(row![icon(icons::clock(), 12, TEXT_SECONDARY), text("Claims close").size(12).font(SEMIBOLD).color(LABEL), space().width(Fill), text(format!("≈ {} (the payout plans decide)", when(closes))).size(11).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([10, 12])
            .style(inset)
            .into()
    } else {
        duration_picker::view("Claim until", &CLAIM_PRESETS, f.expiry, None, |d| msg(DropsMsg::Expiry(d)))
    };

    let mut col = column![
        column![text("New drop").size(18).font(BOLD).color(Color::WHITE), text("Send tokens to a list of wallets from one deposit. Each wallet claims its own share — nobody else can.").size(12.5).color(TEXT_SECONDARY)].spacing(3),
        column![label("Name (shown on the claim page)", false), input("e.g. Team tokens 2027", &f.title, |v| msg(DropsMsg::Title(v)))].spacing(6),
        row![
            column![label("Token", false), token_select::view(tokens, f.token.as_ref(), crate::components::token_picker::Purpose::Drop)].spacing(6).width(Fill),
            column![label("Total to lock", false), container(row![text(total_text.clone()).size(13).font(Font::MONOSPACE).color(GREEN), space().width(Fill), icon(icons::users(), 12, MUTED), text(c.recipients.len().to_string()).size(11.5).color(MUTED)].spacing(4).align_y(Alignment::Center)).padding([10, 12]).style(boxed(hex(0x0e0f17), rgba(0x10b981, 0.35), 8.0))].spacing(6).width(Fill),
        ]
        .spacing(12),
        column![label("How to pay out", false), container(mode).padding(4).style(boxed(rgba(0x000000, 0.2), BORDER, 9.0))].spacing(6),
        wallets,
    ]
    .spacing(16);
    if let Some(e) = c.errors.first().filter(|_| !c.row_errors.is_empty() || f.parts) {
        col = col.push(text(e.clone()).size(12).color(DANGER_TEXT));
    }
    col = col
        .push(until)
        .push(fee_picker::view("Platform fee (deducted on claim)", f.fee_bps, None, false, |b| msg(DropsMsg::Fee(b))))
        .push(column![label("Note (optional, public)", false), input("", &f.note, |v| msg(DropsMsg::Note(v))).padding([14, 12])].spacing(6))
        .push(
            button(container(text(format!("Create drop · {total_text}")).size(14).font(SEMIBOLD)).center_x(Fill))
                .width(Fill)
                .padding([10, 0])
                .style(tone_button(Tone::Accent))
                .on_press(msg(DropsMsg::Create)),
        );
    container(col).width(Fill).padding(22).style(settings_card(false)).into()
}

fn plans_view<'a>(st: &'a State, plans: &'a [Plan]) -> El<'a> {
    let start = now();
    let active = st.form.parts;
    let mut col: Column<'a, Message> = column![column![
        text("Payout plans").size(18).font(BOLD).color(if active { Color::WHITE } else { TEXT_SECONDARY }),
        text("Used when the drop pays \"In parts over time\". Kept on this computer.").size(12).color(TEXT_SECONDARY),
    ]
    .spacing(3)]
    .spacing(12);
    for p in plans {
        let id = p.id;
        // A number as typed while it's being typed, else the plan's.
        let typed = |field: PlanField, value: String| st.plan_text.get(&(id, field)).cloned().unwrap_or(value);
        let mut units: Row<'a, Message> = row![].spacing(3);
        for u in Unit::ALL {
            units = units.push(chip_btn(u.many().into(), p.unit == u, DropsMsg::PlanUnit(id, u)));
        }
        let mut windows: Row<'a, Message> = row![].spacing(3);
        for w in WINDOWS {
            windows = windows.push(chip_btn(format!("{w} mo"), p.window_months == w, DropsMsg::PlanWindow(id, w)));
        }
        let later = p.delay_months > 0;
        let mut first = row![text("first payout").size(12).color(TEXT_SECONDARY), chip_btn("right away".into(), !later, DropsMsg::PlanLater(id, false)), chip_btn("later".into(), later, DropsMsg::PlanLater(id, true))].spacing(4).align_y(Alignment::Center);
        if later {
            first = first.push(text("after").size(12).color(TEXT_SECONDARY)).push(small_input("6", &typed(PlanField::Delay, p.delay_months.to_string()), 50.0, move |v| msg(DropsMsg::PlanDelay(id, v)))).push(text("months").size(12).color(TEXT_SECONDARY));
        }
        let mut head = row![small_input("Plan name", &p.name, 150.0, move |v| msg(DropsMsg::PlanName(id, v))), space().width(Fill)].align_y(Alignment::Center);
        if plans.len() > 1 {
            head = head.push(button(icon(icons::x(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(DropsMsg::PlanRemove(id))));
        }
        let body = column![
            head,
            row![text("Pay over").size(12).color(TEXT_SECONDARY), small_input("12", &typed(PlanField::Payouts, p.payouts.to_string()), 56.0, move |v| msg(DropsMsg::PlanPayouts(id, v))), units].spacing(6).align_y(Alignment::Center),
            first,
            row![text("unlock").size(12).color(TEXT_SECONDARY), small_input("0", &typed(PlanField::Upfront, format!("{}", p.upfront_pct)), 56.0, move |v| msg(DropsMsg::PlanUpfront(id, v))), text("% right away").size(12).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center),
            row![text("claim within").size(12).color(TEXT_SECONDARY), windows, text("after the last payout").size(12).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center),
            text(if p.valid() { p.summary(start) } else { "Payouts: 1 to 1000, unlock: 0 to 100%".into() })
            .size(11)
            .color(if p.valid() { FAINT } else { DANGER_TEXT }),
        ]
        .spacing(8);
        col = col.push(container(body).width(Fill).padding([12, 14]).style(boxed(rgba(0xffffff, 0.02), BORDER, 10.0)));
    }
    col = col.push(button(row![icon(icons::plus(), 12, TEXT_BODY), text("New plan").size(11.5)].spacing(4).align_y(Alignment::Center)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::PlanAdd)));
    col = col.push(
        text("e.g. Team — over 24 months, first payout after 6 · Investors — 10% right away, the rest over 12 months · Community — 100% right away. Each wallet's plan is picked in the Plan column; a `plan` column in a pasted list picks one by name.")
            .size(11)
            .color(FAINT),
    );
    container(col).width(Fill).padding(22).style(settings_card(false)).into()
}

fn list_view<'a>(st: &'a State) -> El<'a> {
    match (&st.drops, &st.error) {
        (None, Some(e)) => container(column![text("Couldn't load your drops").size(14).font(BOLD).color(DANGER_TEXT), text(e.as_str()).size(12.5).color(TEXT_SECONDARY), button(text("Try again").size(12)).padding([5, 12]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::Refresh))].spacing(8).align_x(Alignment::Center)).center_x(Fill).padding(40).into(),
        (None, None) => container(text("Loading your drops…").size(13).color(MUTED)).center_x(Fill).padding(40).into(),
        (Some(list), _) if list.is_empty() => {
            if st.creating {
                return space().into();
            }
            container(
                column![
                    icon(icons::gift(), 32, MUTED),
                    text("No drops yet").size(17).font(BOLD).color(Color::WHITE),
                    text("Upload a list of wallets and amounts — each wallet gets a claim page link.").size(14).color(TEXT_SECONDARY),
                    space().height(4),
                    button(text("Create a drop").size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press(msg(DropsMsg::ToggleNew)),
                ]
                .spacing(10)
                .align_x(Alignment::Center),
            )
            .width(Fill)
            .center_x(Fill)
            .padding([56, 24])
            .style(boxed(rgba(0xffffff, 0.015), rgba(0xffffff, 0.04), 14.0))
            .into()
        }
        (Some(list), _) => {
            let mut col: Column<'a, Message> = column![].spacing(10);
            for d in list {
                let (label, color) = status_of(d);
                col = col.push(
                    button(
                        column![
                            row![
                                column![
                                    text(d.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| format!("Drop {}", crate::wallets::short_address(&d.drop_id)))).size(14).font(BOLD).color(Color::WHITE),
                                    text(format!("{} · ends {}", d.chain, when(d.expires_at))).size(12).color(TEXT_SECONDARY),
                                ]
                                .spacing(2),
                                space().width(Fill),
                                text(label).size(12).font(BOLD).color(color),
                            ],
                            progress(d),
                        ]
                        .spacing(10),
                    )
                    .width(Fill)
                    .padding(16)
                    .style(|_, s| button::Style { background: Some(rgba(0xffffff, if matches!(s, button::Status::Hovered) { 0.04 } else { 0.02 }).into()), text_color: TEXT, border: border(BORDER, 1.0, 12.0), ..Default::default() })
                    .on_press(msg(DropsMsg::Open(Some(d.drop_id.clone())))),
                );
            }
            col.into()
        }
    }
}

fn details<'a>(st: &'a State, d: &'a DropInfo, web_base: &str) -> El<'a> {
    let (label, color) = status_of(d);
    let ended = d.expires_at < now();
    let can_refund = ended && !d.refunded && d.claimed_count.unwrap_or(0) < d.recipients_count.unwrap_or(0);
    let fee = format!("{:.2}", f64::from(d.fee_bps) / 100.0);
    let creation = d.creation_fee.as_deref().filter(|f| f.parse::<f64>().is_ok_and(|x| x > 0.0)).map(|f| format!(" · creation fee {f}")).unwrap_or_default();
    let head = row![
        column![
            text(d.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| format!("Drop {}", crate::wallets::short_address(&d.drop_id)))).size(20).font(BOLD).color(Color::WHITE),
            text(format!("{} · {} {} · claim fee {fee}%{creation} · {} {}", d.chain, d.total, d.token, if ended { "ended" } else { "claim until" }, when(d.expires_at))).size(12.5).color(TEXT_SECONDARY),
        ]
        .spacing(4),
        space().width(Fill),
        text(label).size(13).font(BOLD).color(color),
    ];
    let link_text = link(web_base, d);
    let link_row = column![
        text("Claim page — share it anywhere; only listed wallets can claim").size(12).color(TEXT_SECONDARY),
        row![
            container(text(link_text).size(12).font(Font::MONOSPACE).color(TEXT_BODY).wrapping(iced::widget::text::Wrapping::None)).width(Fill).padding([8, 10]).clip(true).style(boxed(rgba(0x000000, 0.25), BORDER, 8.0)),
            button(icon(icons::copy(), 14, TEXT_BODY)).padding(8).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::CopyLink)),
            button(icon(icons::external_link(), 14, TEXT_BODY)).padding(8).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::OpenLink)),
        ]
        .spacing(6),
    ]
    .spacing(4);
    let mut actions = row![button(row![icon(icons::refresh_cw(), 14, TEXT_BODY), text("Check the chain for claims").size(12.5)].spacing(6).align_y(Alignment::Center)).padding([7, 12]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::Sync))].spacing(8);
    if can_refund {
        actions = actions.push(button(row![icon(icons::rotate_ccw(), 14, TEXT_BODY), text("Take back the rest").size(12.5)].spacing(6).align_y(Alignment::Center)).padding([7, 12]).style(tone_button(Tone::Dark)).on_press(msg(DropsMsg::Refund)));
    }
    let mut filters: Row<'a, Message> = row![
        container(row![icon(icons::search(), 13, MUTED), text_input("Find a wallet", &st.q).size(12.5).padding([6, 0]).style(bare_input).on_input(|q| msg(DropsMsg::Query(q)))].spacing(6).align_y(Alignment::Center)).width(Fill).padding([0, 10]).style(boxed(rgba(0xffffff, 0.02), BORDER, 8.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    for (f, l) in [(SlotFilter::All, "All"), (SlotFilter::Unclaimed, "Unclaimed"), (SlotFilter::Claimed, "Claimed")] {
        filters = filters.push(chip_btn(l.into(), st.filter == f, DropsMsg::Filter(f)));
    }
    let slots: El = match &st.slots {
        None => container(text("Loading the wallets…").size(13).color(MUTED)).padding(20).into(),
        Some(p) if p.items.is_empty() => container(text("Nothing here").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(12).into(),
        Some(p) => {
            let mut col: Column<'a, Message> = column![].spacing(4);
            // Names exist only on drops made before they were dropped from the form (owner-only).
            let labelled = p.items.iter().any(|s| s.label.as_deref().is_some_and(|l| !l.trim().is_empty()));
            for s in &p.items {
                let state: (String, Color) = if s.claimed {
                    ("Claimed".into(), GREEN)
                } else if s.claimed_amount.parse::<f64>().is_ok_and(|x| x > 0.0) {
                    ("In progress".into(), INDIGO_SOFT)
                } else {
                    ("Waiting".into(), TEXT_SECONDARY)
                };
                let amount = if s.schedule.periods > 1 || s.claimed_amount.parse::<f64>().is_ok_and(|x| x > 0.0) { format!("{} / {} {}", s.claimed_amount, s.amount, d.token) } else { format!("{} {}", s.amount, d.token) };
                col = col.push(
                    container(
                        row![text(format!("#{}", s.slot + 1)).size(11.5).color(FAINT).width(44)]
                        .push(labelled.then(|| text(s.label.clone().unwrap_or_default()).size(12.5).color(TEXT_BODY).width(Length::FillPortion(2))))
                        .push(text(crate::wallets::short_address(&s.address)).size(12).font(Font::MONOSPACE).color(TEXT_BODY).width(Length::FillPortion(2)))
                        .push(text(amount).size(12).font(Font::MONOSPACE).color(TEXT_BODY).width(Length::FillPortion(2)))
                        .push(text(state.0).size(12).color(state.1).width(80))
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 10])
                    .style(boxed(rgba(0xffffff, 0.02), Color::TRANSPARENT, 6.0)),
                );
            }
            if p.total > SLOTS_PAGE {
                let from = st.slots_page * SLOTS_PAGE;
                col = col.push(
                    row![
                        space().width(Fill),
                        button(text("Previous").size(12)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press_maybe((st.slots_page > 0).then(|| msg(DropsMsg::SlotsPage(-1)))),
                        text(format!("{}–{} of {}", from + 1, (from + SLOTS_PAGE).min(p.total), p.total)).size(12).color(TEXT_SECONDARY),
                        button(text("Next").size(12)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press_maybe((from + SLOTS_PAGE < p.total).then(|| msg(DropsMsg::SlotsPage(1)))),
                        space().width(Fill),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
            col.into()
        }
    };
    container(
        column![
            button(text("← All drops").size(13).color(TEXT_SECONDARY)).padding(0).style(link_button).on_press(msg(DropsMsg::Open(None))),
            head,
            progress(d),
            link_row,
            actions,
            column![filters, slots].spacing(10),
        ]
        .spacing(16),
    )
    .width(Fill)
    .padding(22)
    .style(settings_card(false))
    .into()
}

pub fn view<'a>(st: &'a State, plans: &'a [Plan], tokens: token_select::Field, web_base: &'a str) -> El<'a> {
    let header = row![
        column![
            row![icon(icons::gift(), 22, INDIGO_SOFT), text("Drops").size(24).font(EXTRABOLD).color(Color::WHITE)].spacing(10).align_y(Alignment::Center),
            text("Airdrops and mass payouts: one deposit, a list of wallets, each claims its own amount.").size(13).color(TEXT_SECONDARY),
        ]
        .spacing(4),
        space().width(Fill),
        button(row![icon(if st.creating { icons::x() } else { icons::plus() }, 14, Color::WHITE), text(if st.creating { "Close" } else { "New drop" }).size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
            .padding([9, 16])
            .style(tone_button(Tone::Accent))
            .on_press(msg(DropsMsg::ToggleNew)),
    ]
    .align_y(Alignment::Center);
    let mut col = column![header].spacing(20);
    if st.creating {
        col = col.push(row![container(form_view(st, plans, tokens)).width(Length::FillPortion(1)), container(plans_view(st, plans)).width(Length::FillPortion(1))].spacing(16));
    }
    col = col.push(match open_drop(st) {
        Some(d) => details(st, d, web_base),
        None => list_view(st),
    });
    scrollable(container(col).padding([20, 24]).width(Fill)).height(Fill).style(thin_scroll).into()
}
