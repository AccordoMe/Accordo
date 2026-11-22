// SPDX-License-Identifier: GPL-3.0-or-later
//! Networks window (ChainSettingsModal.tsx, desktop-style): the gear next to the network picker
//! opens it on the network in use. Networks on the left (search, mainnets / testnets / local),
//! the picked one's settings on the right: your own RPC endpoints over the server's, a block
//! explorer, an endpoint check, and the faucet on a local chain. Kept on this computer only —
//! the same `chainOverrides` the web keeps in its local storage.

use std::collections::HashMap;

use iced::widget::{button, column, container, row, scrollable, space, text, text_editor, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Length, Padding};

use super::overlay;
use super::ui::{icon, El};
use crate::api::data::{Chain, Token};
use crate::app::Message;
use crate::chain_rpc::{Drip, Probe};
use crate::icons;
use crate::theme::{self, *};

/// More than this many RPC URLs is surely a paste gone wrong.
pub const MAX_RPCS: usize = 10;
const MAX_URL: usize = 500;

#[derive(Debug, Clone)]
pub enum ChainSettingsMsg {
    Open,
    Close,
    Pick(String),
    Query(String),
    Rpc(text_editor::Action),
    Explorer(String),
    Test,
    Probed(String, Result<Probe, String>),
    Save,
    Discard,
    Reset,
    Use,
    Drip(Drip),
    Dripped(Result<String, String>),
    FaucetTokens(String, Vec<Token>),
    /// A contract's address to the clipboard.
    CopyContract(String),
    /// A contract on the explorer (the one set here, else the network's).
    OpenContract(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeState {
    Running,
    Done(Result<Probe, String>),
}

pub struct State {
    pub chain_id: String,
    pub query: String,
    pub rpc: text_editor::Content,
    pub explorer: String,
    /// Edits of other networks not saved yet: (rpc text, explorer), kept while the window is open.
    pub drafts: HashMap<String, (String, String)>,
    /// Endpoint checks by URL (kept across networks: a URL is a URL).
    pub probes: HashMap<String, ProbeState>,
    /// Test tokens of the local chain, for the faucet: (chain id, tokens).
    pub faucet_tokens: Option<(String, Vec<Token>)>,
    /// The drip in flight (its label).
    pub dripping: Option<String>,
}

/// What was saved for a network: (RPC URLs one per line, explorer) — "" = the defaults.
pub type Saved = (String, String);

impl State {
    pub fn new(chain_id: String, saved: Saved) -> Self {
        Self {
            chain_id,
            query: String::new(),
            rpc: text_editor::Content::with_text(&saved.0),
            explorer: saved.1,
            drafts: HashMap::new(),
            probes: HashMap::new(),
            faucet_tokens: None,
            dripping: None,
        }
    }

    /// Switches to another network, keeping this one's unsaved edits.
    pub fn pick(&mut self, chain_id: String, saved_now: &Saved, saved_next: Saved) {
        if self.dirty(saved_now) {
            self.drafts.insert(self.chain_id.clone(), (self.rpc.text(), self.explorer.clone()));
        } else {
            self.drafts.remove(&self.chain_id);
        }
        let (rpc, explorer) = self.drafts.remove(&chain_id).unwrap_or(saved_next);
        self.rpc = text_editor::Content::with_text(&rpc);
        self.explorer = explorer;
        self.chain_id = chain_id;
    }

    /// Restores the saved values of the current network.
    pub fn load(&mut self, saved: Saved) {
        self.rpc = text_editor::Content::with_text(&saved.0);
        self.explorer = saved.1;
        self.drafts.remove(&self.chain_id);
    }

    /// The RPC URLs typed, cleaned (blank lines and spaces dropped).
    pub fn rpc_lines(&self) -> Vec<String> {
        lines(&self.rpc.text())
    }

    pub fn dirty(&self, saved: &Saved) -> bool {
        self.rpc_lines() != lines(&saved.0) || self.explorer.trim() != saved.1.trim()
    }

    /// What's wrong with the form, if anything.
    pub fn problem(&self) -> Option<String> {
        let rpcs = self.rpc_lines();
        if rpcs.len() > MAX_RPCS {
            return Some(format!("Up to {MAX_RPCS} RPC URLs"));
        }
        if let Some(bad) = rpcs.iter().find(|u| !valid_url(u, true)) {
            return Some(format!("Not an RPC URL: {}", short(bad)));
        }
        let ex = self.explorer.trim();
        if !ex.is_empty() && !valid_url(ex, false) {
            return Some("The explorer must be an http(s):// address".into());
        }
        None
    }
}

pub fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

fn valid_url(u: &str, ws_ok: bool) -> bool {
    let schemes: &[&str] = if ws_ok { &["http://", "https://", "ws://", "wss://"] } else { &["http://", "https://"] };
    u.len() <= MAX_URL && !u.contains(char::is_whitespace) && schemes.iter().any(|s| u.len() > s.len() && u.to_ascii_lowercase().starts_with(s))
}

fn short(u: &str) -> String {
    if u.chars().count() > 48 { format!("{}…", u.chars().take(47).collect::<String>()) } else { u.to_string() }
}

fn msg(m: ChainSettingsMsg) -> Message {
    Message::ChainSettings(m)
}

/// What the window needs from the app.
pub struct View<'a> {
    pub st: &'a State,
    pub chains: &'a [Chain],
    /// The network in use (top bar).
    pub active: &'a str,
    /// Networks the user changed (have overrides).
    pub custom: &'a HashMap<String, serde_json::Value>,
    pub saved: Saved,
    /// Where the faucet sends: the active EVM wallet's address.
    pub faucet_to: Option<&'a str>,
}

fn nav<'a>(v: &View<'a>) -> El<'a> {
    let q = v.st.query.trim().to_lowercase();
    let hit = |c: &&Chain| q.is_empty() || c.name.to_lowercase().contains(&q) || c.id.contains(&q);
    let groups: [(&str, Vec<&Chain>); 3] = [
        ("MAINNETS", v.chains.iter().filter(|c| !c.testnet && !c.local).filter(hit).collect()),
        ("TESTNETS", v.chains.iter().filter(|c| c.testnet && !c.local).filter(hit).collect()),
        ("LOCAL", v.chains.iter().filter(|c| c.local).filter(hit).collect()),
    ];
    let mut list: Column<'a, Message> = column![].spacing(2);
    for (title, chains) in groups {
        if chains.is_empty() {
            continue;
        }
        list = list.push(container(text(title).size(10.5).font(BOLD).color(FAINT)).padding(Padding { top: 10.0, right: 0.0, bottom: 4.0, left: 10.0 }));
        for c in chains {
            let on = c.id == v.st.chain_id;
            let mut line = row![
                column![
                    text(c.name.clone()).size(13).font(SEMIBOLD).color(if on { Color::WHITE } else { TEXT_BODY }),
                    text(format!("ID {}", c.id)).size(10.5).color(FAINT),
                ]
                .spacing(1)
                .width(Fill),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            if v.st.drafts.contains_key(&c.id) || (on && v.st.dirty(&v.saved)) {
                line = line.push(container(space().width(6).height(6)).style(theme::round_fill(AMBER.into(), 4.0))); // unsaved
            } else if v.custom.contains_key(&c.id) {
                line = line.push(container(text("custom").size(9.5).font(SEMIBOLD).color(INDIGO_SOFT)).padding([1, 5]).style(theme::badge(rgba(0x6366f1, 0.14), rgba(0x6366f1, 0.3))));
            }
            if c.id == v.active {
                line = line.push(container(space().width(7).height(7)).style(theme::round_fill(GREEN.into(), 4.0)));
            }
            list = list.push(
                button(line).width(Fill).padding([7, 10]).style(selectable(on, rgba(0x6366f1, 0.22), 8.0)).on_press(msg(ChainSettingsMsg::Pick(c.id.clone()))),
            );
        }
    }
    if v.chains.is_empty() {
        list = list.push(container(text("Loading networks…").size(12).color(MUTED)).padding(10));
    }
    container(
        column![
            column![text("Networks").size(16).font(BOLD).color(Color::WHITE), text("RPC and explorer · this computer only").size(11.5).color(TEXT_SECONDARY)].spacing(3),
            text_input("Search networks", &v.st.query).size(12.5).padding([7, 10]).style(field).on_input(|q| msg(ChainSettingsMsg::Query(q))),
            scrollable(list).height(Fill).style(thin_scroll),
            row![container(space().width(7).height(7)).style(theme::round_fill(GREEN.into(), 4.0)), text("in use").size(10.5).color(FAINT), space().width(8), container(space().width(6).height(6)).style(theme::round_fill(AMBER.into(), 4.0)), text("not saved").size(10.5).color(FAINT)]
                .spacing(5)
                .align_y(Alignment::Center),
        ]
        .spacing(12),
    )
    .width(250)
    .height(Fill)
    .padding([22, 14])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0))
    .into()
}

fn section<'a>(title: &'a str, hint: &'a str, body: El<'a>) -> El<'a> {
    column![
        column![text(title).size(11).font(BOLD).color(TEXT_SECONDARY), text(hint).size(11.5).color(FAINT).line_height(1.4)].spacing(3),
        body
    ]
    .spacing(9)
    .into()
}

/// One endpoint with its check result.
fn endpoint<'a>(url: &str, probe: Option<&ProbeState>, default: bool) -> El<'a> {
    let status: El = match probe {
        None => space().into(),
        Some(ProbeState::Running) => text("checking…").size(11.5).color(MUTED).into(),
        Some(ProbeState::Done(Ok(p))) => {
            let color = if p.ms < 300 { GREEN } else if p.ms < 1000 { AMBER } else { DANGER_TEXT };
            row![
                container(space().width(6).height(6)).style(theme::round_fill(color.into(), 4.0)),
                text(match p.height {
                    Some(h) => format!("{} ms · #{h}", p.ms),
                    None => format!("{} ms", p.ms),
                })
                .size(11.5)
                .color(TEXT_BODY),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        }
        Some(ProbeState::Done(Err(e))) => text(e.clone()).size(11.5).color(DANGER_TEXT).into(),
    };
    let mut line = row![text(short(url)).size(12).font(Font::MONOSPACE).color(if default { TEXT_SECONDARY } else { TEXT_BODY }).width(Fill)].spacing(10).align_y(Alignment::Center);
    if default {
        line = line.push(text("default").size(10.5).color(FAINT));
    }
    container(line.push(status)).width(Fill).padding([7, 12]).style(theme::soft_row).into()
}

pub(crate) fn editor_style(_: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: hex(0x0e0f17).into(),
        border: border(
            match status {
                text_editor::Status::Focused { .. } => ACCENT,
                text_editor::Status::Hovered => BORDER_STRONG,
                _ => BORDER,
            },
            1.0,
            8.0,
        ),
        placeholder: hex(0x6b7080),
        value: TEXT,
        selection: rgba(0x5865f2, 0.45),
    }
}

fn faucet<'a>(v: &View<'a>, chain: &'a Chain) -> El<'a> {
    let st = v.st;
    let busy = st.dripping.is_some();
    let drip_btn = |d: Drip| -> El<'a> {
        let label = d.label(chain.solana);
        let running = st.dripping.as_deref() == Some(label.as_str());
        button(row![icon(icons::droplets(), 13, hex(0x7dd3fc)), text(if running { "Sending…".to_string() } else { label }).size(12).font(SEMIBOLD).color(hex(0x7dd3fc))].spacing(6).align_y(Alignment::Center))
            .padding([7, 12])
            .style(move |_, status| iced::widget::button::Style {
                background: Some(rgba(0x38bdf8, if matches!(status, iced::widget::button::Status::Hovered) { 0.18 } else { 0.1 }).into()),
                border: border(rgba(0x38bdf8, 0.3), 1.0, 8.0),
                text_color: hex(0x7dd3fc),
                ..Default::default()
            })
            .on_press_maybe((!busy && v.faucet_to.is_some()).then(|| msg(ChainSettingsMsg::Drip(d))))
            .into()
    };
    let mut buttons = row![drip_btn(Drip::Native { symbol: chain.native_symbol.clone() })].spacing(8);
    if !chain.solana {
        if let Some((id, tokens)) = &st.faucet_tokens {
            if *id == chain.id {
                for t in tokens {
                    buttons = buttons.push(drip_btn(Drip::Token { symbol: t.symbol.clone(), address: t.address.clone(), decimals: t.decimals }));
                }
            }
        }
    }
    let who: El = match v.faucet_to {
        Some(a) => row![text("Test coins for").size(11.5).color(TEXT_SECONDARY), text(crate::wallets::short(a)).size(11.5).font(Font::MONOSPACE).color(TEXT_BODY), text("on this local chain only.").size(11.5).color(TEXT_SECONDARY)].spacing(5).into(),
        None if chain.solana => text("Solana wallets aren't in the desktop app yet — use the web app's faucet.").size(11.5).color(AMBER_TEXT).into(),
        None => text("Pick an EVM wallet in the wallet drawer (Connect) first.").size(11.5).color(AMBER_TEXT).into(),
    };
    container(column![row![icon(icons::droplets(), 14, hex(0x7dd3fc)), text("Faucet").size(13.5).font(SEMIBOLD).color(Color::WHITE)].spacing(7).align_y(Alignment::Center), who, buttons.wrap()].spacing(10))
        .width(Fill)
        .padding(14)
        .style(boxed(rgba(0x38bdf8, 0.05), rgba(0x38bdf8, 0.2), 12.0))
        .into()
}

fn body<'a>(v: &View<'a>, chain: &'a Chain) -> El<'a> {
    let st = v.st;
    let mine = st.rpc_lines();
    let mut rpc_list: Column<'a, Message> = column![].spacing(5);
    for u in &mine {
        rpc_list = rpc_list.push(endpoint(u, st.probes.get(u), false));
    }
    for u in &chain.rpc_urls {
        rpc_list = rpc_list.push(endpoint(u, st.probes.get(u), true));
    }
    if mine.is_empty() && chain.rpc_urls.is_empty() {
        rpc_list = rpc_list.push(text("The server lists no RPC for this network.").size(12).color(MUTED));
    }
    let testing = st.probes.values().any(|p| *p == ProbeState::Running);
    let in_use = if mine.is_empty() { "In use: the defaults." } else { "In use: yours (the defaults are skipped)." };
    let rpc = section(
        "RPC ENDPOINTS",
        "Your own URLs, one per line — they replace the server's. Leave empty for the defaults. http(s):// or ws(s)://.",
        column![
            text_editor(&st.rpc)
                .placeholder("https://my-node.example.com\nhttps://rpc.ankr.com/…")
                .height(96)
                .size(12.5)
                .font(Font::MONOSPACE)
                .padding(10)
                .style(editor_style)
                .on_action(|a| msg(ChainSettingsMsg::Rpc(a))),
            row![
                button(row![icon(icons::activity(), 13, Color::WHITE), text(if testing { "Testing…" } else { "Test endpoints" }).size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                    .padding([6, 12])
                    .style(tone_button(Tone::Dark))
                    .on_press_maybe((!testing).then(|| msg(ChainSettingsMsg::Test))),
                text(format!("Asks each for its chain ID and latest block. {in_use}")).size(11.5).color(FAINT),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            rpc_list,
        ]
        .spacing(9)
        .into(),
    );
    let explorer = section(
        "BLOCK EXPLORER",
        "Where links to transactions and addresses open. Empty: the default.",
        text_input(chain.explorer.as_deref().unwrap_or("https://…"), &st.explorer).size(12.5).font(Font::MONOSPACE).padding([8, 10]).style(field).on_input(|e| msg(ChainSettingsMsg::Explorer(e))).into(),
    );
    let mut col = column![rpc, explorer, contracts(chain)].spacing(24);
    if chain.local {
        col = col.push(faucet(v, chain));
    }
    col.into()
}

/// What Accordo's contract keys (chain.data.json) are, in the order they're shown.
const CONTRACTS: [(&str, &str); 11] = [
    ("AccordoCheques", "Cheques · swaps"),
    ("AccordoTask", "Task cheques · gasless claims"),
    ("AccordoDrop", "Token drops"),
    ("AccordoInvoice", "Invoices"),
    ("AccordoInvoices", "Invoices"),
    ("AccordoCrowdfund", "Crowdfunding"),
    ("AccordoTreasury", "Treasury · fees"),
    ("AccordoMinter", "Token minter"),
    ("AccordoCollectionFactory", "NFT collections"),
    ("AccordoGenesis", "Genesis NFT"),
    ("Multicall3", "Multicall3"),
];

/// The network's contracts, as the server lists them: known ones first, in CONTRACTS' order.
fn contracts<'a>(chain: &'a Chain) -> El<'a> {
    let mut list: Vec<(&str, &str, &str)> = chain
        .contracts
        .iter()
        .map(|(k, a)| (CONTRACTS.iter().find(|(key, _)| key == k).map_or(k.as_str(), |(_, l)| *l), k.as_str(), a.as_str()))
        .collect();
    list.sort_by_key(|(_, k, _)| (CONTRACTS.iter().position(|(key, _)| key == k).unwrap_or(usize::MAX), k.to_string()));
    let mut rows = column![].spacing(6);
    for (label, key, addr) in list {
        let a = addr.to_string();
        rows = rows.push(
            container(
                row![
                    column![
                        row![text(label).size(12.5).font(SEMIBOLD).color(TEXT_BODY), text(key).size(10.5).font(Font::MONOSPACE).color(FAINT)].spacing(8).align_y(Alignment::Center),
                        text(addr).size(11.5).font(Font::MONOSPACE).color(TEXT_SECONDARY),
                    ]
                    .spacing(2)
                    .width(Fill),
                    button(icon(icons::copy(), 13, MUTED)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(ChainSettingsMsg::CopyContract(a.clone()))),
                    button(icon(icons::external_link(), 13, MUTED)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(ChainSettingsMsg::OpenContract(a))),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .style(boxed(rgba(0xffffff, 0.02), BORDER, 8.0)),
        );
    }
    if chain.contracts.is_empty() {
        rows = rows.push(text("No Accordo contracts on this network yet — cheques, invoices and the rest aren't available here.").size(12).color(FAINT));
    }
    section("CONTRACTS", "Accordo's contracts on this network, as the server lists them (chain.data.json). Read only.", rows.into())
}

fn header<'a>(v: &View<'a>, chain: &'a Chain) -> El<'a> {
    let kind = if chain.local { "Local" } else if chain.testnet { "Testnet" } else { "Mainnet" };
    let using: El = if chain.id == v.active {
        container(row![container(space().width(7).height(7)).style(theme::round_fill(GREEN.into(), 4.0)), text("In use").size(11.5).font(SEMIBOLD).color(TEXT_BODY)].spacing(6).align_y(Alignment::Center))
            .padding([5, 10])
            .style(theme::badge(rgba(0x22c55e, 0.08), rgba(0x22c55e, 0.25)))
            .into()
    } else {
        button(text("Use this network").size(12).font(SEMIBOLD)).padding([6, 12]).style(tone_button(Tone::Accent)).on_press(msg(ChainSettingsMsg::Use)).into()
    };
    row![
        column![
            text(chain.name.clone()).size(18).font(BOLD).color(Color::WHITE),
            text(format!("Chain ID {} · {} · {kind}{}", chain.id, chain.native_symbol, if chain.solana { " · Solana" } else { "" })).size(12).color(TEXT_SECONDARY),
        ]
        .spacing(2),
        space().width(Fill),
        using,
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(ChainSettingsMsg::Close)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn footer<'a>(v: &View<'a>, chain: &'a Chain) -> El<'a> {
    let dirty = v.st.dirty(&v.saved);
    let problem = v.st.problem();
    let custom = v.custom.contains_key(&chain.id);
    let left: El = button(row![icon(icons::rotate_ccw(), 13, TEXT_BODY), text("Reset to defaults").size(12)].spacing(6).align_y(Alignment::Center))
        .padding([6, 12])
        .style(tone_button(Tone::Dark))
        .on_press_maybe(custom.then(|| msg(ChainSettingsMsg::Reset)))
        .into();
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    if let Some(p) = &problem {
        right = right.push(text(p.clone()).size(11.5).color(DANGER_TEXT));
    } else if dirty {
        right = right.push(text("Unsaved changes").size(11.5).color(AMBER_TEXT));
    }
    right = right
        .push(button(text("Discard").size(12)).padding([6, 12]).style(tone_button(Tone::Dark)).on_press_maybe(dirty.then(|| msg(ChainSettingsMsg::Discard))))
        .push(button(text("Save").size(12).font(SEMIBOLD)).padding([6, 16]).style(tone_button(Tone::Accent)).on_press_maybe((dirty && problem.is_none()).then(|| msg(ChainSettingsMsg::Save))));
    row![left, space().width(Fill), right].align_y(Alignment::Center).into()
}

pub fn view<'a>(v: View<'a>) -> El<'a> {
    let side = nav(&v);
    let main: El = match v.chains.iter().find(|c| c.id == v.st.chain_id) {
        Some(chain) => column![
            container(header(&v, chain)).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
            container(space().width(Fill).height(1)).style(rule),
            scrollable(container(body(&v, chain)).width(Fill).padding(Padding { top: 18.0, right: 26.0, bottom: 24.0, left: 26.0 })).height(Fill).style(thin_scroll),
            container(space().width(Fill).height(1)).style(rule),
            container(footer(&v, chain)).padding([12, 20]),
        ]
        .width(Fill)
        .into(),
        None => column![
            container(row![space().width(Fill), button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(ChainSettingsMsg::Close))]).padding(20),
            container(text("Pick a network on the left.").size(13).color(MUTED)).center_x(Fill).padding(40),
        ]
        .width(Fill)
        .into(),
    };
    let window = container(row![side, main]).width(Length::Fixed(920.0)).height(Length::Fixed(640.0)).style(modal);
    overlay::modal(window, msg(ChainSettingsMsg::Close))
}
