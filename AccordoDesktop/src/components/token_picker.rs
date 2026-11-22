// SPDX-License-Identifier: GPL-3.0-or-later
//! Token picker (TokenModal.tsx, desktop-style): a big window, the network's tokens on the left
//! (search, the ones this wallet holds first, logos, balances), the picked one on the right — its
//! contract, decimals, balance and price, and a check against the contract itself: name, symbol
//! and decimals read from the chain must match the list, so a look-alike can't pass as USDC.
//! Your own tokens are added by address only; their symbol and decimals come from the contract.

use std::collections::HashMap;

use alloy_primitives::U256;
use iced::widget::{button, column, container, image, mouse_area, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Length, Padding};

use super::overlay;
use super::ui::{icon, El};
use crate::api::data::Token;
use crate::app::Message;
use crate::icons;
use crate::theme::{self, *};

pub const SEARCH: &str = "token-search";
pub const ADD_INPUT: &str = "token-add";
/// Rows shown at most (Base alone has ~2 300 tokens; search narrows it).
pub const MAX_ROWS: usize = 200;
const ZERO: &str = "0x0000000000000000000000000000000000000000";

/// Which form asked: where the pick goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Cheque,
    /// What a swap cheque asks for in return.
    SwapGet,
    Invoice,
    #[cfg(feature = "crowdfund")]
    Campaign,
    #[cfg(feature = "drops")]
    Drop,
    /// A module's payments.selectTokenModal: no balances (with an address they'd tell a module
    /// how much the user has); closing it is the module's 4001.
    Module,
}

/// (name, symbol, decimals) as the contract reports them.
pub type OnChain = Result<(String, String, u8), String>;

#[derive(Debug, Clone)]
pub enum TokenPickerMsg {
    Open(Purpose),
    Close,
    Query(String),
    Move(i32),
    Highlight(String),
    Use,
    Balances(String, HashMap<String, U256>),
    Prices(HashMap<String, f64>),
    Checked(String, OnChain),
    StartAdd,
    AddAddress(String),
    AddInfo(String, OnChain),
    AddSave,
    AddCancel,
    Copy(String),
    Explorer(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Running,
    Done(OnChain),
}

pub struct AddForm {
    pub address: String,
    /// The contract's answer for `address` (None while reading / not a full address yet).
    pub info: Option<(String, OnChain)>,
}

pub struct State {
    pub purpose: Purpose,
    pub chain_id: String,
    pub query: String,
    /// Lowercase address of the token shown on the right.
    pub highlighted: Option<String>,
    /// Lowercase address → balance of the active wallet (ZERO = the gas coin).
    pub balances: HashMap<String, U256>,
    pub balances_loading: bool,
    /// Lowercase address → USD price.
    pub prices: HashMap<String, f64>,
    pub checks: HashMap<String, Check>,
    pub add: Option<AddForm>,
}

impl State {
    pub fn new(purpose: Purpose, chain_id: String, selected: Option<&Token>) -> Self {
        Self {
            purpose,
            chain_id,
            query: String::new(),
            highlighted: selected.map(|t| key(&t.address)),
            balances: HashMap::new(),
            balances_loading: false,
            prices: HashMap::new(),
            checks: HashMap::new(),
            add: None,
        }
    }

    fn held(&self, t: &Token) -> Option<U256> {
        self.balances.get(&key(&t.address)).copied().filter(|b| !b.is_zero())
    }
}

pub fn key(address: &str) -> String {
    let a = address.trim().to_lowercase();
    if a.is_empty() || a == "native" { ZERO.to_string() } else { a }
}

pub fn is_native(t: &Token) -> bool {
    key(&t.address) == ZERO
}

/// One row: the token and whether you added it yourself.
#[derive(Debug, Clone)]
pub struct Entry {
    pub token: Token,
    pub custom: bool,
}

/// What the list shows, in order: matches of the search, held tokens first (each group keeps the
/// list's own order), at most MAX_ROWS.
pub fn visible(st: &State, custom: &[Token], listed: &[Token]) -> Vec<Entry> {
    let q = st.query.trim().to_lowercase();
    let hit = |t: &Token| q.is_empty() || t.symbol.to_lowercase().contains(&q) || t.name.to_lowercase().contains(&q) || t.address.to_lowercase().contains(&q);
    let mut seen = std::collections::HashSet::new();
    let all = custom.iter().map(|t| (t, true)).chain(listed.iter().map(|t| (t, false)));
    let mut out: Vec<Entry> = Vec::new();
    for (t, is_custom) in all {
        if hit(t) && seen.insert(key(&t.address)) {
            out.push(Entry { token: t.clone(), custom: is_custom });
        }
    }
    let (mut held, rest): (Vec<Entry>, Vec<Entry>) = out.into_iter().partition(|e| st.held(&e.token).is_some());
    held.extend(rest);
    held.truncate(MAX_ROWS);
    held
}

/// "12 345.6789" with up to `max_frac` decimals.
pub fn amount(v: U256, decimals: u8, max_frac: usize) -> String {
    let s = crate::evm::client::format_units(v, decimals);
    let (w, f) = s.split_once('.').unwrap_or((&s, ""));
    let f: String = f.chars().take(max_frac).collect();
    let f = f.trim_end_matches('0');
    let mut grouped = String::new();
    for (i, c) in w.chars().enumerate() {
        if i > 0 && (w.len() - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(c);
    }
    if f.is_empty() { grouped } else { format!("{grouped}.{f}") }
}

fn usd(v: U256, decimals: u8, price: Option<f64>) -> Option<String> {
    let p = price?;
    let n: f64 = crate::evm::client::format_units(v, decimals).parse().ok()?;
    let total = n * p;
    Some(if total >= 0.01 { format!("≈ ${total:.2}") } else { "< $0.01".into() })
}

fn short(a: &str) -> String {
    if a.len() > 14 { format!("{}…{}", &a[..8], &a[a.len() - 6..]) } else { a.to_string() }
}

fn msg(m: TokenPickerMsg) -> Message {
    Message::TokenPicker(m)
}

/// The token's picture, or its first letter on a colour.
pub fn logo<'a>(t: &Token, handle: Option<&image::Handle>, size: f32) -> El<'a> {
    match handle {
        Some(h) => container(image(h.clone()).width(size).height(size)).width(size).height(size).clip(true).style(move |_| container::Style { border: border(Color::TRANSPARENT, 0.0, size / 2.0), ..Default::default() }).into(),
        None => {
            let letter = if t.symbol.starts_with("USD") { "$".to_string() } else { t.symbol.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()) };
            container(text(letter).size(size * 0.42).font(BOLD).color(Color::WHITE)).width(size).height(size).center(size).style(theme::round_fill(theme::avatar_gradient(&t.symbol), size / 2.0)).into()
        }
    }
}

fn logo_for(logos: &HashMap<String, Option<image::Handle>>, t: &Token) -> Option<image::Handle> {
    t.logo.as_ref().and_then(|u| logos.get(u).cloned().flatten())
}

/// What the window needs from the app.
pub struct View<'a> {
    pub st: &'a State,
    pub entries: Vec<Entry>,
    /// Every token of the network's own list (to spot a custom one using a listed symbol).
    pub listed: &'a [Token],
    pub total: usize,
    pub loading: bool,
    pub chain_name: &'a str,
    /// Pictures by URL (None: in flight or none).
    pub logos: &'a HashMap<String, Option<image::Handle>>,
    /// The wallet the balances are of (None: no EVM wallet picked).
    pub wallet: Option<&'a str>,
    pub explorer: bool,
}

fn list_row<'a>(v: &View<'a>, e: &Entry, on: bool) -> El<'a> {
    let t = &e.token;
    let k = key(&t.address);
    let mut right: Column<'a, Message> = column![].align_x(Alignment::End).spacing(1);
    if let Some(b) = v.st.held(t) {
        right = right.push(text(amount(b, t.decimals, 4)).size(12.5).font(SEMIBOLD).color(TEXT_BODY));
        if let Some(u) = usd(b, t.decimals, v.st.prices.get(&k).copied()) {
            right = right.push(text(u).size(10.5).color(FAINT));
        }
    }
    let mut name_line = row![text(super::ui::truncate(&t.name, 22)).size(11.5).color(TEXT_SECONDARY)].spacing(5).align_y(Alignment::Center);
    if e.custom {
        name_line = name_line.push(container(text("yours").size(9.5).font(SEMIBOLD).color(AMBER_TEXT)).padding([0, 4]).style(theme::badge(rgba(0xf59e0b, 0.1), rgba(0xf59e0b, 0.3))));
    }
    let line = row![
        logo(t, logo_for(v.logos, t).as_ref(), 30.0),
        column![text(t.symbol.clone()).size(13.5).font(BOLD).color(if on { Color::WHITE } else { TEXT_BRIGHT }), name_line].spacing(1).width(Fill),
        right,
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    mouse_area(container(line).width(Fill).padding([7, 10]).style(move |_| container::Style {
        background: on.then(|| rgba(0x6366f1, 0.2).into()),
        border: border(if on { rgba(0x6366f1, 0.45) } else { Color::TRANSPARENT }, 1.0, 9.0),
        ..Default::default()
    }))
    .on_press(msg(TokenPickerMsg::Highlight(k)))
    .on_double_click(msg(TokenPickerMsg::Use))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

fn side<'a>(v: &View<'a>) -> El<'a> {
    let st = v.st;
    let highlighted = st.highlighted.clone().or_else(|| v.entries.first().map(|e| key(&e.token.address)));
    let mut list: Column<'a, Message> = column![].spacing(2);
    if v.loading {
        list = list.push(container(text("Loading tokens…").size(12.5).color(MUTED)).padding(12));
    } else if v.entries.is_empty() {
        list = list.push(container(text(if st.query.trim().is_empty() { "No tokens for this network." } else { "Nothing matches — paste the contract address to add it." }).size(12.5).color(MUTED)).padding(12));
    }
    let mut shown_held = false;
    for e in &v.entries {
        let held = st.held(&e.token).is_some();
        if held && !shown_held {
            list = list.push(container(text("IN YOUR WALLET").size(10).font(BOLD).color(FAINT)).padding(Padding { top: 4.0, right: 0.0, bottom: 4.0, left: 10.0 }));
            shown_held = true;
        } else if !held && shown_held {
            list = list.push(container(text("ALL TOKENS").size(10).font(BOLD).color(FAINT)).padding(Padding { top: 10.0, right: 0.0, bottom: 4.0, left: 10.0 }));
            shown_held = false;
        }
        list = list.push(list_row(v, e, highlighted.as_deref() == Some(&key(&e.token.address)) && st.add.is_none()));
    }
    let count = if v.loading { "loading…".to_string() } else { format!("{} tokens · {}", v.total, v.chain_name) };
    container(
        column![
            column![text("Select a token").size(16).font(BOLD).color(Color::WHITE), text(count).size(11.5).color(TEXT_SECONDARY)].spacing(3),
            row![
                icon(icons::search(), 14, MUTED),
                text_input("Name, symbol or address", &st.query).id(SEARCH).size(13).padding([7, 0]).style(bare_input).on_input(|q| msg(TokenPickerMsg::Query(q))).on_submit(msg(TokenPickerMsg::Use)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding([0, 10]),
            scrollable(list).height(Fill).style(thin_scroll),
            button(row![icon(icons::plus(), 13, TEXT_BODY), text("Add a token by address").size(12.5).font(SEMIBOLD)].spacing(7).align_y(Alignment::Center))
                .width(Fill)
                .padding([8, 12])
                .style(tone_button(Tone::Dark))
                .on_press(msg(TokenPickerMsg::StartAdd)),
        ]
        .spacing(12),
    )
    .width(320)
    .height(Fill)
    .padding([20, 12])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0))
    .into()
}

/// One labelled value of the token card.
fn field<'a>(label: &'a str, value: El<'a>) -> El<'a> {
    row![text(label).size(12.5).color(TEXT_SECONDARY).width(130), value].spacing(10).align_y(Alignment::Center).into()
}

fn mark<'a>(ok: bool) -> El<'a> {
    if ok { icon(icons::circle_check(), 14, GREEN).into() } else { icon(icons::shield_alert(), 14, DANGER_TEXT).into() }
}

/// The contract's own answer next to what the list says.
fn check_rows<'a>(t: &Token, check: Option<&Check>) -> El<'a> {
    let body: El = match check {
        None | Some(Check::Running) => text("Reading the contract…").size(12).color(MUTED).into(),
        Some(Check::Done(Err(e))) => row![mark(false), text(format!("Couldn't read the contract: {e}")).size(12).color(DANGER_TEXT)].spacing(8).into(),
        Some(Check::Done(Ok((name, symbol, decimals)))) => {
            let line = |what: &'a str, listed: String, chain: String, ok: bool| -> El<'a> {
                row![
                    mark(ok),
                    text(what).size(12).color(TEXT_SECONDARY).width(70),
                    text(chain).size(12).font(Font::MONOSPACE).color(TEXT_BODY).width(Fill),
                    text(if ok { "matches".to_string() } else { format!("list says {listed}") }).size(11.5).color(if ok { FAINT } else { DANGER_TEXT }),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            };
            column![
                line("Symbol", t.symbol.clone(), symbol.clone(), symbol.trim().eq_ignore_ascii_case(t.symbol.trim())),
                line("Decimals", t.decimals.to_string(), decimals.to_string(), *decimals == t.decimals),
                line("Name", t.name.clone(), if name.is_empty() { "—".into() } else { name.clone() }, name.is_empty() || name.trim().eq_ignore_ascii_case(t.name.trim())),
            ]
            .spacing(7)
            .into()
        }
    };
    container(body).width(Fill).padding([10, 12]).style(theme::soft_row).into()
}

/// Same symbol as a listed token, different contract: the classic fake.
fn lookalike<'a>(t: &Token, custom: bool, listed: &'a [Token]) -> Option<&'a Token> {
    if !custom {
        return None;
    }
    listed.iter().find(|l| l.symbol.trim().eq_ignore_ascii_case(t.symbol.trim()) && key(&l.address) != key(&t.address))
}

fn detail<'a>(v: &View<'a>, e: &Entry) -> El<'a> {
    let t = &e.token;
    let k = key(&t.address);
    let native = is_native(t);
    let source: El = if native {
        container(text("Network coin").size(11).font(SEMIBOLD).color(TEXT_BODY)).padding([2, 7]).style(theme::badge(rgba(0xffffff, 0.06), BORDER)).into()
    } else if e.custom {
        container(text("Added by you").size(11).font(SEMIBOLD).color(AMBER_TEXT)).padding([2, 7]).style(theme::badge(rgba(0xf59e0b, 0.1), rgba(0xf59e0b, 0.35))).into()
    } else {
        container(text("Accordo list").size(11).font(SEMIBOLD).color(INDIGO_SOFT)).padding([2, 7]).style(theme::badge(rgba(0x6366f1, 0.12), rgba(0x6366f1, 0.35))).into()
    };
    let head = row![
        logo(t, logo_for(v.logos, t).as_ref(), 52.0),
        column![row![text(t.symbol.clone()).size(22).font(BOLD).color(Color::WHITE), source].spacing(10).align_y(Alignment::Center), text(t.name.clone()).size(13).color(TEXT_SECONDARY)].spacing(3),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    let address: El = if native {
        text("— (the network's own coin, no contract)").size(12.5).color(TEXT_SECONDARY).into()
    } else {
        let mut r = row![text(t.address.clone()).size(12).font(Font::MONOSPACE).color(TEXT_BODY)].spacing(6).align_y(Alignment::Center);
        r = r.push(button(icon(icons::copy(), 13, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(TokenPickerMsg::Copy(t.address.clone()))));
        if v.explorer {
            r = r.push(button(icon(icons::external_link(), 13, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(TokenPickerMsg::Explorer(t.address.clone()))));
        }
        r.into()
    };
    let balance: El = match (v.wallet, v.st.balances.get(&k)) {
        _ if v.st.purpose == Purpose::Module => text("Not shown — a module never sees balances").size(12.5).color(FAINT).into(),
        (None, _) => text("Pick an EVM wallet to see it").size(12.5).color(FAINT).into(),
        (Some(_), None) if v.st.balances_loading => text("…").size(12.5).color(MUTED).into(),
        (Some(_), None) => text("—").size(12.5).color(MUTED).into(),
        (Some(_), Some(b)) => {
            let mut r = row![text(format!("{} {}", amount(*b, t.decimals, 6), t.symbol)).size(12.5).font(SEMIBOLD).color(TEXT_BODY)].spacing(8);
            if let Some(u) = usd(*b, t.decimals, v.st.prices.get(&k).copied()) {
                r = r.push(text(u).size(12).color(FAINT));
            }
            r.into()
        }
    };
    let price: El = match v.st.prices.get(&k) {
        Some(p) => text(if *p >= 1.0 { format!("${p:.2}") } else { format!("${p:.6}") }).size(12.5).color(TEXT_BODY).into(),
        None => text("—").size(12.5).color(MUTED).into(),
    };
    let mut col = column![
        head,
        container(space().width(Fill).height(1)).style(rule),
        column![
            field("Contract", address),
            field("Decimals", text(t.decimals.to_string()).size(12.5).color(TEXT_BODY).into()),
            field("Network", text(v.chain_name.to_string()).size(12.5).color(TEXT_BODY).into()),
            field("Your balance", balance),
            field("Price", price),
        ]
        .spacing(10),
    ]
    .spacing(18);
    if !native {
        col = col.push(column![
            row![icon(icons::shield(), 13, TEXT_SECONDARY), text("CHECKED AGAINST THE CONTRACT").size(11).font(BOLD).color(TEXT_SECONDARY)].spacing(7).align_y(Alignment::Center),
            text("Read from the chain right now, not from the list — a token can't pretend to be another one.").size(11.5).color(FAINT),
            check_rows(t, v.st.checks.get(&k)),
        ]
        .spacing(8));
    }
    if let Some(real) = lookalike(t, e.custom, v.listed) {
        col = col.push(
            container(row![icon(icons::shield_alert(), 15, DANGER_TEXT), text(format!("Not the {} from Accordo's list — that one is {}. Make sure this is the token you mean.", real.symbol, short(&real.address))).size(12).color(DANGER_TEXT).line_height(1.4)].spacing(8))
                .width(Fill)
                .padding([10, 12])
                .style(boxed(rgba(0xef4444, 0.08), rgba(0xef4444, 0.35), 10.0)),
        );
    }
    col.into()
}

fn add_panel<'a>(f: &'a AddForm) -> El<'a> {
    let addr = f.address.trim();
    let status: El = match &f.info {
        _ if addr.is_empty() => text("Paste the token's contract address. Its symbol and decimals are read from the contract — they can't be typed in.").size(12).color(FAINT).line_height(1.4).into(),
        _ if crate::evm::client::address(addr).is_err() || addr.len() != 42 => text("Not a contract address yet (0x + 40 characters).").size(12).color(MUTED).into(),
        None => text("Reading the contract…").size(12).color(MUTED).into(),
        Some((a, _)) if !a.eq_ignore_ascii_case(addr) => text("Reading the contract…").size(12).color(MUTED).into(),
        Some((_, Err(e))) => row![mark(false), text(e.clone()).size(12).color(DANGER_TEXT)].spacing(8).into(),
        Some((_, Ok((name, symbol, decimals)))) => column![
            field("Symbol", text(symbol.clone()).size(13).font(BOLD).color(Color::WHITE).into()),
            field("Name", text(if name.is_empty() { "—".into() } else { name.clone() }).size(12.5).color(TEXT_BODY).into()),
            field("Decimals", text(decimals.to_string()).size(12.5).color(TEXT_BODY).into()),
        ]
        .spacing(9)
        .into(),
    };
    let ready = matches!(&f.info, Some((a, Ok(_))) if a.eq_ignore_ascii_case(addr));
    column![
        column![text("Add a token").size(20).font(BOLD).color(Color::WHITE), text("Kept on this computer, for this network.").size(12.5).color(TEXT_SECONDARY)].spacing(3),
        text_input("0x… contract address", &f.address).id(ADD_INPUT).size(13).font(Font::MONOSPACE).padding([9, 11]).style(theme::field).on_input(|a| msg(TokenPickerMsg::AddAddress(a))),
        container(status).width(Fill).padding([10, 12]).style(theme::soft_row),
        row![
            space().width(Fill),
            button(text("Cancel").size(12.5)).padding([7, 14]).style(tone_button(Tone::Dark)).on_press(msg(TokenPickerMsg::AddCancel)),
            button(text("Add token").size(12.5).font(SEMIBOLD)).padding([7, 16]).style(tone_button(Tone::Accent)).on_press_maybe(ready.then(|| msg(TokenPickerMsg::AddSave))),
        ]
        .spacing(8),
    ]
    .spacing(16)
    .into()
}

pub fn view<'a>(v: View<'a>) -> El<'a> {
    let st = v.st;
    let current = st.highlighted.as_ref().and_then(|k| v.entries.iter().find(|e| key(&e.token.address) == *k)).or_else(|| v.entries.first());
    let body: El = match (&st.add, current) {
        (Some(f), _) => add_panel(f),
        (None, Some(e)) => detail(&v, e),
        (None, None) => container(text("Pick a token on the left.").size(13).color(MUTED)).center_x(Fill).padding(40).into(),
    };
    let use_label = current.filter(|_| st.add.is_none()).map(|e| format!("Use {}", e.token.symbol));
    let header = row![
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(TokenPickerMsg::Close)),
    ];
    let footer = row![
        text("↑↓ to move · Enter to use · double-click a row · Esc to close").size(11).color(FAINT),
        space().width(Fill),
        button(text(use_label.clone().unwrap_or_else(|| "Use".into())).size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press_maybe(use_label.map(|_| msg(TokenPickerMsg::Use))),
    ]
    .align_y(Alignment::Center);
    let main = column![
        container(header).padding(Padding { top: 14.0, right: 14.0, bottom: 0.0, left: 26.0 }),
        scrollable(container(body).width(Fill).padding(Padding { top: 0.0, right: 28.0, bottom: 20.0, left: 28.0 })).height(Fill).style(thin_scroll),
        container(space().width(Fill).height(1)).style(rule),
        container(footer).padding([12, 20]),
    ]
    .width(Fill);
    let window = container(row![side(&v), main]).width(Length::Fixed(940.0)).height(Length::Fixed(640.0)).style(modal);
    overlay::modal(window, msg(TokenPickerMsg::Close))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        assert_eq!(amount(U256::from(1_234_567_890_000u64), 6, 4), "1 234 567.89");
        assert_eq!(amount(U256::from(5u8), 6, 4), "0");
        assert_eq!(amount(U256::from(15u8) * U256::from(10u64).pow(U256::from(17u8)), 18, 4), "1.5");
    }

    #[test]
    fn spots_lookalikes() {
        let t = |s: &str, a: &str| Token { symbol: s.into(), name: s.into(), address: a.into(), decimals: 6, logo: None };
        let listed = vec![t("USDC", "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")];
        let fake = t("usdc", "0x1111111111111111111111111111111111111111");
        assert!(lookalike(&fake, true, &listed).is_some());
        assert!(lookalike(&fake, false, &listed).is_none());
        assert!(lookalike(&listed[0], true, &listed).is_none());
    }
}
