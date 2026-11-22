// SPDX-License-Identifier: GPL-3.0-or-later
//! Cheques and invoices as the server keeps them (/api/cheques, /api/cheques/my, /api/invoices,
//! /api/invoices/my): one shared cache for the pages and the chat's attach window (the web's
//! useCachedList + listQuery), the lines each list shows, and the search / status filters of
//! ChequeInvoicePickerModal.tsx.

use iced::widget::{button, column, container, row, space, text, Column};
use iced::{Alignment, Color, Fill};
use serde_json::Value;

use crate::app::Message;
use crate::components::finance::status_badge;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    IncomingCheques,
    MyCheques,
    IncomingInvoices,
    MyInvoices,
}

impl ListKind {
    pub fn cheques(self) -> bool {
        matches!(self, ListKind::IncomingCheques | ListKind::MyCheques)
    }
}

#[derive(Debug, Clone)]
pub enum ListsMsg {
    Load(ListKind),
    Loaded(ListKind, Result<Vec<Value>, String>),
    /// A row clicked: its details open under it (again: they close).
    Toggle(ListKind, String),
}

/// One list: None until it was loaded once (then it stays while it refreshes).
#[derive(Default)]
pub struct List {
    pub items: Option<Vec<Value>>,
    pub loading: bool,
    pub error: Option<String>,
    /// The cheque / invoice whose details are open under its row.
    pub open: Option<String>,
}

/// The chat's cheque and invoice cards, for a list's open row: the same card as in the chat
/// (Details, Claim History / Payments, Claim / Pay), from the same cache.
pub struct Cards<'a> {
    pub cheques: &'a std::collections::HashMap<String, crate::components::chat::cheque_card::ChequeView>,
    pub invoices: &'a std::collections::HashMap<String, crate::components::chat::invoice_card::InvoiceView>,
    pub explorers: &'a std::collections::HashMap<String, String>,
    /// The viewer's wallets: (address lowercase, name).
    pub wallets: &'a [(String, String)],
    pub me: &'a str,
}

#[derive(Default)]
pub struct Lists {
    pub incoming_cheques: List,
    pub my_cheques: List,
    pub incoming_invoices: List,
    pub my_invoices: List,
}

impl Lists {
    pub fn get(&self, k: ListKind) -> &List {
        match k {
            ListKind::IncomingCheques => &self.incoming_cheques,
            ListKind::MyCheques => &self.my_cheques,
            ListKind::IncomingInvoices => &self.incoming_invoices,
            ListKind::MyInvoices => &self.my_invoices,
        }
    }

    pub fn get_mut(&mut self, k: ListKind) -> &mut List {
        match k {
            ListKind::IncomingCheques => &mut self.incoming_cheques,
            ListKind::MyCheques => &mut self.my_cheques,
            ListKind::IncomingInvoices => &mut self.incoming_invoices,
            ListKind::MyInvoices => &mut self.my_invoices,
        }
    }

    /// The lists a live event touches (`cheque_created`, `invoice_paid`…), among those loaded.
    pub fn touched_by(&self, event: &str) -> Vec<ListKind> {
        let kinds: &[ListKind] = if event.starts_with("cheque_") {
            &[ListKind::IncomingCheques, ListKind::MyCheques]
        } else if event.starts_with("invoice_") {
            &[ListKind::IncomingInvoices, ListKind::MyInvoices]
        } else {
            &[]
        };
        kinds.iter().copied().filter(|k| self.get(*k).items.is_some()).collect()
    }
}

fn s<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().filter(|x| !x.is_empty())
}

/// A number or a string ("12.5" / 12.5) as text.
fn num(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    match cur {
        Value::String(x) if !x.is_empty() => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn int(v: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_i64().or_else(|| cur.as_f64().map(|f| f as i64)).or_else(|| cur.as_str()?.parse().ok())
}

// ── what a record says (the picker's fallbacks) ──

pub fn cheque_amount(c: &Value) -> String {
    num(c, &["detail", "amount"]).or_else(|| num(c, &["amount"])).unwrap_or_else(|| "0".into())
}
pub fn cheque_token(c: &Value) -> String {
    s(c, &["detail", "token"]).or_else(|| s(c, &["chain"])).unwrap_or("Tokens").to_string()
}
pub fn cheque_chain(c: &Value) -> String {
    s(c, &["chain"]).or_else(|| s(c, &["detail", "chain"])).unwrap_or("EVM").to_string()
}
pub fn cheque_note(c: &Value) -> Option<&str> {
    s(c, &["detail", "note"])
}
pub fn cheque_recipient(c: &Value) -> String {
    s(c, &["recipient_identificator"]).map(|r| r.split('\n').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ")).filter(|r| !r.is_empty()).unwrap_or_else(|| "Anyone with link".into())
}
pub fn cheque_id(c: &Value) -> String {
    num(c, &["cheque_id"]).or_else(|| num(c, &["id"])).unwrap_or_default()
}
fn cheque_status(c: &Value, now: i64) -> String {
    let st = s(c, &["status"]).unwrap_or("pending").to_lowercase();
    let exp = int(c, &["detail", "expires_at"]).unwrap_or(0);
    if exp > 0 && exp < now && st != "claimed" && st != "refunded" { "expired".into() } else { st }
}

pub fn invoice_amount(i: &Value) -> String {
    num(i, &["amount_due"]).or_else(|| num(i, &["detail", "amount"])).unwrap_or_else(|| "0".into())
}
pub fn invoice_token(i: &Value) -> String {
    s(i, &["token_symbol"]).or_else(|| s(i, &["detail", "token"])).unwrap_or("Tokens").to_string()
}
pub fn invoice_chain(i: &Value) -> String {
    s(i, &["chain_name"]).or_else(|| s(i, &["detail", "chain"])).unwrap_or("EVM").to_string()
}
pub fn invoice_item(i: &Value) -> Option<&str> {
    s(i, &["item_name"]).or_else(|| s(i, &["detail", "item_name"]))
}
pub fn invoice_note(i: &Value) -> Option<&str> {
    s(i, &["note"]).or_else(|| s(i, &["detail", "note"]))
}
pub fn invoice_payer(i: &Value) -> String {
    s(i, &["payer_identifier"]).unwrap_or("Direct Client").to_string()
}
pub fn invoice_id(i: &Value) -> String {
    num(i, &["invoice_id"]).or_else(|| num(i, &["id"])).unwrap_or_default()
}
fn invoice_status(i: &Value) -> String {
    s(i, &["status"]).unwrap_or("unpaid").to_lowercase()
}

/// (amount, token, network) of a cheque or an invoice record.
pub fn summary(v: &Value, invoice: bool) -> (String, String, String) {
    if invoice { (invoice_amount(v), invoice_token(v), invoice_chain(v)) } else { (cheque_amount(v), cheque_token(v), cheque_chain(v)) }
}

// ── the picker's filters ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    /// Cheques: not claimed / refunded. Invoices: not paid / cancelled.
    Active,
    Settled,
}

pub fn cheque_matches(c: &Value, q: &str, f: Filter) -> bool {
    let st = s(c, &["status"]).unwrap_or("").to_lowercase();
    let settled = st == "claimed" || st == "refunded";
    match f {
        Filter::Active if settled => return false,
        Filter::Settled if !settled => return false,
        _ => {}
    }
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    [cheque_token(c).to_lowercase(), cheque_amount(c), s(c, &["recipient_identificator"]).unwrap_or("").to_lowercase(), cheque_note(c).unwrap_or("").to_lowercase(), cheque_id(c).to_lowercase()].iter().any(|f| f.contains(&q))
}

pub fn invoice_matches(i: &Value, q: &str, f: Filter) -> bool {
    let st = invoice_status(i);
    let settled = st == "paid" || st == "cancelled";
    match f {
        Filter::Active if settled => return false,
        Filter::Settled if !settled => return false,
        _ => {}
    }
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    [invoice_token(i).to_lowercase(), invoice_amount(i), s(i, &["payer_identifier"]).unwrap_or("").to_lowercase(), invoice_item(i).unwrap_or("").to_lowercase(), invoice_note(i).unwrap_or("").to_lowercase(), invoice_id(i).to_lowercase()]
        .iter()
        .any(|f| f.contains(&q))
}

// ── what can be done ──

/// Not claimed, refunded or expired.
pub fn cheque_claimable(c: &Value, now: i64) -> bool {
    !matches!(cheque_status(c, now).as_str(), "claimed" | "refunded" | "expired")
}

pub fn invoice_payable(i: &Value) -> bool {
    !matches!(invoice_status(i).as_str(), "paid" | "cancelled" | "canceled" | "expired")
}

pub fn claim_button<'a>(c: &Value) -> El<'a> {
    button(text("Claim").size(12.5).font(BOLD)).padding([6, 16]).style(primary).on_press(Message::ClaimCheque(Box::new(c.clone()), None)).into()
}

pub fn pay_button<'a>(i: &Value) -> El<'a> {
    button(text("Pay").size(12.5).font(BOLD)).padding([6, 16]).style(primary).on_press(Message::PayInvoice(Box::new(i.clone()))).into()
}

// ── drawing ──

fn when(iso: Option<&str>) -> String {
    iso.and_then(crate::components::chat::display::local).map(|t| t.format("%b %-d, %Y, %H:%M").to_string()).unwrap_or_default()
}

/// "Expires in 3d 4h" / "Expired" / "No expiry" for a unix time (0: none).
fn until(ts: i64, now: i64, soon: &str, past: &str, none: &str) -> (String, Color) {
    if ts <= 0 {
        return (none.into(), TEXT_SECONDARY);
    }
    let left = ts - now;
    if left <= 0 {
        return (past.into(), DANGER_TEXT);
    }
    let (d, h, m) = (left / 86_400, (left % 86_400) / 3_600, (left % 3_600) / 60);
    let span = if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{}m", m.max(1)) };
    (format!("{soon} {span}"), if left < 86_400 { AMBER_TEXT } else { TEXT_BODY })
}

fn chip<'a>(t: String) -> El<'a> {
    container(text(t).size(11).color(TEXT_SECONDARY)).padding([2, 7]).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 5.0)).into()
}

fn short_id(id: &str) -> String {
    if id.len() > 12 { format!("{}…", &id[..10]) } else { id.to_string() }
}

/// Who's on the other side, for the subtitle line.
pub enum Side {
    /// A list of things sent to me: "From: …".
    Incoming,
    /// Mine: "To: …" / "Billed to: …".
    Outgoing,
}

fn frame<'a>(face: El<'a>, title: El<'a>, sub: El<'a>, mid: El<'a>, badge: El<'a>, action: Option<El<'a>>, tone: Color) -> El<'a> {
    let mut r = row![face, column![title, sub].spacing(3).width(Fill), mid, badge].spacing(14).align_y(Alignment::Center);
    if let Some(a) = action {
        r = r.push(a);
    }
    container(r).width(Fill).padding([12, 16]).style(boxed(rgba(0xffffff, 0.02), tone, 12.0)).into()
}

fn face<'a>(letter: String, done: bool) -> El<'a> {
    container(text(letter).size(16).font(BOLD).color(if done { hex(0x10b981) } else { INDIGO_SOFT }))
        .width(40)
        .height(40)
        .center(40)
        .style(boxed(if done { rgba(0x10b981, 0.15) } else { rgba(0x6366f1, 0.15) }, Color::TRANSPARENT, 10.0))
        .into()
}

fn sub_line<'a>(who_label: &'a str, who: String, id: String, note: Option<&str>) -> El<'a> {
    let mut r = row![text(who_label).size(12).color(TEXT_SECONDARY), text(crate::components::ui::truncate(&who, 40)).size(12).font(SEMIBOLD).color(TEXT_BRIGHT)].spacing(4).align_y(Alignment::Center);
    if !id.is_empty() {
        r = r.push(text("•").size(12).color(FAINT)).push(text(format!("ID: {}", short_id(&id))).size(12).font(iced::Font::MONOSPACE).color(MUTED));
    }
    if let Some(n) = note {
        r = r.push(text("•").size(12).color(FAINT)).push(text(format!("\"{}\"", crate::components::ui::truncate(n, 40))).size(12).color(rgba(0xffffff, 0.6)));
    }
    r.into()
}

pub fn cheque_row<'a>(c: &Value, side: Side, now: i64, action: Option<El<'a>>) -> El<'a> {
    let token = cheque_token(c);
    let status = cheque_status(c, now);
    let done = status == "claimed";
    let tone = match status.as_str() {
        "claimed" => rgba(0x10b981, 0.2),
        "expired" | "refunded" => rgba(0xef4444, 0.2),
        _ => BORDER,
    };
    let title = row![
        text(cheque_amount(c)).size(16).font(BOLD).color(if done { hex(0x10b981) } else { Color::WHITE }),
        text(token.clone()).size(14).font(BOLD).color(ACCENT),
        chip(cheque_chain(c)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let sub = match side {
        Side::Incoming => sub_line("From:", s(c, &["cheque_owner_identificator"]).unwrap_or("—").to_string(), cheque_id(c), cheque_note(c)),
        Side::Outgoing => sub_line("To:", cheque_recipient(c), cheque_id(c), cheque_note(c)),
    };
    let (exp, exp_color) = until(int(c, &["detail", "expires_at"]).unwrap_or(0), now, "Expires in", "Expired", "No expiry");
    let mid = column![text(exp).size(12).color(exp_color), text(when(s(c, &["created_at"]))).size(11).color(TEXT_SECONDARY)].spacing(2).align_x(Alignment::End).width(170);
    let letter = token.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "$".into());
    frame(face(letter, done), title.into(), sub, mid.into(), status_badge::view(status_badge::cheque(&status)), action, tone)
}

pub fn invoice_row<'a>(i: &Value, side: Side, now: i64, action: Option<El<'a>>) -> El<'a> {
    let token = invoice_token(i);
    let status = invoice_status(i);
    let done = status == "paid";
    let tone = match status.as_str() {
        "paid" => rgba(0x10b981, 0.2),
        "overdue" | "cancelled" => rgba(0xef4444, 0.2),
        _ => BORDER,
    };
    let mut title = row![text(invoice_amount(i)).size(16).font(BOLD).color(if done { hex(0x10b981) } else { Color::WHITE }), text(token.clone()).size(14).font(BOLD).color(ACCENT), chip(invoice_chain(i))].spacing(6).align_y(Alignment::Center);
    if let Some(item) = invoice_item(i) {
        title = title.push(container(row![icon(icons::tag(), 10, INDIGO_ICON), text(crate::components::ui::truncate(item, 30)).size(11).color(INDIGO_SOFT)].spacing(4).align_y(Alignment::Center)).padding([2, 7]).style(boxed(rgba(0x6366f1, 0.1), Color::TRANSPARENT, 5.0)));
    }
    let sub = match side {
        Side::Incoming => sub_line("From:", s(i, &["invoice_owner_identifier"]).unwrap_or("—").to_string(), invoice_id(i), invoice_note(i)),
        Side::Outgoing => sub_line("Billed to:", invoice_payer(i), invoice_id(i), invoice_note(i)),
    };
    let due = int(i, &["due_at_timestamp"]).unwrap_or(0);
    let (due_text, due_color) = if done || status == "cancelled" { (String::new(), TEXT_SECONDARY) } else { until(due, now, "Due in", "Overdue", "No due date") };
    let mid = column![text(due_text).size(12).color(due_color), text(when(s(i, &["created_at"]))).size(11).color(TEXT_SECONDARY)].spacing(2).align_x(Alignment::End).width(170);
    frame(face(token.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "$".into()), done), title.into(), sub, mid.into(), status_badge::view(status_badge::invoice(&status)), action, tone)
}

/// A whole list tab: header with the count and a refresh, then the rows (or why there are none).
#[allow(clippy::too_many_arguments)]
pub fn list_view<'a>(list: &'a List, kind: ListKind, title: &'a str, what: &'a str, empty_title: &'a str, empty_text: String, side_incoming: bool, now: i64, cards: &Cards<'a>) -> El<'a> {
    let count = list.items.as_ref().map(|l| format!("{}", l.len())).unwrap_or_default();
    let header = row![
        column![row![text(title).size(20).font(BOLD).color(Color::WHITE), text(count).size(13).color(TEXT_SECONDARY)].spacing(8).align_y(Alignment::Center), text(what).size(13).color(TEXT_SECONDARY)].spacing(4).width(Fill),
        button(row![icon(icons::refresh_cw(), 13, TEXT_BODY), text(if list.loading { "Loading…" } else { "Refresh" }).size(12.5)].spacing(6).align_y(Alignment::Center))
            .padding([6, 12])
            .style(tone_button(Tone::Dark))
            .on_press_maybe((!list.loading).then_some(Message::Lists(ListsMsg::Load(kind)))),
    ]
    .align_y(Alignment::Center);
    let body: El = match (&list.items, &list.error) {
        (None, Some(e)) => container(column![text("Couldn't load this list").size(14).font(BOLD).color(DANGER_TEXT), text(e.as_str()).size(12.5).color(TEXT_SECONDARY)].spacing(6).align_x(Alignment::Center)).center_x(Fill).padding(40).into(),
        (None, None) => container(text("Loading…").size(13).color(MUTED)).center_x(Fill).padding(40).into(),
        (Some(items), _) if items.is_empty() => container(
            column![
                icon(if kind.cheques() { icons::gift() } else { icons::receipt() }, 32, MUTED),
                text(empty_title).size(15).font(BOLD).color(Color::WHITE),
                text(empty_text).size(13).color(TEXT_SECONDARY),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .padding([48, 24])
        .style(boxed(rgba(0xffffff, 0.015), rgba(0xffffff, 0.04), 14.0))
        .into(),
        (Some(items), _) => {
            let mut col: Column<'a, Message> = column![].spacing(10);
            for v in items {
                let side = if side_incoming { Side::Incoming } else { Side::Outgoing };
                let row_el = match kind {
                    ListKind::IncomingCheques => cheque_row(v, side, now, cheque_claimable(v, now).then(|| claim_button(v))),
                    ListKind::IncomingInvoices => invoice_row(v, side, now, invoice_payable(v).then(|| pay_button(v))),
                    ListKind::MyCheques => cheque_row(v, side, now, None),
                    ListKind::MyInvoices => invoice_row(v, side, now, None),
                };
                let id = if kind.cheques() { cheque_id(v) } else { invoice_id(v) };
                if id.is_empty() {
                    col = col.push(row_el);
                    continue;
                }
                // The row opens its card (the Claim / Pay button in it still does its own thing).
                col = col.push(iced::widget::mouse_area(row_el).on_press(Message::Lists(ListsMsg::Toggle(kind, id.clone()))).interaction(iced::mouse::Interaction::Pointer));
                if list.open.as_deref() == Some(id.as_str()) {
                    let card: El<'a> = if kind.cheques() {
                        let ctx = crate::components::chat::cheque_card::CardCtx { cheques: cards.cheques, explorers: cards.explorers, wallets: cards.wallets };
                        crate::components::chat::cheque_card::view(v, &ctx)
                    } else {
                        let ctx = crate::components::chat::invoice_card::CardCtx { invoices: cards.invoices, explorers: cards.explorers, me: cards.me, mine: kind == ListKind::MyInvoices };
                        crate::components::chat::invoice_card::view(v, &ctx)
                    };
                    col = col.push(container(card).padding(iced::Padding { left: 54.0, ..Default::default() }));
                }
            }
            col.into()
        }
    };
    let mut out = column![header].spacing(18);
    if let (Some(_), Some(e)) = (&list.items, &list.error) {
        out = out.push(text(format!("Couldn't refresh: {e}")).size(12).color(DANGER_TEXT));
    }
    out = out.push(body);
    container(out.push(space().height(4))).width(Fill).padding(24).style(boxed(CARD, BORDER, 14.0)).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filters_like_the_picker() {
        let c = json!({"status": "Claimed", "recipient_identificator": "bob\nalice", "detail": {"amount": "5", "token": "USDC", "note": "Logo work"}, "cheque_id": "0xabc"});
        assert!(cheque_matches(&c, "", Filter::Settled));
        assert!(!cheque_matches(&c, "", Filter::Active));
        assert!(cheque_matches(&c, "usdc", Filter::All));
        assert!(cheque_matches(&c, "logo", Filter::All));
        assert!(cheque_matches(&c, "alice", Filter::All));
        assert!(!cheque_matches(&c, "eth", Filter::All));
        assert_eq!(cheque_recipient(&c), "bob, alice");

        let i = json!({"status": "UNPAID", "amount_due": 12.5, "token_symbol": "ETH", "item_name": "Design", "payer_identifier": "zuko"});
        assert!(invoice_matches(&i, "", Filter::Active));
        assert!(invoice_matches(&i, "12.5", Filter::All));
        assert!(invoice_matches(&i, "design", Filter::All));
        assert_eq!(invoice_amount(&i), "12.5");
    }

    #[test]
    fn expired_cheques() {
        let c = json!({"status": "pending", "detail": {"expires_at": 100}});
        assert_eq!(cheque_status(&c, 200), "expired");
        assert_eq!(cheque_status(&c, 50), "pending");
        assert_eq!(cheque_status(&json!({"status": "Claimed", "detail": {"expires_at": 100}}), 200), "claimed");
    }

    #[test]
    fn live_events() {
        let mut l = Lists::default();
        assert!(l.touched_by("cheque_created").is_empty());
        l.my_cheques.items = Some(vec![]);
        assert_eq!(l.touched_by("cheque_claimed"), vec![ListKind::MyCheques]);
        assert!(l.touched_by("invoice_paid").is_empty());
    }
}
