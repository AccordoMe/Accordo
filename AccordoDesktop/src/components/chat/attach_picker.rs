// SPDX-License-Identifier: GPL-3.0-or-later
//! "Attach to Chat" (ChequeInvoicePickerModal.tsx, CampaignPickerModal.tsx, TokenDropPickerModal)
//! as one desktop window: on the left the lists (My cheques / My invoices / Campaigns / My drops,
//! with counts) and the status filter, on the right the search and the rows with "Attach". The
//! pick goes above the message box and is sent as a card with the next message. A campaign or a
//! drop is sent as its id only: the card loads it live, so a message can't promise numbers.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Length, Padding};
use serde_json::Value;

use crate::api::crowdfund::{CampaignRead, CampaignStatus};
use crate::api::drops::DropInfo;
use crate::app::Message;
use crate::components::finance::records::{self, Filter, ListKind, Lists, Side};
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

pub const SEARCH: &str = "attach-picker-search";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachTab {
    Cheques,
    Invoices,
    Campaigns,
    Drops,
}

impl AttachTab {
    /// The shared list it shows (cheques, invoices); campaigns and drops load here.
    pub fn list(self) -> Option<ListKind> {
        match self {
            AttachTab::Cheques => Some(ListKind::MyCheques),
            AttachTab::Invoices => Some(ListKind::MyInvoices),
            _ => None,
        }
    }

    pub fn module(self) -> crate::modules::Module {
        use crate::modules::Module;
        match self {
            AttachTab::Cheques => Module::Cheques,
            AttachTab::Invoices => Module::Invoices,
            AttachTab::Campaigns => Module::Crowdfund,
            AttachTab::Drops => Module::Drops,
        }
    }
}

pub const TABS: [AttachTab; 4] = [AttachTab::Cheques, AttachTab::Invoices, AttachTab::Campaigns, AttachTab::Drops];

/// What goes into the message: a cheque or an invoice record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceKind {
    Cheque,
    Invoice,
    Campaign,
    Drop,
}

impl FinanceKind {
    /// message_type
    pub fn message_type(self) -> &'static str {
        match self {
            FinanceKind::Cheque => "cheque",
            FinanceKind::Invoice => "invoice",
            FinanceKind::Campaign => "campaign",
            FinanceKind::Drop => "drop",
        }
    }

    /// The text sent when the box is empty (handleSend's defaultText).
    pub fn default_text(self, v: &Value) -> String {
        match self {
            FinanceKind::Cheque => format!("🎁 Attached Cheque for {} {}", records::cheque_amount(v), records::cheque_token(v)),
            FinanceKind::Invoice => format!("🧾 Attached Invoice for {} {}", records::invoice_amount(v), records::invoice_token(v)),
            FinanceKind::Campaign => {
                let t = v.get("title").and_then(Value::as_str).filter(|t| !t.is_empty());
                format!("🔥 Shared a campaign{}", t.map(|t| format!(": \"{t}\"")).unwrap_or_default())
            }
            FinanceKind::Drop => {
                let t = v.get("title").and_then(Value::as_str).filter(|t| !t.is_empty());
                format!("🎁 Drop{} — check if your wallet is in it", t.map(|t| format!(": \"{t}\"")).unwrap_or_default())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AttachMsg {
    Open(AttachTab),
    Close,
    Tab(AttachTab),
    Query(String),
    Filter(Filter),
    Refresh,
    Pick(FinanceKind, Box<Value>),
    Campaigns(Result<Vec<CampaignRead>, String>),
    Drops(Result<Vec<DropInfo>, String>),
}

/// What a 'campaign' message carries (toCampaignAttachment).
pub fn campaign_attachment(c: &CampaignRead) -> Value {
    serde_json::json!({"type": "campaign", "campaign_id": c.campaign_id, "chain_name": c.chain_name, "title": c.title})
}

/// What a 'drop' message carries (toTokenDropAttachment).
pub fn drop_attachment(d: &DropInfo) -> Value {
    let mut v = serde_json::json!({"type": "drop", "drop_id": d.drop_id, "chain": d.chain, "token": d.token});
    if let Some(t) = d.title.as_deref().filter(|t| !t.is_empty()) {
        v["title"] = t.into();
    }
    v
}

pub struct State {
    pub tab: AttachTab,
    pub query: String,
    pub filter: Filter,
    /// Your campaigns and the live ones (CampaignPickerModal).
    pub campaigns: Option<Result<Vec<CampaignRead>, String>>,
    /// Your drops still open.
    pub drops: Option<Result<Vec<DropInfo>, String>>,
}

impl State {
    pub fn new(tab: AttachTab) -> Self {
        Self { tab, query: String::new(), filter: Filter::All, campaigns: None, drops: None }
    }

    pub fn update(&mut self, m: AttachMsg) {
        match m {
            AttachMsg::Tab(t) => self.tab = t,
            AttachMsg::Query(q) => self.query = q,
            AttachMsg::Filter(f) => self.filter = f,
            AttachMsg::Campaigns(r) => self.campaigns = Some(r),
            AttachMsg::Drops(r) => self.drops = Some(r),
            _ => {}
        }
    }
}

fn msg(m: AttachMsg) -> Message {
    Message::Attach(m)
}

fn nav_item<'a>(handle: iced::widget::svg::Handle, tint: Color, label: &'a str, count: Option<usize>, active: bool, on: Message) -> El<'a> {
    let count = count.map(|n| n.to_string()).unwrap_or_else(|| "…".into());
    button(
        row![icon(handle, 15, if active { tint } else { MUTED }), text(label).size(13).font(if active { BOLD } else { SEMIBOLD }).color(if active { Color::WHITE } else { TEXT_BODY }), space().width(Fill), text(count).size(11.5).color(TEXT_SECONDARY)]
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([8, 10])
    .style(move |t, s| {
        if active {
            button::Style { background: Some(Color { a: 0.14, ..tint }.into()), text_color: Color::WHITE, border: border(Color { a: 0.3, ..tint }, 1.0, 8.0), ..Default::default() }
        } else {
            ghost(8.0, rgba(0xffffff, 0.05))(t, s)
        }
    })
    .on_press(on)
    .into()
}

fn filter_item<'a>(label: &'a str, active: bool, f: Filter) -> El<'a> {
    button(row![container(space()).width(7).height(7).style(boxed(if active { INDIGO } else { rgba(0xffffff, 0.15) }, Color::TRANSPARENT, 4.0)), text(label).size(12.5).color(if active { Color::WHITE } else { TEXT_SECONDARY })].spacing(9).align_y(Alignment::Center))
        .width(Fill)
        .padding([6, 10])
        .style(ghost(7.0, rgba(0xffffff, 0.05)))
        .on_press(msg(AttachMsg::Filter(f)))
        .into()
}

fn loading_or<'a, T>(v: &'a Option<Result<Vec<T>, String>>, what: &'a str) -> Result<&'a [T], El<'a>> {
    match v {
        None => Err(container(text(format!("Loading {what}…")).size(13).color(MUTED)).center_x(Fill).padding(40).into()),
        Some(Err(e)) => Err(container(column![text("Couldn't load the list").size(14).font(BOLD).color(DANGER_TEXT), text(e.as_str()).size(12.5).color(TEXT_SECONDARY)].spacing(6).align_x(Alignment::Center)).center_x(Fill).padding(40).into()),
        Some(Ok(l)) => Ok(l.as_slice()),
    }
}

fn nothing<'a>(handle: iced::widget::svg::Handle, title: &'a str, why: String) -> El<'a> {
    container(column![icon(handle, 30, MUTED), text(title).size(14).font(BOLD).color(Color::WHITE), text(why).size(12.5).color(TEXT_SECONDARY)].spacing(8).align_x(Alignment::Center)).center_x(Fill).padding([48, 20]).into()
}

fn simple_row<'a>(title: String, sub: String, badge: (&'static str, Color), face: El<'a>, attach: El<'a>) -> El<'a> {
    container(
        row![
            face,
            column![text(title).size(14).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(3).width(Fill),
            text(badge.0).size(11).font(BOLD).color(badge.1),
            attach,
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([12, 16])
    .style(boxed(rgba(0xffffff, 0.02), BORDER, 12.0))
    .into()
}

fn tile<'a>(t: &str, tint: Color) -> El<'a> {
    container(text(t.to_string()).size(16).font(BOLD).color(tint)).width(40).height(40).center(40).style(boxed(Color { a: 0.15, ..tint }, Color::TRANSPARENT, 10.0)).into()
}

/// `modules`: tabs only for the sections that are on.
pub fn view<'a>(st: &'a State, lists: &'a Lists, now: i64, modules: crate::modules::Modules) -> El<'a> {
    let cheques = lists.get(ListKind::MyCheques);
    let invoices = lists.get(ListKind::MyInvoices);
    let tab = st.tab;
    let (active_l, settled_l) = match tab {
        AttachTab::Cheques => ("Active", "Claimed"),
        AttachTab::Invoices => ("Unpaid", "Paid"),
        AttachTab::Campaigns => ("Live", "Ended"),
        AttachTab::Drops => ("Open", "Ended"),
    };
    let campaigns_n = st.campaigns.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::len);
    let drops_n = st.drops.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::len);
    let (new_label, new_nav) = match tab {
        AttachTab::Cheques => ("Write a new cheque", crate::app::Nav::Cheques),
        AttachTab::Invoices => ("Issue a new invoice", crate::app::Nav::Invoices),
        AttachTab::Campaigns => ("Launch a campaign", crate::app::Nav::Crowdfund),
        AttachTab::Drops => ("Create a drop", crate::app::Nav::Drops),
    };

    let side = container(
        column![
            column![text("Attach to chat").size(16).font(BOLD).color(Color::WHITE), text("Send one as a card").size(11.5).color(TEXT_SECONDARY)].spacing(3),
            Column::with_children(
                [
                    (AttachTab::Cheques, icons::gift(), SKY, "My cheques", cheques.items.as_ref().map(Vec::len)),
                    (AttachTab::Invoices, icons::receipt(), hex(0x10b981), "My invoices", invoices.items.as_ref().map(Vec::len)),
                    (AttachTab::Campaigns, icons::flame(), hex(0xfb923c), "Campaigns", campaigns_n),
                    (AttachTab::Drops, icons::send(), INDIGO_SOFT, "My drops", drops_n),
                ]
                .into_iter()
                .filter(|(t, ..)| modules.shown(t.module()))
                .map(|(t, h, tint, label, n)| nav_item(h, tint, label, n, tab == t, msg(AttachMsg::Tab(t)))),
            )
            .spacing(4),
            column![
                container(text("STATUS").size(10).font(BOLD).color(FAINT)).padding(Padding { top: 0.0, right: 0.0, bottom: 2.0, left: 10.0 }),
                filter_item("All", st.filter == Filter::All, Filter::All),
                filter_item(active_l, st.filter == Filter::Active, Filter::Active),
                filter_item(settled_l, st.filter == Filter::Settled, Filter::Settled),
            ]
            .spacing(2),
            space().height(Fill),
            button(row![icon(icons::plus(), 13, TEXT_BODY), text(new_label).size(12.5).font(SEMIBOLD)].spacing(7).align_y(Alignment::Center))
                .width(Fill)
                .padding([8, 12])
                .style(tone_button(Tone::Dark))
                .on_press(Message::Navigate(new_nav)),
        ]
        .spacing(18),
    )
    .width(260)
    .height(Fill)
    .padding([20, 12])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let busy = match tab.list() {
        Some(k) => lists.get(k).loading,
        None => false,
    };
    let placeholder = match tab {
        AttachTab::Cheques => "Search by token, amount, recipient, note or ID",
        AttachTab::Invoices => "Search by token, amount, client, item, note or ID",
        AttachTab::Campaigns => "Search by title, creator, category or token",
        AttachTab::Drops => "Search by name, token or network",
    };
    let header = row![
        icon(icons::search(), 14, MUTED),
        text_input(placeholder, &st.query).id(SEARCH).size(13).padding([7, 0]).style(bare_input).on_input(|q| msg(AttachMsg::Query(q))),
        button(icon(icons::refresh_cw(), 14, MUTED)).padding(6).style(ghost(7.0, rgba(0xffffff, 0.06))).on_press_maybe((!busy).then(|| msg(AttachMsg::Refresh))),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(AttachMsg::Close)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let attach_btn = |kind: FinanceKind, v: Value, tint: Color| -> El<'a> {
        button(row![icon(icons::paperclip(), 13, tint), text("Attach").size(12.5).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center))
            .padding([6, 12])
            .style(tone_button(Tone::Dark))
            .on_press(msg(AttachMsg::Pick(kind, Box::new(v))))
            .into()
    };
    let q = st.query.trim().to_lowercase();
    let filtered_note = !q.is_empty() || st.filter != Filter::All;
    let body: El = match tab {
        AttachTab::Cheques | AttachTab::Invoices => {
            let is_cheques = tab == AttachTab::Cheques;
            let list = lists.get(tab.list().expect("a shared list"));
            let kind = if is_cheques { FinanceKind::Cheque } else { FinanceKind::Invoice };
            match (&list.items, &list.error) {
                (None, Some(e)) => container(column![text("Couldn't load the list").size(14).font(BOLD).color(DANGER_TEXT), text(e.as_str()).size(12.5).color(TEXT_SECONDARY)].spacing(6).align_x(Alignment::Center)).center_x(Fill).padding(40).into(),
                (None, None) => container(text(if is_cheques { "Loading your cheques…" } else { "Loading your invoices…" }).size(13).color(MUTED)).center_x(Fill).padding(40).into(),
                (Some(items), _) => {
                    let shown: Vec<&Value> = items.iter().filter(|v| if is_cheques { records::cheque_matches(v, &st.query, st.filter) } else { records::invoice_matches(v, &st.query, st.filter) }).collect();
                    if shown.is_empty() {
                        let why = if filtered_note { "Try changing your search or the status filter".to_string() } else if is_cheques { "You haven't created any cheques yet".to_string() } else { "You haven't issued any invoices yet".to_string() };
                        nothing(if is_cheques { icons::gift() } else { icons::receipt() }, if is_cheques { "No cheques found" } else { "No invoices found" }, why)
                    } else {
                        let tint = if is_cheques { SKY } else { hex(0x10b981) };
                        let mut col: Column<'a, Message> = column![].spacing(8);
                        for v in shown {
                            let a = attach_btn(kind, v.clone(), tint);
                            col = col.push(if is_cheques { records::cheque_row(v, Side::Outgoing, now, Some(a)) } else { records::invoice_row(v, Side::Outgoing, now, Some(a)) });
                        }
                        col.into()
                    }
                }
            }
        }
        AttachTab::Campaigns => match loading_or(&st.campaigns, "campaigns") {
            Err(e) => e,
            Ok(list) => {
                let shown: Vec<&CampaignRead> = list
                    .iter()
                    .filter(|c| match st.filter {
                        Filter::All => true,
                        Filter::Active => c.status == CampaignStatus::Active,
                        Filter::Settled => c.status != CampaignStatus::Active,
                    })
                    .filter(|c| q.is_empty() || [&c.title, &c.creator_username, &c.category, &c.token_symbol, &c.chain_name].iter().any(|f| f.to_lowercase().contains(&q)))
                    .collect();
                if shown.is_empty() {
                    nothing(icons::flame(), "No campaigns found", if filtered_note { "Try changing your search or the status filter".into() } else { "Launch one on the Crowdfund page".into() })
                } else {
                    let mut col: Column<'a, Message> = column![].spacing(8);
                    for c in shown {
                        let badge = match c.status {
                            CampaignStatus::Active => ("LIVE", GREEN),
                            CampaignStatus::Ended => ("ENDED", AMBER_TEXT),
                            CampaignStatus::Claimed => ("WITHDRAWN", INDIGO_SOFT),
                            CampaignStatus::Refunding => ("REFUNDING", DANGER_TEXT),
                            CampaignStatus::Unknown => ("", MUTED),
                        };
                        let sub = format!("by @{} · {} · {} / {} {} · {} backers", c.creator_username, c.chain_name, c.raised_amount, c.target_amount, c.token_symbol, c.contributors_count);
                        col = col.push(simple_row(c.title.clone(), sub, badge, tile("🔥", hex(0xfb923c)), attach_btn(FinanceKind::Campaign, campaign_attachment(c), hex(0xfb923c))));
                    }
                    col.into()
                }
            }
        },
        AttachTab::Drops => match loading_or(&st.drops, "your drops") {
            Err(e) => e,
            Ok(list) => {
                let open = |d: &DropInfo| !d.refunded && d.expires_at as i64 > now;
                let shown: Vec<&DropInfo> = list
                    .iter()
                    .filter(|d| match st.filter {
                        Filter::All => true,
                        Filter::Active => open(d),
                        Filter::Settled => !open(d),
                    })
                    .filter(|d| q.is_empty() || [d.title.as_deref().unwrap_or(""), &d.token, &d.chain, d.note.as_deref().unwrap_or("")].iter().any(|f| f.to_lowercase().contains(&q)))
                    .collect();
                if shown.is_empty() {
                    nothing(icons::send(), "No drops found", if filtered_note { "Try changing your search or the status filter".into() } else { "Make one on the Drops page".into() })
                } else {
                    let mut col: Column<'a, Message> = column![].spacing(8);
                    for d in shown {
                        let until = chrono::DateTime::from_timestamp(d.expires_at as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default();
                        let sub = format!("{} {} · {} wallets · {} · until {until}", d.total, d.token, d.recipients_count.unwrap_or(0), d.chain);
                        let title = d.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| format!("{} drop", d.token));
                        let badge = if open(d) { ("OPEN", GREEN) } else if d.refunded { ("CLOSED", MUTED) } else { ("ENDED", AMBER_TEXT) };
                        let initial = d.token.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
                        col = col.push(simple_row(title, sub, badge, tile(&initial, INDIGO_SOFT), attach_btn(FinanceKind::Drop, drop_attachment(d), INDIGO_SOFT)));
                    }
                    col.into()
                }
            }
        },
    };

    let main = column![
        container(header).padding(Padding { top: 14.0, right: 14.0, bottom: 8.0, left: 22.0 }),
        container(space().width(Fill).height(1)).style(rule),
        scrollable(container(body).width(Fill).padding([14, 18])).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    let window = container(row![side, main]).width(Length::Fixed(1000.0)).height(Length::Fixed(640.0)).style(modal);
    overlay::modal(window, msg(AttachMsg::Close))
}
