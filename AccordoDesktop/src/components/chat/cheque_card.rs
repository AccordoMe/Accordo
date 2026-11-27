// SPDX-License-Identifier: GPL-3.0-or-later
//! A cheque in the chat (ChequeDetailsCard.tsx, chat mode): the "Accordo Cheque Inspector" —
//! total, id, how the payout and fee split, dates with a live countdown, sender and recipients,
//! and the claim history. What the message carries is shown at once; the server's copy
//! (/api/cheques/{id}, its claims) is loaded next to it and wins where it says more.
//!
//! Claim opens the review window (flows/claim.rs): AccordoCheques signs with your wallet, AccordoTask is gasless.

use std::collections::HashMap;

use chrono::{Local, TimeZone};
use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, space, text, Column};
use iced::{Alignment, Color, Fill, Font, Padding};
use serde_json::Value;

use crate::app::Message;
use crate::components::finance::status_badge;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Details,
    History,
}

#[derive(Debug, Clone)]
pub enum ChequeMsg {
    Tab(String, Tab),
    Loaded(String, Result<Value, String>),
    Claims(String, Result<Vec<Value>, String>),
    RefreshClaims(String),
    Copy(String, &'static str),
    OpenTx(String),
    /// The cheque (as shown) and the clicked slot's recipient.
    Claim(Box<Value>, String),
}

/// What a card keeps between redraws.
#[derive(Default)]
pub struct ChequeView {
    pub tab: Tab,
    /// The server's copy.
    pub live: Option<Value>,
    pub claims: Option<Vec<Value>>,
    pub loading_claims: bool,
}

/// What the cards need from around them.
pub struct CardCtx<'a> {
    pub cheques: &'a HashMap<String, ChequeView>,
    /// Network name (lowercase) → explorer base.
    pub explorers: &'a HashMap<String, String>,
    /// The viewer's wallets: (address lowercase, name).
    pub wallets: &'a [(String, String)],
}

fn m(msg: ChequeMsg) -> Message {
    Message::Chat(ChatMsg::Cheque(msg))
}

pub fn cheque_id(data: &Value) -> Option<String> {
    data.get("cheque_id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// The message's cheque with the server's copy laid over it (claims merged, like the web).
pub fn merged(data: &Value, live: Option<&Value>) -> Value {
    let Some(live) = live.filter(|l| l.is_object()) else { return data.clone() };
    let mut out = data.clone();
    if let (Some(o), Some(l)) = (out.as_object_mut(), live.as_object()) {
        for (k, v) in l {
            if k != "detail" {
                o.insert(k.clone(), v.clone());
            }
        }
        let mut detail = data.get("detail").cloned().unwrap_or_else(|| Value::Object(Default::default()));
        if let (Some(d), Some(ld)) = (detail.as_object_mut(), live.get("detail").and_then(Value::as_object)) {
            for (k, v) in ld {
                if k == "claims" {
                    let mut claims = d.get("claims").cloned().unwrap_or_else(|| Value::Object(Default::default()));
                    if let (Some(c), Some(lc)) = (claims.as_object_mut(), v.as_object()) {
                        for (ck, cv) in lc {
                            c.insert(ck.clone(), cv.clone());
                        }
                    }
                    d.insert("claims".into(), claims);
                } else {
                    d.insert(k.clone(), v.clone());
                }
            }
        }
        o.insert("detail".into(), detail);
    }
    out
}

pub(crate) fn s<'v>(v: &'v Value, k: &str) -> Option<&'v str> {
    v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty())
}

pub(crate) fn num(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(t) => t.trim().parse().ok(),
        _ => None,
    }
}

/// toFixed(6) without trailing zeros, like the web's formatAmount.
pub fn amount(x: f64) -> String {
    if !x.is_finite() {
        return "0.00".into();
    }
    let t = format!("{x:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" { "0".into() } else { t.to_string() }
}

pub(crate) fn short(addr: &str) -> String {
    if addr.len() > 20 { format!("{}...{}", &addr[..8], &addr[addr.len() - 6..]) } else { addr.to_string() }
}

pub(crate) fn when(iso: &str) -> String {
    super::display::local(iso).map(|t| t.format("%b %-d, %H:%M").to_string()).unwrap_or_else(|| "N/A".into())
}

pub(crate) fn left(until_unix: i64) -> String {
    let secs = until_unix - Local::now().timestamp();
    if secs <= 0 {
        return "expired".into();
    }
    let d = secs / 86400;
    let hms = format!("{:02}:{:02}:{:02}", (secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
    if d > 0 { format!("{d}d {hms} left") } else { format!("{hms} left") }
}

/// Does showing this card need a clock (a countdown to expiry)?
pub fn is_live(data: &Value) -> bool {
    let status = s(data, "status").unwrap_or("").to_lowercase();
    data.get("detail").and_then(|d| num(d, "expires_at")).is_some_and(|e| e > 0.0) && !matches!(status.as_str(), "claimed" | "refunded")
}

pub(crate) fn panel<'a>(content: impl Into<El<'a>>) -> container::Container<'a, Message> {
    container(content).width(Fill).padding([10, 12]).style(boxed(rgba(0xffffff, 0.02), BORDER, 10.0))
}

pub(crate) fn label<'a>(handle: iced::widget::svg::Handle, t: &'a str, tint: Color) -> El<'a> {
    row![icon(handle, 12, tint), text(t).size(11).color(TEXT_SECONDARY)].spacing(5).align_y(Alignment::Center).into()
}

pub(crate) fn copy_btn<'a>(value: String, what: &'static str, with_label: bool) -> El<'a> {
    let content: El = if with_label {
        row![icon(icons::copy(), 12, TEXT_SECONDARY), text("Copy").size(11).color(TEXT_SECONDARY)].spacing(4).align_y(Alignment::Center).into()
    } else {
        icon(icons::copy(), 12, TEXT_SECONDARY).into()
    };
    button(content).padding(if with_label { [4, 8] } else { [3, 3] }).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press(m(ChequeMsg::Copy(value, what))).into()
}

pub(crate) fn line<'a>(left_text: String, right: String, color: Color, bold: bool) -> El<'a> {
    row![text(left_text).size(12).color(color).width(Fill), text(right).size(12).font(if bold { Font { weight: iced::font::Weight::Bold, ..Font::MONOSPACE } } else { Font::MONOSPACE }).color(color)].align_y(Alignment::Center).into()
}

pub fn view<'a>(data: &'a Value, ctx: &CardCtx<'a>) -> El<'a> {
    let id = cheque_id(data).unwrap_or_default();
    let state = ctx.cheques.get(&id);
    let c = merged(data, state.and_then(|v| v.live.as_ref()));
    let empty = Value::Object(Default::default());
    let detail = c.get("detail").unwrap_or(&empty).clone();
    let status = s(&c, "status").unwrap_or("pending").to_string();
    let claimed_all = status.eq_ignore_ascii_case("claimed");
    let refunded = status.eq_ignore_ascii_case("refunded");
    let exp = num(&detail, "expires_at").unwrap_or(0.0) as i64;
    let expired = exp > 0 && Local::now().timestamp() > exp;
    let token = s(&detail, "token_symbol").or_else(|| s(&detail, "token")).unwrap_or("TOKEN").to_string();
    let chain = s(&detail, "chain").or_else(|| s(&c, "chain")).unwrap_or("EVM").to_string();

    let recipients: Vec<String> = match detail.get("to") {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(Value::String(t)) if !t.trim().is_empty() => vec![t.clone()],
        _ => s(&c, "recipient_identificator").map(|r| vec![r.to_string()]).unwrap_or_default(),
    };
    let n = recipients.len().max(1) as f64;
    let total = num(&detail, "original_amount").or_else(|| num(&detail, "amount")).or_else(|| num(&detail, "amount_per_recipient")).unwrap_or(0.0);
    let fee_bps = num(&detail, "feeBps").or_else(|| num(&detail, "fee_bps")).unwrap_or(50.0);
    let fee_pct = fee_bps / 100.0;
    // A swap: the taker gets the locked amount whole and pays `pay` of another token; the fee comes
    // out of that payment, on its way to the sender.
    let is_swap = detail.get("is_swap").and_then(Value::as_bool) == Some(true);
    let pay = num(&detail, "pay_amount").unwrap_or(0.0);
    let pay_token = s(&detail, "pay_token").unwrap_or("TOKEN").to_string();
    let pay_fee = pay * fee_pct / 100.0;
    let sender_gets = num(&detail, "creator_gets").unwrap_or(pay - pay_fee);
    let gross = total / n;
    let fee_each = if is_swap { 0.0 } else { gross * fee_pct / 100.0 };
    let net = gross - fee_each;

    let tab = state.map_or(Tab::Details, |v| v.tab);
    let shown_status = if expired && !claimed_all && !refunded { "expired".to_string() } else { status.clone() };

    // ── header ──
    let header = row![
        container(center(icon(icons::file_text(), 18, INDIGO_SOFT))).width(34).height(34).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 9.0)),
        column![
            text(if is_swap { "Swap Details" } else { "Cheque Details" }).size(16).font(BOLD).color(Color::WHITE),
            text(if is_swap { "Accordo P2P swap: both sides move in one transaction" } else { "Accordo Cheque Inspector" }).size(11).color(TEXT_SECONDARY),
        ]
        .spacing(1),
        space().width(Fill),
        status_badge::view(status_badge::cheque(&shown_status)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let claims_count = state.and_then(|v| v.claims.as_ref()).map_or(0, Vec::len);
    let tab_btn = |t: Tab, handle: iced::widget::svg::Handle, title: &'static str, badge: Option<usize>| -> El<'a> {
        let on = tab == t;
        let mut r = row![icon(handle, 14, if on { if t == Tab::History { SKY } else { INDIGO } } else { TEXT_SECONDARY }), text(title).size(12.5).font(if on { BOLD } else { MEDIUM }).color(if on { Color::WHITE } else { TEXT_SECONDARY })].spacing(6).align_y(Alignment::Center);
        if let Some(b) = badge.filter(|b| *b > 0) {
            r = r.push(container(text(b.to_string()).size(10).font(BOLD).color(if on { SKY } else { MUTED })).padding([0, 6]).style(boxed(if on { rgba(0x38bdf8, 0.2) } else { rgba(0xffffff, 0.08) }, Color::TRANSPARENT, 8.0)));
        }
        let underline = if on { if t == Tab::History { SKY } else { INDIGO } } else { Color::TRANSPARENT };
        button(column![container(r).padding([8, 12]), container(space().width(Fill).height(2)).style(move |_| container::Style { background: Some(underline.into()), ..Default::default() })].width(iced::Shrink))
            .padding(0)
            .style(ghost(6.0, rgba(0xffffff, 0.04)))
            .on_press(m(ChequeMsg::Tab(id.clone(), t)))
            .into()
    };
    let tabs = row![tab_btn(Tab::Details, icons::file_text(), "Details", None), tab_btn(Tab::History, icons::clock(), "Claim History", Some(claims_count))].spacing(4);

    let body: El = match tab {
        Tab::Details => {
            let hero = container(
                column![
                    label(icons::link_2(), if is_swap { "Locked for the swap" } else { "Cheque Total Deposit" }, SKY),
                    row![text(amount(total)).size(24).font(EXTRABOLD).color(Color::WHITE), text(token.clone()).size(16).font(BOLD).color(INDIGO)]
                        .push(is_swap.then(|| text(format!("for {} {pay_token}", amount(pay))).size(14).font(BOLD).color(LABEL)))
                        .spacing(6)
                        .align_y(Alignment::End),
                    row![text("Network:").size(12).color(TEXT_SECONDARY), text(chain.clone()).size(12).font(BOLD).color(LABEL)].spacing(4),
                ]
                .spacing(4),
            )
            .width(Fill)
            .padding([14, 16])
            .style(boxed(rgba(0x6366f1, 0.08), rgba(0x6366f1, 0.25), 12.0));

            let id_bar = panel(
                row![
                    column![text("Cheque ID (UUID / Hash)").size(11).color(TEXT_SECONDARY), text(id.clone()).size(12).font(Font::MONOSPACE).color(SKY).wrapping(Wrapping::None)].spacing(2).width(Fill),
                    copy_btn(id.clone(), "Cheque ID", true),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .clip(true);

            let plural = if n > 1.0 { "s" } else { "" };
            let mut econ = if is_swap {
                column![
                    row![icon(icons::link_2(), 14, SKY), text("Swap & Fee Economics").size(12).font(BOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center),
                    line("Locked by the sender:".into(), format!("{} {token}", amount(total)), TEXT_SECONDARY, false),
                    line("The taker pays:".into(), format!("{} {pay_token}", amount(pay)), TEXT_SECONDARY, false),
                    line(format!("Platform Treasury Fee, from the payment ({fee_pct}%):"), format!("-{} {pay_token}", amount(pay_fee)), SKY, false),
                    container(space().width(Fill).height(1)).style(rule),
                    line("The sender receives:".into(), format!("{} {pay_token}", amount(sender_gets)), GREEN, true),
                    line("The taker receives:".into(), format!("{} {token}", amount(total)), GREEN, true),
                ]
                .spacing(5)
            } else {
                column![
                row![icon(icons::link_2(), 14, SKY), text(format!("Payout & Fee Economics ({} recipient{plural})", n as usize)).size(12).font(BOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center),
                line("Total Locked Deposit:".into(), format!("{} {token}", amount(total)), TEXT_SECONDARY, false),
                line("Each Recipient Share (Gross):".into(), format!("{} {token}{}", amount(gross), if n > 1.0 { format!(" (1/{})", n as usize) } else { String::new() }), TEXT_SECONDARY, false),
                line(format!("Platform Treasury Fee per Claim ({fee_pct}%):"), format!("-{} {token}", amount(fee_each)), SKY, false),
                container(space().width(Fill).height(1)).style(rule),
                line("Each Recipient Gets on Claim:".into(), format!("{} {token}", amount(net)), GREEN, true),
                ]
                .spacing(5)
            };
            if n > 1.0 && !is_swap {
                econ = econ
                    .push(line(format!("Total Treasury Fee (all {} claims):", n as usize), format!("{} {token}", amount(fee_each * n)), SKY, false))
                    .push(line("Total Net Payout to all recipients:".into(), format!("{} {token}", amount(net * n)), TEXT_SECONDARY, false));
            }

            let created = s(&c, "created_at").map(when).unwrap_or_else(|| "N/A".into());
            let exp_text = if exp == 0 { "No Expiration (Lifetime)".to_string() } else { Local.timestamp_opt(exp, 0).single().map(|t| t.format("%b %-d, %H:%M").to_string()).unwrap_or_default() };
            let mut exp_col = column![label(icons::clock(), "Expires", TEXT_SECONDARY), text(exp_text).size(12).font(SEMIBOLD).color(if expired { hex(0xef4444) } else { Color::WHITE })].spacing(3);
            if exp > 0 && !claimed_all && !refunded {
                exp_col = exp_col.push(text(left(exp)).size(11.5).color(TEXT_SECONDARY));
            }
            let dates = row![
                panel(column![label(icons::calendar_clock(), "Created Date", TEXT_SECONDARY), text(created).size(12).font(SEMIBOLD).color(Color::WHITE)].spacing(3)),
                panel(exp_col),
            ]
            .spacing(10);

            let from = s(&detail, "from").map(str::to_string);
            let sender = column![
                label(icons::user(), "Sender (Creator)", TEXT_SECONDARY),
                container(
                    row![
                        column![
                            text(crate::components::finance::records::cheque_sender(&c, "Unknown")).size(12).font(SEMIBOLD).color(Color::WHITE),
                            text(from.as_deref().map(short).unwrap_or_else(|| "N/A".into())).size(11).font(Font::MONOSPACE).color(TEXT_SECONDARY),
                        ]
                        .spacing(1)
                        .width(Fill),
                        from.clone().map_or_else(|| space().width(0).into(), |f| copy_btn(f, "Sender Address", false)),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([6, 10])
                .style(boxed(rgba(0xffffff, 0.03), Color::TRANSPARENT, 8.0)),
            ]
            .spacing(4);

            let claims_obj: Vec<(String, Value)> = detail.get("claims").and_then(Value::as_object).map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
            let mut rows = Column::new().spacing(8);
            for (i, r) in recipients.iter().enumerate() {
                let slot = claims_obj.get(i).map(|(_, v)| v.clone()).or_else(|| detail.get("claims").and_then(|o| o.get(r).or_else(|| o.get(r.to_lowercase()))).cloned());
                let slot_key = claims_obj.get(i).map(|(k, _)| k.clone()).unwrap_or_else(|| format!("slot{}", i + 1));
                let bearer = r.eq_ignore_ascii_case(ZERO_ADDRESS);
                let this_claimed = slot.as_ref().and_then(|v| v.get("claimed")).and_then(Value::as_bool).unwrap_or(false) || claimed_all;
                let pending = !this_claimed && !expired && !refunded;
                let wallet = ctx.wallets.iter().find(|(a, _)| a == &r.to_lowercase()).map(|(_, n)| n.clone());
                let can_claim = pending && (wallet.is_some() || bearer);

                let mut addr = row![text(if bearer { format!("{slot_key} (Anyone)") } else { short(r) }).size(12).font(Font::MONOSPACE).color(LABEL).wrapping(Wrapping::None)].spacing(6).align_y(Alignment::Center);
                if bearer && !this_claimed {
                    addr = addr.push(container(text("Bearer").size(9.5).font(BOLD).color(AMBER_TEXT)).padding([1, 6]).style(boxed(rgba(0xf59e0b, 0.15), Color::TRANSPARENT, 4.0)));
                }
                if let Some(name) = wallet {
                    addr = addr.push(container(text(name).size(9.5).font(BOLD).color(INDIGO_SOFT)).padding([1, 6]).style(boxed(rgba(0x6366f1, 0.2), Color::TRANSPARENT, 4.0)));
                }
                addr = addr.push(copy_btn(r.clone(), "Recipient Address", false));
                let right: El = if can_claim {
                    button(text(if is_swap { "Swap" } else { "Claim" }).size(12).font(BOLD)).padding([5, 14]).style(primary).on_press(m(ChequeMsg::Claim(Box::new(c.clone()), r.clone()))).into()
                } else {
                    let (t, col) = if this_claimed { ("CLAIMED", GREEN) } else if expired { ("EXPIRED", hex(0xef4444)) } else if refunded { ("REFUNDED", MUTED) } else { ("PENDING", AMBER_TEXT) };
                    container(text(t).size(10).font(BOLD).color(col)).padding([3, 8]).style(boxed(Color { a: 0.12, ..col }, Color::TRANSPARENT, 6.0)).into()
                };
                rows = rows.push(
                    container(
                        row![
                            column![
                                addr,
                                if is_swap {
                                    row![text(format!("Pays {} {pay_token} → gets {} {token}", amount(pay), amount(total))).size(11).font(Font { weight: iced::font::Weight::Bold, ..Font::MONOSPACE }).color(GREEN)]
                                } else {
                                    row![
                                        text(format!("Claim: {} {token}", amount(net))).size(11).font(Font { weight: iced::font::Weight::Bold, ..Font::MONOSPACE }).color(GREEN),
                                        text("|").size(11).color(rgba(0xffffff, 0.3)),
                                        text(format!("Fee: {} ({fee_pct}%)", amount(fee_each))).size(11).font(Font::MONOSPACE).color(SKY),
                                    ]
                                }
                                .spacing(6),
                            ]
                            .spacing(3)
                            .width(Fill),
                            right,
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 12])
                    .style(boxed(if this_claimed { rgba(0x10b981, 0.06) } else { rgba(0x6366f1, 0.06) }, if this_claimed { rgba(0x10b981, 0.25) } else { rgba(0x6366f1, 0.35) }, 8.0)),
                );
            }
            let people = panel(
                column![
                    sender,
                    column![
                        row![label(icons::wallet(), "", TEXT_SECONDARY), text(if is_swap { "Who can take it".to_string() } else { format!("Recipients ({})", recipients.len().max(1)) }).size(11).color(TEXT_SECONDARY), space().width(Fill), text(if is_swap { format!("Pays {} {pay_token}", amount(pay)) } else { format!("Each: {} {token}", amount(net)) }).size(11).font(Font::MONOSPACE).color(GREEN)].spacing(4).align_y(Alignment::Center),
                        rows,
                    ]
                    .spacing(6),
                ]
                .spacing(12),
            )
            .padding(14);

            column![hero, id_bar, container(econ).width(Fill).padding([12, 14]).style(boxed(rgba(0x38bdf8, 0.04), rgba(0x38bdf8, 0.2), 10.0)), dates, people].spacing(14).into()
        }
        Tab::History => history(&id, &c, state, ctx, total, net, &token, &chain),
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

#[allow(clippy::too_many_arguments)]
fn history<'a>(id: &str, c: &Value, state: Option<&'a ChequeView>, ctx: &CardCtx<'a>, total: f64, net: f64, token: &str, chain: &str) -> El<'a> {
    let status = s(c, "status").unwrap_or("").to_lowercase();
    let list = state.and_then(|v| v.claims.as_ref());
    let loading = state.is_some_and(|v| v.loading_claims);
    let claimed_sum: f64 = list.map(|l| l.iter().filter(|x| s(x, "action_type").unwrap_or("CLAIM").eq_ignore_ascii_case("claim")).filter_map(|x| num(x, "amount")).sum()).unwrap_or(0.0);
    let events = list.map_or(0, Vec::len);
    let summary = panel(
        row![
            column![
                label(icons::clock(), "Cheque Claims & Refunds", SKY),
                row![
                    text(amount(claimed_sum)).size(22).font(EXTRABOLD).color(if status == "claimed" { hex(0x10b981) } else if status == "refunded" { DANGER_TEXT } else { Color::WHITE }),
                    text(format!("/ {} {token}", amount(total))).size(14).color(TEXT_SECONDARY),
                ]
                .spacing(4)
                .align_y(Alignment::End),
            ]
            .spacing(4)
            .width(Fill),
            column![text("Events").size(11).color(TEXT_SECONDARY), container(text(format!("{events} {}", if events == 1 { "event" } else { "events" })).size(11).font(BOLD).color(SKY)).padding([2, 8]).style(boxed(rgba(0x38bdf8, 0.12), Color::TRANSPARENT, 8.0))].spacing(2).align_x(Alignment::End),
            button(icon(icons::refresh_cw(), 13, TEXT_SECONDARY)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press_maybe((!loading).then(|| m(ChequeMsg::RefreshClaims(id.to_string())))),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(14);
    let mut col = column![summary].spacing(10);
    match list {
        _ if loading && list.is_none() => col = col.push(center(text("Loading claims history...").size(13).color(TEXT_SECONDARY)).height(90)),
        None | Some(_) if events == 0 => {
            col = col.push(
                container(
                    column![
                        icon(icons::clock(), 20, TEXT_SECONDARY),
                        text("No claims or refunds yet").size(14).font(SEMIBOLD).color(Color::WHITE),
                        text("When a recipient claims their funds or the creator reclaims them, the audit trail will be shown here.").size(12).color(TEXT_SECONDARY).align_x(iced::alignment::Horizontal::Center).width(300),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .width(Fill)
                .padding([26, 14])
                .align_x(iced::alignment::Horizontal::Center)
                .style(boxed(rgba(0xffffff, 0.02), BORDER, 12.0)),
            )
        }
        Some(list) => {
            for x in list {
                let refund = s(x, "action_type").unwrap_or("CLAIM").eq_ignore_ascii_case("refund");
                let amt = num(x, "amount").map(amount).unwrap_or_else(|| amount(net));
                let sym = s(x, "token_symbol").unwrap_or(token).to_string();
                let net_name = s(x, "chain").unwrap_or(chain).to_string();
                let tint = if refund { AMBER_TEXT } else { GREEN };
                let mut card = column![row![
                    container(center(text(if refund { "↩" } else { "✓" }).size(13).font(BOLD).color(tint))).width(26).height(26).style(boxed(Color { a: 0.15, ..tint }, Color::TRANSPARENT, 13.0)),
                    column![
                        text(if refund { format!("Refunded {amt} {sym}") } else { format!("+{amt} {sym}") }).size(14).font(BOLD).color(tint),
                        text(s(x, "created_at").map(when).unwrap_or_else(|| "Recent activity".into())).size(10).color(TEXT_SECONDARY),
                    ]
                    .spacing(1)
                    .width(Fill),
                    container(text(s(x, "action_type").unwrap_or("CLAIM").to_uppercase()).size(9.5).font(BOLD).color(tint)).padding([2, 6]).style(boxed(Color { a: 0.12, ..tint }, Color::TRANSPARENT, 4.0)),
                    container(text(net_name.clone()).size(9.5).font(BOLD).color(MUTED)).padding([2, 6]).style(boxed(rgba(0xffffff, 0.06), Color::TRANSPARENT, 4.0)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)]
                .spacing(8);
                let mut who = Column::new().spacing(4);
                if let Some(ident) = s(x, "claimant_identifier") {
                    let name = if ident.starts_with('@') || ident.starts_with("external_") { ident.to_string() } else { format!("@{ident}") };
                    who = who.push(row![text(if refund { "Owner:" } else { "Claimant:" }).size(11.5).color(TEXT_SECONDARY).width(Fill), text(name).size(11.5).font(SEMIBOLD).color(Color::WHITE)]);
                }
                let addr = s(x, "claimant_address").map(|a| a.trim_start_matches("external_").to_string());
                if let Some(a) = addr.filter(|a| a.len() > 10) {
                    who = who.push(row![text("Address:").size(11.5).color(TEXT_SECONDARY).width(Fill), text(format!("{}...{}", &a[..6], &a[a.len() - 4..])).size(11).font(Font::MONOSPACE).color(MUTED), copy_btn(a.clone(), "Claimant Address", false)].spacing(5).align_y(Alignment::Center));
                }
                card = card.push(container(who).width(Fill).padding([6, 8]).style(boxed(rgba(0xffffff, 0.02), Color::TRANSPARENT, 6.0)));
                match s(x, "tx_hash") {
                    Some(tx) => {
                        let tx = if tx.starts_with("0x") { tx.to_string() } else { format!("0x{tx}") };
                        let explorer = ctx.explorers.get(&net_name.to_lowercase()).cloned().unwrap_or_else(|| "https://etherscan.io".into());
                        let url = format!("{explorer}/tx/{tx}");
                        card = card.push(
                            row![
                                text("Tx:").size(11).color(TEXT_SECONDARY),
                                text(format!("{}...{}", &tx[..tx.len().min(8)], &tx[tx.len().saturating_sub(6)..])).size(11).font(Font::MONOSPACE).color(SKY),
                                copy_btn(tx.clone(), "Tx Hash", false),
                                space().width(Fill),
                                button(row![text("Explorer").size(11).color(INDIGO_SOFT), icon(icons::arrow_up_right(), 11, INDIGO_SOFT)].spacing(3).align_y(Alignment::Center)).padding([2, 6]).style(ghost(6.0, rgba(0x6366f1, 0.12))).on_press(m(ChequeMsg::OpenTx(url))),
                            ]
                            .spacing(5)
                            .align_y(Alignment::Center),
                        );
                    }
                    None => card = card.push(text("Internal settlement").size(10).color(TEXT_SECONDARY)),
                }
                col = col.push(panel(card).padding(12));
            }
        }
        _ => {}
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn amounts_like_the_web() {
        assert_eq!(amount(9.5), "9.5");
        assert_eq!(amount(10.0), "10");
        assert_eq!(amount(0.0000001), "0");
        assert_eq!(amount(1.23456789), "1.234568");
    }

    #[test]
    fn server_copy_wins_and_claims_merge() {
        let msg = json!({"cheque_id": "0xab", "status": "pending", "detail": {"amount": "10", "claims": {"slot1": {"claimed": false}, "slot2": {"claimed": false}}}});
        let live = json!({"status": "partially_claimed", "detail": {"claims": {"slot1": {"claimed": true}}}});
        let c = merged(&msg, Some(&live));
        assert_eq!(c["status"], "partially_claimed");
        assert_eq!(c["detail"]["amount"], "10");
        assert_eq!(c["detail"]["claims"]["slot1"]["claimed"], true);
        assert_eq!(c["detail"]["claims"]["slot2"]["claimed"], false);
    }
}
