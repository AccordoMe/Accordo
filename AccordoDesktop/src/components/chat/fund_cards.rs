// SPDX-License-Identifier: GPL-3.0-or-later
//! Campaigns and drops in the chat (CampaignChatEmbed.tsx, TokenDropChatEmbed.tsx). The message
//! carries only the id: the card loads the campaign / drop live, so a message can't show made-up
//! progress or promise an amount the list doesn't hold.
//!
//! A campaign: its progress, deadline, and Back / the creator's and backers' actions. A drop
//! (drawn by drop_card.rs): what it is and the reader's own shares with Claim — Accordo gives the
//! title, the note and the shares with their proofs, the contract everything else.

// Without both sections most of this file is the full cards, built only with them.
#![cfg_attr(not(all(feature = "drops", feature = "crowdfund")), allow(unused_imports, dead_code))]

use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use iced::widget::{button, container, row, text};
use iced::{Alignment, Color};
use serde_json::Value;

#[cfg(feature = "crowdfund")]
use crate::api::crowdfund::CampaignStatus;
use crate::api::crowdfund::CampaignRead;
use crate::api::drops::{DropInfo, DropSlot};
use crate::app::Message;
use crate::components::ui::{icon, El};
#[cfg(feature = "crowdfund")]
use crate::flows::crowdfund::Action;
use crate::icons;
use crate::pages::chat_page::ChatMsg;
#[cfg(feature = "crowdfund")]
use crate::pages::crowdfund_page::CrowdfundMsg;
use crate::theme::*;

/// What a drop card gets from Accordo: the drop, and the reader's shares (None: no wallet).
pub type DropLoad = (DropInfo, Option<Vec<DropSlot>>);

#[derive(Debug, Clone)]
pub enum FundMsg {
    Campaign(String, Result<Box<CampaignRead>, String>),
    /// Accordo's copy of the drop and the reader's shares in it (None: they have no wallet).
    Drop(String, Result<Box<DropLoad>, String>),
    /// The drop as its contract has it.
    OnChain(String, Result<Box<OnChain>, String>),
    Reload(String),
    /// "Check another wallet" (DropClaimPage.tsx): open / close it.
    CheckOpen(String),
    CheckAddress(String, String),
    Check(String),
    /// (drop, address looked up, its share — None: not in the drop).
    Checked(String, String, Result<Option<Box<DropSlot>>, String>),
}

/// "Check another wallet" on a drop card: whether some address (not one of the reader's) is in
/// the drop. Shows only; claiming needs that wallet.
#[derive(Default)]
pub struct CheckOther {
    pub open: bool,
    pub address: String,
    pub busy: bool,
    pub result: Option<(String, Result<Option<DropSlot>, String>)>,
}

/// A drop card's live data. Once the contract answered, `info` and `slots` carry what it says.
#[derive(Default)]
pub struct DropCard {
    pub info: Option<Result<DropInfo, String>>,
    /// The reader's shares (None: they have no wallet to look up).
    pub slots: Option<Vec<DropSlot>>,
    /// The contract's word (None: not read yet; Err: why not — the card shows Accordo's copy).
    pub chain: Option<Result<(), String>>,
    /// Shares Accordo listed for the reader that the contract doesn't confirm (their addresses):
    /// not shown as theirs.
    pub unproven: Vec<String>,
    /// Loading from Accordo or the contract (Refresh waits).
    pub loading: bool,
    pub check: CheckOther,
}

/// A network as the drop cards read it: where to ask, and Accordo's drops contract there.
#[derive(Debug, Clone, Default)]
pub struct ChainRef {
    pub name: String,
    /// The user's RPCs over the server's (App::rpc_urls).
    pub rpc_urls: Vec<String>,
    pub chain_id: Option<u64>,
    pub native_symbol: String,
    pub drops: Option<Address>,
}

/// A drop as AccordoDrop has it, with the reader's shares checked against its Merkle root.
#[derive(Debug, Clone)]
pub struct OnChain {
    pub creator: Address,
    pub token: Address,
    /// None: the token didn't say (Accordo's symbol stays).
    pub symbol: Option<String>,
    pub decimals: u8,
    pub total: U256,
    pub expires_at: u64,
    pub refunded: bool,
    pub fee_bps: u16,
    /// Paid out so far, per slot whose proof leads to the root.
    pub paid: HashMap<u32, U256>,
    /// The listed shares whose proof doesn't (their addresses).
    pub unproven: Vec<String>,
}

#[derive(Default)]
pub struct Cards {
    pub campaigns: HashMap<String, Option<Result<CampaignRead, String>>>,
    pub drops: HashMap<String, DropCard>,
    /// Networks by name (lowercase, without the "?" some names carry), for reading drops.
    pub chains: HashMap<String, ChainRef>,
}

/// The key `Cards::chains` keeps a network under.
pub fn chain_key(name: &str) -> String {
    name.trim().trim_start_matches('?').to_lowercase()
}

pub fn campaign_id(a: &Value) -> Option<String> {
    a.get("campaign_id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn drop_id(a: &Value) -> Option<String> {
    a.get("drop_id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

pub(super) fn note_line<'a>(t: String) -> El<'a> {
    container(row![icon(icons::info(), 13, MUTED), text(t).size(12.5).color(TEXT_SECONDARY)].spacing(8).align_y(Alignment::Center))
        .width(400)
        .padding([10, 12])
        .style(boxed(rgba(0xffffff, 0.03), BORDER, 10.0))
        .into()
}

#[cfg(not(all(feature = "drops", feature = "crowdfund")))]
/// A card for a section this build doesn't have: what it is, without loading or acting on it.
fn not_built<'a>(what: &str, title: Option<&str>) -> El<'a> {
    let named = title.filter(|t| !t.is_empty()).map(|t| format!(" “{t}”")).unwrap_or_default();
    note_line(format!("{what}{named} — this build of Accordo doesn't include them. Open it on the web."))
}

#[cfg(not(feature = "crowdfund"))]
pub fn campaign<'a>(a: &'a Value, _cards: &'a Cards, _me: &'a str) -> El<'a> {
    not_built("A campaign", a.get("title").and_then(Value::as_str))
}

/// `me`: the reader's username (the creator gets the creator's buttons).
#[cfg(feature = "crowdfund")]
pub fn campaign<'a>(a: &'a Value, cards: &'a Cards, me: &'a str) -> El<'a> {
    let title = a.get("title").and_then(Value::as_str).unwrap_or("");
    let Some(id) = campaign_id(a) else { return note_line("This campaign card has no campaign id".into()) };
    let c = match cards.campaigns.get(&id) {
        None | Some(None) => return note_line(format!("Loading campaign{}…", if title.is_empty() { String::new() } else { format!(" “{title}”") })),
        Some(Some(Err(_))) => return note_line(format!("Campaign{} is unavailable right now.", if title.is_empty() { String::new() } else { format!(" “{title}”") })),
        Some(Some(Ok(c))) => c,
    };
    let mine = c.creator_username.eq_ignore_ascii_case(me);
    let act = |caption: &'a str, x: Action, danger: bool| -> El<'a> {
        button(text(caption).size(12).font(SEMIBOLD)).padding([6, 12]).style(tone_button(if danger { Tone::Danger } else { Tone::Dark })).on_press(Message::Crowdfund(CrowdfundMsg::Act(Box::new(c.clone()), x))).into()
    };
    let mut actions: Vec<El<'a>> = Vec::new();
    match c.status {
        CampaignStatus::Active if !mine && c.hidden != Some(true) => actions.push(
            button(row![icon(icons::flame(), 12, Color::WHITE), text("Back this campaign").size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center))
                .padding([6, 14])
                .style(tone_button(Tone::Accent))
                .on_press(Message::Crowdfund(CrowdfundMsg::Pick(c.campaign_id.clone())))
                .into(),
        ),
        CampaignStatus::Active if mine && c.goal_reached == Some(true) => actions.push(act("End early", Action::EndEarly, false)),
        CampaignStatus::Ended if mine => actions.push(act("Withdraw", Action::Withdraw, false)),
        CampaignStatus::Refunding if !mine => actions.push(act("Get my refund", Action::Refund, false)),
        _ => {}
    }
    actions.push(button(icon(icons::refresh_cw(), 12, MUTED)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::Fund(FundMsg::Reload(id.clone())))).into());
    container(crate::pages::crowdfund_page::campaign_card(c, actions)).max_width(460).into()
}

#[cfg(not(feature = "drops"))]
pub fn drop<'a>(a: &'a Value, _cards: &'a Cards, _wallets: &'a [(String, String)]) -> El<'a> {
    not_built("A token drop", a.get("title").and_then(Value::as_str))
}

/// `wallets`: the reader's wallets (address lowercase, name).
#[cfg(feature = "drops")]
pub fn drop<'a>(a: &'a Value, cards: &'a Cards, wallets: &'a [(String, String)]) -> El<'a> {
    super::drop_card::view(a, cards, wallets)
}

/// Whether the text is a full EVM address (0x + 40 hex).
pub fn is_address(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}
