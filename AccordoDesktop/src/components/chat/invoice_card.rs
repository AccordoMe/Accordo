// SPDX-License-Identifier: GPL-3.0-or-later
//! An invoice in the chat (InvoiceDetailsCard.tsx, chat mode): the "Accordo Invoice Inspector" —
//! amount due, id, the item, how the fee splits, dates with a countdown to the due date, the
//! merchant and the client, the PIN (for its creator, or once paid), the note; the payment
//! history in its own tab; Pay for everyone else. What the message carries is shown at once; the
//! server's copy (/api/invoices/{id}) is loaded next to it and wins where it says more.

use std::collections::HashMap;

use chrono::{Local, TimeZone};
use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, space, text, Column};
use iced::{Alignment, Color, Fill, Font, Padding};
use serde_json::Value;

pub use super::cheque_card::Tab;
use super::cheque_card::{amount, copy_btn, label, left, line, merged, num, panel, s, short, when, ChequeMsg};
use crate::app::Message;
use crate::components::finance::status_badge;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

#[derive(Debug, Clone)]
pub enum InvoiceMsg {
    Tab(String, Tab),
    Loaded(String, Result<Value, String>),
    Payments(String, Result<Vec<Value>, String>),
    Refresh(String),
}

#[derive(Default)]
pub struct InvoiceView {
    pub tab: Tab,
    pub live: Option<Value>,
    pub payments: Option<Vec<Value>>,
    pub loading: bool,
}

pub struct CardCtx<'a> {
    pub invoices: &'a HashMap<String, InvoiceView>,
    pub explorers: &'a HashMap<String, String>,
    /// The viewer's username (the creator sees the PIN and gets no Pay button).
    pub me: &'a str,
    /// The message is the viewer's own.
    pub mine: bool,
}

fn m(msg: InvoiceMsg) -> Message {
    Message::Chat(ChatMsg::Invoice(msg))
}

pub fn invoice_id(data: &Value) -> Option<String> {
    s(data, "invoice_id").or_else(|| data.get("detail").and_then(|d| s(d, "invoiceId"))).map(str::to_string)
}

/// Its due date (unix): due_at_timestamp, else the ISO due_date.
fn due_of(c: &Value, detail: &Value) -> i64 {
    num(c, "due_at_timestamp").or_else(|| num(detail, "due_at_timestamp")).map(|x| x as i64).filter(|x| *x > 0).or_else(|| s(c, "due_date").and_then(super::display::local).map(|t| t.timestamp())).unwrap_or(0)
}

/// Does showing it need a clock (a countdown to the due date)?
pub fn is_live(data: &Value) -> bool {
    let empty = Value::Object(Default::default());
    let detail = data.get("detail").unwrap_or(&empty);
    let st = s(data, "status").unwrap_or("").to_lowercase();
    due_of(data, detail) > 0 && !matches!(st.as_str(), "paid" | "cancelled" | "canceled")
}

fn item_icon(kind: Option<&str>) -> &'static str {
    match kind.unwrap_or("").to_lowercase().as_str() {
        "digital" | "file" | "download" => "💾",
        "service" => "🛠",
        "subscription" => "🔁",
        "physical" | "product" => "📦",
        "ticket" | "event" => "🎟",
        _ => "🏷",
    }
}

pub fn view<'a>(data: &'a Value, ctx: &CardCtx<'a>) -> El<'a> {
    let id = invoice_id(data).unwrap_or_default();
    let state = ctx.invoices.get(&id);
    let c = merged(data, state.and_then(|v| v.live.as_ref()));
    let empty = Value::Object(Default::default());
    let detail = c.get("detail").unwrap_or(&empty).clone();

    let status = s(&c, "status").unwrap_or("UNPAID").to_lowercase();
    let paid_flag = status == "paid";
    let cancelled = matches!(status.as_str(), "cancelled" | "canceled");
    let due = due_of(&c, &detail);
    let overdue = due > 0 && Local::now().timestamp() > due && !paid_flag && !cancelled;
    let shown_status = if overdue { "overdue".to_string() } else { status.clone() };

    let token = s(&c, "token_symbol").or_else(|| s(&detail, "token")).unwrap_or("TOKEN").to_string();
    let chain = s(&c, "chain_name").or_else(|| s(&detail, "chain")).or_else(|| s(&c, "chain")).unwrap_or("EVM").to_string();
    let total = num(&c, "amount_due").or_else(|| num(&detail, "amount")).unwrap_or(0.0);
    let paid = num(&c, "amount_paid").unwrap_or(if paid_flag { total } else { 0.0 });
    let fee_pct = num(&c, "fee_percent").or_else(|| num(&detail, "fee_percent")).or_else(|| num(&c, "fee_bps").map(|b| b / 100.0)).unwrap_or(0.5);
    let fee = total * fee_pct / 100.0;
    let net = total - fee;
    let item_name = s(&c, "item_name").or_else(|| s(&detail, "item_name")).map(str::to_string);
    let item_type = s(&c, "item_type").or_else(|| s(&detail, "item_type")).map(str::to_string);
    let deliverable = s(&c, "item_deliverable").or_else(|| s(&detail, "item_deliverable")).map(str::to_string);
    let allow_redact = c.get("allow_redact").or_else(|| detail.get("allow_redact")).and_then(Value::as_bool).unwrap_or(false);
    let creator = s(&c, "invoice_owner_identifier").or_else(|| s(&c, "creator_identificator")).unwrap_or("Merchant").to_string();
    let is_creator = ctx.mine || creator.trim_start_matches('@').eq_ignore_ascii_case(ctx.me);
    let tab = state.map_or(Tab::Details, |v| v.tab);
    let accent = if paid_flag { hex(0x34d399) } else { INDIGO_SOFT };

    // ── header ──
    let header = row![
        container(center(icon(icons::receipt(), 18, accent))).width(34).height(34).style(boxed(Color { a: 0.15, ..accent }, Color::TRANSPARENT, 9.0)),
        column![text("Invoice Details").size(16).font(BOLD).color(Color::WHITE), text("Accordo Invoice Inspector").size(11).color(TEXT_SECONDARY)].spacing(1),
        space().width(Fill),
        button(row![icon(icons::refresh_cw(), 11, TEXT_SECONDARY), text("Refresh").size(11).color(TEXT_SECONDARY)].spacing(4).align_y(Alignment::Center))
            .padding([4, 8])
            .style(ghost(6.0, rgba(0xffffff, 0.08)))
            .on_press_maybe((!state.is_some_and(|v| v.loading)).then(|| m(InvoiceMsg::Refresh(id.clone())))),
        status_badge::view(status_badge::invoice(&shown_status)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let payments_count = state.and_then(|v| v.payments.as_ref()).map_or(0, Vec::len);
    let tab_btn = |t: Tab, handle: iced::widget::svg::Handle, title: &'static str, badge: Option<usize>| -> El<'a> {
        let on = tab == t;
        let tint = if t == Tab::History { SKY } else { INDIGO };
        let mut r = row![icon(handle, 14, if on { tint } else { TEXT_SECONDARY }), text(title).size(12.5).font(if on { BOLD } else { MEDIUM }).color(if on { Color::WHITE } else { TEXT_SECONDARY })].spacing(6).align_y(Alignment::Center);
        if let Some(b) = badge.filter(|b| *b > 0) {
            r = r.push(container(text(b.to_string()).size(10).font(BOLD).color(if on { SKY } else { MUTED })).padding([0, 6]).style(boxed(if on { rgba(0x38bdf8, 0.2) } else { rgba(0xffffff, 0.08) }, Color::TRANSPARENT, 8.0)));
        }
        let underline = if on { tint } else { Color::TRANSPARENT };
        button(column![container(r).padding([8, 12]), container(space().width(Fill).height(2)).style(move |_| container::Style { background: Some(underline.into()), ..Default::default() })].width(iced::Shrink))
            .padding(0)
            .style(ghost(6.0, rgba(0xffffff, 0.04)))
            .on_press(m(InvoiceMsg::Tab(id.clone(), t)))
            .into()
    };
    let tabs = row![tab_btn(Tab::Details, icons::receipt(), "Details", None), tab_btn(Tab::History, icons::clock(), "Payment History", Some(payments_count))].spacing(4);

    let body: El = match tab {
        Tab::Details => {
            let mut hero_left = column![
                row![icon(icons::coins(), 14, SKY), text("Invoice Amount Due").size(12).color(TEXT_SECONDARY)].spacing(5).align_y(Alignment::Center),
                row![text(amount(total)).size(24).font(EXTRABOLD).color(if paid_flag { hex(0x10b981) } else { Color::WHITE }), text(token.clone()).size(16).font(BOLD).color(INDIGO)].spacing(6).align_y(Alignment::End),
                row![text("Network:").size(12).color(TEXT_SECONDARY), text(chain.clone()).size(12).font(BOLD).color(LABEL)].spacing(4),
            ]
            .spacing(4)
            .width(Fill);
            if allow_redact {
                hero_left = hero_left.push(container(text("✏️ Redactable").size(10).font(BOLD).color(AMBER_TEXT)).padding([1, 6]).style(boxed(rgba(0xf59e0b, 0.15), Color::TRANSPARENT, 4.0)));
            }
            let kind = item_type.clone().map(|t| t.to_uppercase()).unwrap_or_else(|| "STANDARD INVOICE".into());
            let hero = container(
                row![hero_left, column![text("Type").size(11).color(TEXT_SECONDARY), container(text(kind).size(10).font(BOLD).color(INDIGO_SOFT)).padding([2, 8]).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 6.0))].spacing(4).align_x(Alignment::End)]
                    .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([14, 16])
            .style(boxed(Color { a: 0.08, ..accent }, Color { a: 0.25, ..accent }, 12.0));

            let id_bar = panel(
                row![
                    column![text("Invoice ID (UUID / Hash)").size(11).color(TEXT_SECONDARY), text(id.clone()).size(12).font(Font::MONOSPACE).color(SKY).wrapping(Wrapping::None)].spacing(2).width(Fill),
                    copy_btn(id.clone(), "Invoice ID", true),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .clip(true);

            let mut col = column![hero, id_bar].spacing(14);

            if let Some(name) = &item_name {
                let mut item = column![row![
                    container(text(format!("{} {}", item_icon(item_type.as_deref()), item_type.clone().map(|t| t.to_uppercase()).unwrap_or_else(|| "PRODUCT".into()))).size(10).font(BOLD).color(INDIGO_SOFT)).padding([2, 8]).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 6.0)),
                    space().width(Fill),
                    text(name.clone()).size(13).font(BOLD).color(Color::WHITE),
                ]
                .align_y(Alignment::Center)]
                .spacing(6);
                if let Some(dl) = &deliverable {
                    item = item.push(container(space().width(Fill).height(1)).style(rule)).push(
                        row![icon(icons::inbox(), 13, INDIGO_ICON), text("Delivery on Settlement:").size(11).font(BOLD).color(LABEL), text(dl.clone()).size(11).color(LABEL)].spacing(5).align_y(Alignment::Center),
                    );
                }
                col = col.push(container(item).width(Fill).padding([10, 12]).style(boxed(rgba(0x6366f1, 0.06), rgba(0x6366f1, 0.25), 10.0)));
            }

            let econ = column![
                row![icon(icons::coins(), 14, AMBER_TEXT), text("Settlement & Fee Economics").size(12).font(BOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center),
                line("Gross Amount Due:".into(), format!("{} {token}", amount(total)), TEXT_SECONDARY, false),
                line(format!("Platform Fee ({fee_pct}%):"), format!("-{} {token}", amount(fee)), SKY, false),
                container(space().width(Fill).height(1)).style(rule),
                line("Net to Merchant:".into(), format!("{} {token}", amount(net)), GREEN, true),
                line("Settled:".into(), format!("{} / {} {token}", amount(paid), amount(total)), if paid_flag { hex(0x10b981) } else { AMBER_TEXT }, true),
            ]
            .spacing(5);
            col = col.push(container(econ).width(Fill).padding([12, 14]).style(boxed(rgba(0x38bdf8, 0.04), rgba(0x38bdf8, 0.2), 10.0)));

            let created = s(&c, "created_at").map(when).unwrap_or_else(|| "N/A".into());
            let due_text = if due == 0 { "No Due Date (Lifetime)".to_string() } else { Local.timestamp_opt(due, 0).single().map(|t| t.format("%b %-d, %H:%M").to_string()).unwrap_or_default() };
            let mut due_col = column![label(icons::clock(), "Payment Due", TEXT_SECONDARY), text(due_text).size(12).font(SEMIBOLD).color(if overdue { hex(0xef4444) } else { Color::WHITE })].spacing(3);
            if due > 0 && !paid_flag && !cancelled {
                due_col = due_col.push(text(if overdue { "Overdue".to_string() } else { left(due) }).size(11.5).color(TEXT_SECONDARY));
            }
            col = col.push(row![panel(column![label(icons::calendar_clock(), "Created At", TEXT_SECONDARY), text(created).size(12).font(SEMIBOLD).color(Color::WHITE)].spacing(3)), panel(due_col)].spacing(10));

            // ── who ──
            let person = |caption: &'a str, handle: iced::widget::svg::Handle, name: String, addr: Option<String>, none: &'a str, what: &'static str| -> El<'a> {
                let mut r = row![column![
                    text(name).size(12).font(SEMIBOLD).color(Color::WHITE),
                    text(addr.as_deref().map(short).unwrap_or_else(|| none.to_string())).size(11).font(Font::MONOSPACE).color(TEXT_SECONDARY),
                ]
                .spacing(1)
                .width(Fill)]
                .align_y(Alignment::Center);
                if let Some(a) = addr {
                    r = r.push(copy_btn(a, what, false));
                }
                column![label(handle, caption, TEXT_SECONDARY), container(r).padding([6, 10]).style(boxed(rgba(0xffffff, 0.03), Color::TRANSPARENT, 8.0))].spacing(4).into()
            };
            let merchant_addr = s(&c, "merchant_address").or_else(|| s(&detail, "merchant")).map(str::to_string);
            let payer_addr = s(&c, "payer_address").or_else(|| s(&detail, "payer_address")).filter(|a| !a.eq_ignore_ascii_case(ZERO_ADDRESS)).map(str::to_string);
            let payer = crate::components::finance::records::invoice_payer(&c);
            let mut people = column![
                person("Merchant (Issuer)", icons::user(), creator.clone(), merchant_addr, "On-Chain Vault", "Merchant Address"),
                person("Billed Client (Payer)", icons::wallet(), payer, payer_addr, "Anyone / Open Payer", "Client Address"),
            ]
            .spacing(12);
            let pin = s(&c, "secret_code").or_else(|| s(&detail, "secret_code")).map(str::to_string);
            if let Some(p) = pin.filter(|_| is_creator || paid_flag) {
                people = people.push(
                    container(row![icon(icons::key_round(), 12, AMBER_TEXT), text("Secret PIN Code:").size(11).font(SEMIBOLD).color(AMBER_TEXT), space().width(Fill), text(p.clone()).size(13).font(Font { weight: iced::font::Weight::Bold, ..Font::MONOSPACE }).color(AMBER_TEXT), copy_btn(p, "PIN", false)].spacing(5).align_y(Alignment::Center))
                        .padding([6, 10])
                        .style(boxed(rgba(0xfbbf24, 0.06), rgba(0xfbbf24, 0.2), 8.0)),
                );
            }
            if let Some(note) = s(&c, "note").or_else(|| s(&detail, "note")) {
                people = people.push(
                    column![
                        text("Note / Memo").size(11).color(TEXT_SECONDARY),
                        container(text(format!("\"{note}\"")).size(12).color(TEXT)).width(Fill).padding([8, 12]).style(|_| container::Style {
                            background: Some(rgba(0xffffff, 0.04).into()),
                            border: iced::Border { color: INDIGO, width: 0.0, radius: 8.0.into() },
                            ..Default::default()
                        }),
                    ]
                    .spacing(2),
                );
            }
            col = col.push(panel(people).padding(14));

            // ── what can be done ──
            if paid_flag {
                col = col.push(container(row![icon(icons::circle_check(), 15, hex(0x34d399)), text("Paid & settled").size(13).font(BOLD).color(hex(0x34d399))].spacing(8).align_y(Alignment::Center)).width(Fill).center_x(Fill).padding([10, 12]).style(boxed(rgba(0x10b981, 0.08), rgba(0x10b981, 0.25), 10.0)));
            } else if cancelled {
                col = col.push(container(text("This invoice was cancelled").size(12.5).color(MUTED)).width(Fill).center_x(Fill).padding([10, 12]).style(boxed(rgba(0xffffff, 0.03), BORDER, 10.0)));
            } else if !is_creator {
                let left_to_pay = (total - paid).max(0.0);
                col = col.push(
                    button(center(row![icon(icons::wallet(), 15, Color::WHITE), text(format!("Pay {} {token}", amount(left_to_pay))).size(14).font(BOLD)].spacing(8).align_y(Alignment::Center)).height(22))
                        .width(Fill)
                        .padding([10, 0])
                        .style(primary)
                        .on_press(Message::PayInvoice(Box::new(c.clone()))),
                );
            } else {
                col = col.push(container(text("Waiting for the client to pay").size(12.5).color(AMBER_TEXT)).width(Fill).center_x(Fill).padding([10, 12]).style(boxed(rgba(0xf59e0b, 0.06), rgba(0xf59e0b, 0.2), 10.0)));
            }
            col.into()
        }
        Tab::History => history(&id, state, ctx, total, &token, &chain),
    };

    container(
        column![
            container(header).padding(Padding { top: 4.0, right: 4.0, bottom: 10.0, left: 4.0 }),
            tabs,
            container(space().width(Fill).height(1)).style(rule),
            container(body).padding(Padding { top: 14.0, right: 4.0, bottom: 4.0, left: 4.0 }),
        ]
        .spacing(0),
    )
    .width(520)
    .padding(Padding { top: 12.0, right: 14.0, bottom: 14.0, left: 16.0 })
    .style(|_| container::Style {
        background: Some(rgba(0x000000, 0.3).into()),
        border: iced::Border { color: rgba(0xffffff, 0.06), width: 1.0, radius: 8.0.into() },
        ..Default::default()
    })
    .into()
}

fn history<'a>(id: &str, state: Option<&'a InvoiceView>, ctx: &CardCtx<'a>, total: f64, token: &str, chain: &str) -> El<'a> {
    let list = state.and_then(|v| v.payments.as_ref());
    let loading = state.is_some_and(|v| v.loading);
    let sum: f64 = list.map(|l| l.iter().filter_map(|p| num(p, "amount_paid")).sum()).unwrap_or(0.0);
    let n = list.map_or(0, Vec::len);
    let summary = panel(
        row![
            column![
                label(icons::clock(), "Payments Received", SKY),
                row![text(amount(sum)).size(22).font(EXTRABOLD).color(if sum >= total && total > 0.0 { hex(0x10b981) } else { Color::WHITE }), text(format!("/ {} {token}", amount(total))).size(14).color(TEXT_SECONDARY)].spacing(4).align_y(Alignment::End),
            ]
            .spacing(4)
            .width(Fill),
            column![text("Payments").size(11).color(TEXT_SECONDARY), container(text(format!("{n} {}", if n == 1 { "payment" } else { "payments" })).size(11).font(BOLD).color(SKY)).padding([2, 8]).style(boxed(rgba(0x38bdf8, 0.12), Color::TRANSPARENT, 8.0))].spacing(2).align_x(Alignment::End),
            button(icon(icons::refresh_cw(), 13, TEXT_SECONDARY)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press_maybe((!loading).then(|| m(InvoiceMsg::Refresh(id.to_string())))),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(14);
    let mut col: Column<'a, Message> = column![summary].spacing(10);
    match list {
        None if loading => col = col.push(center(text("Loading payment history...").size(13).color(TEXT_SECONDARY)).height(90)),
        Some(list) if !list.is_empty() => {
            for p in list {
                let amt = num(p, "amount_paid").map(amount).unwrap_or_else(|| "0".into());
                let sym = s(p, "token_symbol").unwrap_or(token).to_string();
                let net_name = s(p, "chain").unwrap_or(chain).to_string();
                let mut card = column![row![
                    container(center(text("✓").size(13).font(BOLD).color(GREEN))).width(26).height(26).style(boxed(Color { a: 0.15, ..GREEN }, Color::TRANSPARENT, 13.0)),
                    column![text(format!("+{amt} {sym}")).size(14).font(BOLD).color(GREEN), text(s(p, "created_at").map(when).unwrap_or_else(|| "Recent transaction".into())).size(10).color(TEXT_SECONDARY)].spacing(1).width(Fill),
                    container(text(net_name.clone()).size(9.5).font(BOLD).color(MUTED)).padding([2, 6]).style(boxed(rgba(0xffffff, 0.06), Color::TRANSPARENT, 4.0)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)]
                .spacing(8);
                let mut who = Column::new().spacing(4);
                if let Some(ident) = s(p, "payer_identifier") {
                    who = who.push(row![text("Payer:").size(11.5).color(TEXT_SECONDARY).width(Fill), text(if ident.starts_with('@') { ident.to_string() } else { format!("@{ident}") }).size(11.5).font(SEMIBOLD).color(Color::WHITE)]);
                }
                if let Some(a) = s(p, "payer_address").map(|a| a.trim_start_matches("external_").to_string()).filter(|a| a.len() > 10) {
                    who = who.push(row![text("Address:").size(11.5).color(TEXT_SECONDARY).width(Fill), text(crate::wallets::short_address(&a)).size(11).font(Font::MONOSPACE).color(MUTED), copy_btn(a.clone(), "Payer Address", false)].spacing(5).align_y(Alignment::Center));
                }
                card = card.push(container(who).width(Fill).padding([6, 8]).style(boxed(rgba(0xffffff, 0.02), Color::TRANSPARENT, 6.0)));
                if let Some(tx) = s(p, "tx_hash") {
                    let tx = if tx.starts_with("0x") { tx.to_string() } else { format!("0x{tx}") };
                    let mut r = row![text("Tx:").size(11).color(TEXT_SECONDARY), text(crate::wallets::short_address(&tx)).size(11).font(Font::MONOSPACE).color(SKY), copy_btn(tx.clone(), "Tx Hash", false), space().width(Fill)].spacing(5).align_y(Alignment::Center);
                    if let Some(explorer) = ctx.explorers.get(&net_name.to_lowercase()) {
                        r = r.push(
                            button(row![text("Explorer").size(11).color(INDIGO_SOFT), icon(icons::arrow_up_right(), 11, INDIGO_SOFT)].spacing(3).align_y(Alignment::Center))
                                .padding([2, 6])
                                .style(ghost(6.0, rgba(0x6366f1, 0.12)))
                                .on_press(Message::Chat(ChatMsg::Cheque(ChequeMsg::OpenTx(format!("{explorer}/tx/{tx}"))))),
                        );
                    }
                    card = card.push(r);
                }
                col = col.push(panel(card).padding(12));
            }
        }
        _ => {
            col = col.push(
                container(
                    column![icon(icons::clock(), 20, TEXT_SECONDARY), text("No payments yet").size(14).font(SEMIBOLD).color(Color::WHITE), text("Payments to this invoice show up here with their transactions.").size(12).color(TEXT_SECONDARY)]
                        .spacing(8)
                        .align_x(Alignment::Center),
                )
                .width(Fill)
                .padding([26, 14])
                .align_x(iced::alignment::Horizontal::Center)
                .style(boxed(rgba(0xffffff, 0.02), BORDER, 12.0)),
            )
        }
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_and_due_dates() {
        let v = json!({"detail": {"invoiceId": "0xab", "due_at_timestamp": 1_900_000_000}, "status": "UNPAID"});
        assert_eq!(invoice_id(&v).as_deref(), Some("0xab"));
        assert!(is_live(&v));
        assert!(!is_live(&json!({"status": "PAID", "due_at_timestamp": 5})));
        assert!(!is_live(&json!({"status": "UNPAID"})));
    }
}
