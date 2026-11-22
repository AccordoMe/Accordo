// SPDX-License-Identifier: GPL-3.0-or-later
//! A token drop in the chat (TokenDropChatEmbed.tsx with the claim card of DropClaimPage.tsx): what
//! the drop is (pool, claim deadline, who made it) and the reader's own shares. A share that vests
//! shows its parts (claimed / ready / locked), the plan in words and when the next part unlocks;
//! Claim takes what's ready.
//!
//! Where it comes from: Accordo gives what only it has — the title, the note, and the reader's
//! shares with their Merkle proofs; the contract gives the rest (creator, token, pool, deadline,
//! fee, closed or not, and what each share was paid so far), and a share is the reader's only if its
//! proof leads to the contract's root (`apply`). Until the contract answered — or if it can't be
//! reached — the card shows Accordo's copy and says so.
//!
//! What's unlocked is worked out here from the share's schedule (as AccordoDrop.unlockedAt does),
//! not frozen at the moment the card loaded: a part that unlocks while the chat is open shows up
//! without a reload, and Claim hands the review the amount of right now.

use alloy_primitives::{Address, U256};
use iced::gradient::Linear;
use iced::widget::{button, column, container, row, space, text, text_input, Column};
use iced::{Alignment, Background, Color, Fill, Font, Gradient, Length, Radians};
use serde_json::Value;

use super::fund_cards::{drop_id, is_address, note_line, Cards, CheckOther, DropCard, FundMsg, OnChain};
use crate::api::drops::{DropInfo, DropSlot, SlotSchedule};
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::evm::client::format_units;
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

const CLAIMED: Color = GREEN;
const READY: Color = INDIGO_ICON;
const LOCKED: Color = rgba(0xffffff, 0.08);
const VIOLET: Color = hex(0xa78bfa);
/// More parts than this: one smooth bar instead of a segment for each.
const MAX_SEGMENTS: usize = 48;
const WIDTH: f32 = 420.0;

fn fm(m: FundMsg) -> Message {
    Message::Chat(ChatMsg::Fund(m))
}

fn now() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

fn day(unix: u64) -> String {
    chrono::DateTime::from_timestamp(unix as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default()
}

/// "in 3d 4h" / "in 2h 5m" / "in 1m".
fn left(until: u64, now: u64) -> String {
    let s = until.saturating_sub(now);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3_600, s % 3_600 / 60);
    if d > 0 {
        format!("in {d}d {h}h")
    } else if h > 0 {
        format!("in {h}h {m}m")
    } else {
        format!("in {}m", m.max(1))
    }
}

/// Basis points as a percentage: 2500 → "25", 1250 → "12.5".
fn pct(bps: u16) -> String {
    let s = format!("{:.2}", f64::from(bps) / 100.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn wei(s: &str) -> U256 {
    U256::from_str_radix(s.trim(), 10).unwrap_or_default()
}

/// An amount for reading: at most 6 decimals, thousands grouped ("12,500.123456"); a dust amount
/// that would read as 0 says so.
pub fn pretty(s: &str) -> String {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let cut = frac.get(..6).unwrap_or(frac).trim_end_matches('0');
    let digits: Vec<char> = whole.chars().collect();
    let mut grouped = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*c);
    }
    if cut.is_empty() && whole.trim_start_matches('0').is_empty() && !frac.trim_end_matches('0').is_empty() {
        return "<0.000001".into();
    }
    if cut.is_empty() { grouped } else { format!("{grouped}.{cut}") }
}

fn units(v: U256, decimals: u8) -> String {
    pretty(&format_units(v, decimals))
}

/// What the wallet gets for `gross`: the contract keeps the fee from each claim.
fn after_fee(gross: U256, fee_bps: u16) -> U256 {
    gross - gross * U256::from(fee_bps) / U256::from(10_000u32)
}

// ── the schedule (AccordoDrop / app/payment_contracts.py) ──

/// AccordoDrop.unlockedAt: the upfront share at start, the rest in equal parts at
/// start + cliff + k × interval.
pub fn unlocked_at(amount: U256, sc: &SlotSchedule, t: u64) -> U256 {
    if t < sc.start {
        return U256::ZERO;
    }
    let upfront = amount * U256::from(sc.upfront_bps.min(10_000)) / U256::from(10_000u32);
    let first = sc.start + u64::from(sc.cliff);
    if t < first {
        return upfront;
    }
    if sc.periods <= 1 || sc.interval == 0 {
        return amount;
    }
    let parts = (t - first) / u64::from(sc.interval) + 1;
    if parts >= u64::from(sc.periods) { amount } else { upfront + (amount - upfront) * U256::from(parts) / U256::from(sc.periods) }
}

/// When the next part unlocks after `t` (None once all of it has).
pub fn next_unlock(sc: &SlotSchedule, t: u64) -> Option<u64> {
    if t < sc.start && sc.upfront_bps > 0 {
        return Some(sc.start);
    }
    let first = sc.start + u64::from(sc.cliff);
    if t < first {
        return Some(first);
    }
    if sc.periods <= 1 || sc.interval == 0 {
        return None;
    }
    let k = (t - first) / u64::from(sc.interval) + 1;
    (k < u64::from(sc.periods)).then(|| first + k * u64::from(sc.interval))
}

/// The share as it stands at `t`: unlocked, claimable and the next part worked out again, the rest
/// as Accordo gave it.
pub fn at(s: &DropSlot, decimals: u8, t: u64) -> DropSlot {
    let Ok(amount) = U256::from_str_radix(s.amount_wei.trim(), 10) else { return s.clone() };
    let unlocked = unlocked_at(amount, &s.schedule, t);
    let claimable = unlocked.saturating_sub(wei(&s.claimed_wei));
    DropSlot {
        unlocked: format_units(unlocked, decimals),
        claimable: format_units(claimable, decimals),
        claimable_wei: claimable.to_string(),
        next_unlock_at: next_unlock(&s.schedule, t),
        ..s.clone()
    }
}

/// Paid out in parts (not all at once).
pub fn vests(sc: &SlotSchedule) -> bool {
    sc.periods > 1 || sc.cliff > 0 || (sc.upfront_bps > 0 && sc.upfront_bps < 10_000)
}

/// The parts a share unlocks in — (share of the amount, when): the upfront share at start, then
/// equal parts.
fn parts(sc: &SlotSchedule) -> Vec<(f64, u64)> {
    let up = f64::from(sc.upfront_bps.min(10_000)) / 10_000.0;
    let n = sc.periods.max(1);
    let first = sc.start + u64::from(sc.cliff);
    let mut out = Vec::new();
    if up > 0.0 {
        out.push((up, sc.start));
    }
    if up < 1.0 {
        out.extend((0..n).map(|k| ((1.0 - up) / f64::from(n), first + u64::from(k) * u64::from(sc.interval))));
    }
    out
}

/// "monthly" / "weekly" / "every 10 days".
fn cadence(interval: u32) -> String {
    let d = f64::from(interval) / 86_400.0;
    let near = |x: f64| (d - x).abs() / x < 0.03;
    if near(1.0) {
        "daily".into()
    } else if near(7.0) {
        "weekly".into()
    } else if near(30.4375) || near(30.0) {
        "monthly".into()
    } else if near(91.3125) || near(91.0) {
        "quarterly".into()
    } else if near(365.25) || near(365.0) {
        "yearly".into()
    } else {
        format!("every {} days", d.round().max(1.0))
    }
}

/// The plan in words: "25% right away, then 3 monthly parts: Dec 20, 2026 → Feb 18, 2027".
pub fn plan(sc: &SlotSchedule, t: u64) -> String {
    let first = sc.start + u64::from(sc.cliff);
    let up = (sc.upfront_bps > 0 && sc.upfront_bps < 10_000).then(|| format!("{}% {}", pct(sc.upfront_bps), if sc.start <= t { "right away".to_string() } else { format!("on {}", day(sc.start)) }));
    let rest = if sc.periods > 1 {
        let last = first + u64::from(sc.periods - 1) * u64::from(sc.interval);
        format!("{} {} parts: {} → {}", sc.periods, cadence(sc.interval), day(first), day(last))
    } else {
        format!("{} on {}", if up.is_some() { "the rest" } else { "All" }, day(first))
    };
    match up {
        Some(u) => format!("{u}, then {rest}"),
        None => rest,
    }
}

fn ended(d: &DropInfo, t: u64) -> bool {
    d.refunded || (d.expires_at > 0 && d.expires_at < t)
}

/// Whether the card needs the clock: a part of one of the reader's shares unlocks within a day
/// (its countdown moves, and Claim lights up when it comes).
pub fn ticking(card: &DropCard, t: u64) -> bool {
    let live = card.info.as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|d| !ended(d, t));
    live && card.slots.iter().flatten().any(|s| !s.claimed && next_unlock(&s.schedule, t).is_some_and(|n| n - t < 86_400))
}

/// Puts the contract's word into the card: the drop's terms, and per share what it paid so far. A
/// share whose proof doesn't lead to the root isn't the reader's: it goes.
pub fn apply(card: &mut DropCard, c: OnChain) {
    let dec = c.decimals;
    if let Some(Ok(d)) = card.info.as_mut() {
        d.creator_address = c.creator.to_checksum(None);
        d.token_address = (c.token != Address::ZERO).then(|| c.token.to_checksum(None));
        if let Some(symbol) = c.symbol.clone() {
            d.token = symbol;
        }
        d.token_decimals = dec;
        d.total_wei = c.total.to_string();
        d.total = format_units(c.total, dec);
        d.expires_at = c.expires_at;
        d.refunded = c.refunded;
        d.fee_bps = c.fee_bps;
    }
    if let Some(slots) = card.slots.as_mut() {
        slots.retain(|s| c.paid.contains_key(&s.slot));
        for s in slots.iter_mut() {
            let (amount, paid) = (wei(&s.amount_wei), c.paid[&s.slot]);
            s.amount = format_units(amount, dec);
            s.net_amount = format_units(after_fee(amount, c.fee_bps), dec);
            s.claimed_wei = paid.to_string();
            s.claimed_amount = format_units(paid, dec);
            s.claimed = paid >= amount;
        }
    }
    card.unproven = c.unproven;
    card.chain = Some(Ok(()));
}

// ── the view ──

fn fill<'a>(color: Color, portion: u16) -> El<'a> {
    container(space()).width(Length::FillPortion(portion)).height(Fill).style(move |_| container::Style { background: Some(color.into()), border: border(Color::TRANSPARENT, 0.0, 2.0), ..Default::default() }).into()
}

/// One part of the bar: `claimed` and `ready` are how much of it (0–1, ready ≥ claimed).
fn segment<'a>(weight: u16, claimed: f64, ready: f64) -> El<'a> {
    let c = (claimed.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let u = ((ready.clamp(0.0, 1.0) * 1000.0).round() as u16).max(c);
    let mut inner = row![];
    for (w, color) in [(c, CLAIMED), (u - c, READY)] {
        if w > 0 {
            inner = inner.push(fill(color, w));
        }
    }
    if u < 1000 {
        inner = inner.push(container(space()).width(Length::FillPortion(1000 - u)));
    }
    container(inner.height(Fill)).width(Length::FillPortion(weight.max(1))).height(8).clip(true).style(boxed(LOCKED, Color::TRANSPARENT, 2.0)).into()
}

/// A segment per part (claimed / ready / locked), one smooth bar when there are too many.
fn parts_bar<'a>(s: &DropSlot) -> El<'a> {
    let num = |v: &str| v.parse::<f64>().unwrap_or(0.0);
    let total = num(&s.amount);
    if total <= 0.0 {
        return space().into();
    }
    let (claimed, unlocked) = (num(&s.claimed_amount) / total, num(&s.unlocked) / total);
    let parts = parts(&s.schedule);
    if parts.len() > MAX_SEGMENTS {
        return row![segment(1, claimed, unlocked)].into();
    }
    let mut bar = row![].spacing(3);
    let mut from = 0.0;
    for (share, _) in parts {
        bar = bar.push(segment((share * 1000.0).round() as u16, (claimed - from) / share, (unlocked - from) / share));
        from += share;
    }
    bar.into()
}

fn dot<'a>(c: Color, round: bool) -> El<'a> {
    container(space()).width(8).height(8).style(boxed(if c == LOCKED { rgba(0xffffff, 0.18) } else { c }, Color::TRANSPARENT, if round { 4.0 } else { 2.0 })).into()
}

fn legend<'a>(c: Color, label: &'static str, value: String) -> El<'a> {
    row![dot(c, false), text(label).size(11.5).color(TEXT_SECONDARY), text(value).size(11.5).font(BOLD).color(Color::WHITE)].spacing(5).align_y(Alignment::Center).into()
}

fn stat<'a>(label: &'static str, value: String, sub: Option<(String, Color)>) -> El<'a> {
    let mut c = column![text(label).size(10).font(BOLD).color(FAINT), text(value).size(14).font(BOLD).color(Color::WHITE)].spacing(3);
    if let Some((s, color)) = sub {
        c = c.push(text(s).size(11).color(color));
    }
    container(c).width(Fill).padding([9, 12]).style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.06), 10.0)).into()
}

fn pill<'a>(label: &'static str, c: Color) -> El<'a> {
    container(row![dot(c, true), text(label).size(11.5).font(SEMIBOLD).color(c)].spacing(5).align_y(Alignment::Center))
        .padding([3, 9])
        .style(boxed(Color { a: 0.1, ..c }, Color { a: 0.25, ..c }, 999.0))
        .into()
}

fn linear(angle: f32, stops: &[(f32, Color)]) -> Background {
    let mut g = Linear::new(Radians(angle));
    for (at, c) in stops {
        g = g.add_stop(*at, *c);
    }
    Background::Gradient(Gradient::Linear(g))
}

/// The card's frame: the stripe on top (CSS 90deg), the faint fade down (180deg). iced clips to the
/// box, not its rounded corners: the stripe stays between them.
fn frame<'a>(content: Column<'a, Message>) -> El<'a> {
    let stripe = container(space().width(Fill).height(3)).style(|_| container::Style {
        background: Some(linear(std::f32::consts::FRAC_PI_2, &[(0.0, INDIGO), (0.5, VIOLET), (1.0, GREEN)])),
        border: border(Color::TRANSPARENT, 0.0, 1.5),
        ..Default::default()
    });
    container(column![container(stripe).padding([0, 14]), container(content).padding([11, 14])])
        .width(WIDTH)
        .clip(true)
        .style(|_| container::Style {
            background: Some(linear(std::f32::consts::PI, &[(0.0, rgba(0x6366f1, 0.12)), (1.0, rgba(0x6366f1, 0.03))])),
            border: border(rgba(0x818cf8, 0.3), 1.0, 14.0),
            ..Default::default()
        })
        .into()
}

/// `wallets`: the reader's wallets (address lowercase, name).
pub fn view<'a>(a: &'a Value, cards: &'a Cards, wallets: &'a [(String, String)]) -> El<'a> {
    let Some(id) = drop_id(a) else { return note_line("This drop card has no drop id".into()) };
    let card = cards.drops.get(&id);
    let info = card.and_then(|c| c.info.as_ref());
    let d = info.and_then(|r| r.as_ref().ok());
    let loading = card.is_none_or(|c| c.loading);
    let t = now();
    let token = d.map(|d| d.token.clone()).or_else(|| a.get("token").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    let title = d.and_then(|d| d.title.clone()).filter(|t| !t.trim().is_empty()).or_else(|| a.get("title").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| format!("{token} drop"));
    let is_ended = d.is_some_and(|d| ended(d, t));

    let sub = match (info, d) {
        (None, _) => "Loading…".to_string(),
        (Some(Err(_)), _) => "This drop isn't available".to_string(),
        (_, Some(d)) => format!("from {} · {}", crate::wallets::short_address(&d.creator_address), d.chain),
        _ => String::new(),
    };
    let initial = token.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let avatar = container(text(initial).size(16).font(EXTRABOLD).color(Color::WHITE)).width(38).height(38).center(38).style(|_| container::Style {
        background: Some(linear(3.0 * std::f32::consts::FRAC_PI_4, &[(0.0, INDIGO), (1.0, GREEN)])),
        border: border(Color::TRANSPARENT, 0.0, 11.0),
        ..Default::default()
    });
    let mut head = row![avatar, column![text(title).size(14).font(BOLD).color(Color::WHITE), text(sub).size(11.5).color(TEXT_SECONDARY)].spacing(2).width(Fill)].spacing(10).align_y(Alignment::Center);
    if let Some(d) = d {
        head = head.push(if d.refunded { pill("Closed", MUTED) } else if is_ended { pill("Ended", DANGER_TEXT) } else { pill("Live", GREEN) });
    }
    head = head.push(button(icon(icons::refresh_cw(), 12, if loading { FAINT } else { MUTED })).padding(6).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press_maybe((!loading).then(|| fm(FundMsg::Reload(id.clone())))));
    let mut col: Column<'a, Message> = column![head].spacing(12);

    let Some(d) = d else { return frame(col) };
    let deadline = if d.expires_at == 0 {
        stat("CLAIM UNTIL", "No deadline".into(), None)
    } else {
        let soon = (!is_ended && d.expires_at.saturating_sub(t) < 7 * 86_400).then(|| (left(d.expires_at, t), AMBER_TEXT));
        stat(if is_ended { "CLAIMS CLOSED" } else { "CLAIM UNTIL" }, day(d.expires_at), soon)
    };
    col = col.push(row![stat("POOL", format!("{} {}", pretty(&d.total), d.token), None), deadline].spacing(8));
    if let Some(n) = d.note.as_deref().filter(|n| !n.trim().is_empty()) {
        col = col.push(text(crate::components::ui::truncate(n, 240)).size(12.5).color(rgba(0xffffff, 0.85)));
    }

    let hint = |s: &'static str| -> El<'a> { container(text(s).size(12.5).color(TEXT_SECONDARY)).width(Fill).padding([10, 12]).style(boxed(rgba(0xffffff, 0.02), rgba(0xffffff, 0.08), 10.0)).into() };
    match card.and_then(|c| c.slots.as_ref()) {
        None if wallets.is_empty() => col = col.push(hint("Add a wallet to see if you're in this drop")),
        None if loading => col = col.push(text("Checking your wallets…").size(12).color(MUTED)),
        None => col = col.push(button(text("Check my wallets").size(12.5).color(INDIGO_SOFT)).padding(0).style(link_button).on_press(fm(FundMsg::Reload(id.clone())))),
        Some(slots) if slots.is_empty() => col = col.push(hint("None of your wallets is in this drop")),
        Some(slots) => {
            for s in slots {
                col = col.push(share(d, at(s, d.token_decimals, t), wallets, is_ended, t));
            }
        }
    }
    let Some(card) = card else { return frame(col) };
    for a in &card.unproven {
        col = col.push(text(format!("Accordo lists a share for {} that the contract doesn't confirm — not shown", crate::wallets::short_address(a))).size(11.5).color(AMBER_TEXT));
    }
    col = col.push(check_other(&id, &card.check, d));
    let source: Option<El<'a>> = match &card.chain {
        Some(Ok(())) => Some(row![icon(icons::circle_check(), 12, FAINT), text(format!("Checked on-chain · {}", d.chain)).size(11).color(FAINT)].spacing(5).align_y(Alignment::Center).into()),
        Some(Err(e)) => Some(row![icon(icons::info(), 12, AMBER_TEXT), text(format!("Couldn't read the contract ({e}) — this is Accordo's copy")).size(11).color(AMBER_TEXT)].spacing(5).align_y(Alignment::Center).into()),
        None if loading => Some(text("Checking the contract…").size(11).color(FAINT).into()),
        None => None,
    };
    if let Some(s) = source {
        col = col.push(s);
    }
    frame(col)
}

/// One of the reader's shares: how much they get, the parts as they unlock, and Claim.
fn share<'a>(d: &'a DropInfo, s: DropSlot, wallets: &[(String, String)], is_ended: bool, t: u64) -> El<'a> {
    let dec = d.token_decimals;
    let name = wallets.iter().find(|(a, _)| a.eq_ignore_ascii_case(&s.address)).map(|(_, n)| n.clone()).unwrap_or_else(|| "Wallet".into());
    let who = container(text(format!("{name} · {}", crate::wallets::short_address(&s.address))).size(11).color(TEXT_BODY)).padding([3, 8]).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 999.0));
    let amount = wei(&s.amount_wei);
    let mut col = column![
        row![text("YOUR SHARE").size(10).font(BOLD).color(FAINT), space().width(Fill), who].align_y(Alignment::Center),
        row![text(pretty(&s.net_amount)).size(26).font(EXTRABOLD).color(Color::WHITE), text(d.token.clone()).size(14).font(SEMIBOLD).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::End),
    ]
    .spacing(4);
    if d.fee_bps > 0 {
        col = col.push(text(format!("{} {} − {}% Accordo fee", units(amount, dec), d.token, pct(d.fee_bps))).size(11.5).color(TEXT_SECONDARY));
    }

    let vesting = vests(&s.schedule);
    let (claimed, ready) = (wei(&s.claimed_wei), wei(&s.claimable_wei));
    if vesting {
        let locked = amount.saturating_sub(claimed).saturating_sub(ready);
        col = col.push(space().height(6)).push(parts_bar(&s)).push(
            row![legend(CLAIMED, "Claimed", units(claimed, dec)), legend(READY, "Ready", units(ready, dec)), legend(LOCKED, "Locked", units(locked, dec))].spacing(14).align_y(Alignment::Center),
        );
        col = col.push(text(plan(&s.schedule, t)).size(12).color(TEXT_SECONDARY));
    }
    if let Some(n) = s.next_unlock_at.filter(|_| !s.claimed && !is_ended) {
        col = col.push(
            container(
                row![
                    icon(icons::calendar_clock(), 14, INDIGO_SOFT),
                    text(if vesting { "Next part" } else { "Unlocks" }).size(12.5).color(TEXT_BODY),
                    text(day(n)).size(12.5).font(BOLD).color(Color::WHITE),
                    space().width(Fill),
                    text(left(n, t)).size(12).color(INDIGO_SOFT),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([8, 10])
            .style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.06), 10.0)),
        );
    }

    let wide = |content: El<'a>, bg: Color, line: Color| -> El<'a> { container(content).width(Fill).center_x(Fill).padding([10, 12]).style(boxed(bg, line, 10.0)).into() };
    let action: El<'a> = if s.claimed {
        wide(
            row![icon(icons::circle_check(), 15, GREEN), text(if vesting { "Fully claimed" } else { "Claimed" }).size(13).font(BOLD).color(GREEN)].spacing(6).align_y(Alignment::Center).into(),
            rgba(0x34d399, 0.08),
            rgba(0x34d399, 0.3),
        )
    } else if is_ended {
        let why = if d.refunded { "The creator took the rest back".to_string() } else { format!("Claims closed on {}", day(d.expires_at)) };
        wide(text(why).size(12.5).color(AMBER_TEXT).into(), rgba(0xf59e0b, 0.06), rgba(0xf59e0b, 0.25))
    } else if ready > U256::ZERO {
        let caption = format!("Claim {} {}", units(after_fee(ready, d.fee_bps), dec), d.token);
        button(container(text(caption).size(13.5).font(BOLD)).center_x(Fill)).width(Fill).padding([10, 12]).style(tone_button(Tone::Accent)).on_press(Message::ClaimDrop(Box::new(d.clone()), Box::new(s.clone()))).into()
    } else {
        let caption = s.next_unlock_at.map(|n| format!("Next part on {}", day(n))).unwrap_or_else(|| "Nothing to claim".into());
        wide(text(caption).size(12.5).color(TEXT_SECONDARY).into(), rgba(0xffffff, 0.04), Color::TRANSPARENT)
    };
    col = col.push(space().height(4)).push(action);
    container(col.spacing(8)).width(Fill).padding(12).style(boxed(rgba(0x6366f1, 0.06), rgba(0x6366f1, 0.25), 12.0)).into()
}

/// "Check another wallet" (DropClaimPage.tsx): whether some address is in the drop. Shows only;
/// claiming needs that wallet.
fn check_other<'a>(id: &str, c: &'a CheckOther, d: &'a DropInfo) -> El<'a> {
    let toggle = button(
        row![text("Check another wallet").size(12).color(TEXT_SECONDARY), icon(if c.open { icons::chevron_up() } else { icons::chevron_down() }, 12, TEXT_SECONDARY)].spacing(4).align_y(Alignment::Center),
    )
    .padding(0)
    .style(link_button)
    .on_press(fm(FundMsg::CheckOpen(id.to_string())));
    let mut col = column![toggle].spacing(8);
    if !c.open {
        return col.into();
    }
    let valid = is_address(&c.address);
    let (a, b) = (id.to_string(), id.to_string());
    let field = text_input("0x…", &c.address)
        .on_input(move |v| fm(FundMsg::CheckAddress(a.clone(), v)))
        .on_submit_maybe(valid.then(|| fm(FundMsg::Check(b.clone()))))
        .font(Font::MONOSPACE)
        .size(12.5)
        .padding([8, 10])
        .style(field);
    let go = button(icon(icons::search(), 14, Color::WHITE)).padding([8, 10]).style(tone_button(Tone::Dark)).on_press_maybe((valid && !c.busy).then(|| fm(FundMsg::Check(id.to_string()))));
    col = col.push(row![field, go].spacing(6).align_y(Alignment::Center));
    if c.busy {
        col = col.push(text("Checking…").size(12).color(MUTED));
    } else if let Some((addr, r)) = &c.result {
        let who = crate::wallets::short_address(addr);
        let (line, color) = match r {
            Ok(Some(s)) => {
                let how = if vests(&s.schedule) { format!(", in parts — {}", plan(&s.schedule, now())) } else { String::new() };
                (format!("{who} gets {} {}{how}{}", pretty(&s.net_amount), d.token, if s.claimed { " — already claimed" } else { " — connect it to claim" }), GREEN)
            }
            Ok(None) => (format!("{who} isn't in this drop"), TEXT_SECONDARY),
            Err(e) => (format!("Couldn't check: {e}"), AMBER_TEXT),
        };
        col = col.push(text(line).size(12.5).color(color));
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONTH: u32 = 2_629_800; // 30.4375 days

    fn sched(start: u64, interval: u32, periods: u16, cliff: u32, upfront_bps: u16) -> SlotSchedule {
        SlotSchedule { start, interval, periods, cliff, upfront_bps }
    }

    #[test]
    fn unlocks_like_the_contract() {
        let amount = U256::from(2_000_000u64);
        // 25% at start, then 3 parts a month apart, the first a month after start.
        let sc = sched(1_000, MONTH, 3, MONTH, 2_500);
        assert_eq!(unlocked_at(amount, &sc, 999), U256::ZERO);
        assert_eq!(unlocked_at(amount, &sc, 1_000), U256::from(500_000u64));
        assert_eq!(next_unlock(&sc, 1_000), Some(1_000 + u64::from(MONTH)));
        assert_eq!(unlocked_at(amount, &sc, 1_000 + u64::from(MONTH)), U256::from(1_000_000u64));
        assert_eq!(unlocked_at(amount, &sc, 1_000 + 2 * u64::from(MONTH)), U256::from(1_500_000u64));
        assert_eq!(unlocked_at(amount, &sc, 1_000 + 3 * u64::from(MONTH)), amount);
        assert_eq!(next_unlock(&sc, 1_000 + 3 * u64::from(MONTH)), None);
        // All at once, no schedule: unlocked from the start, nothing next.
        let once = sched(0, 0, 1, 0, 0);
        assert_eq!(unlocked_at(amount, &once, 5), amount);
        assert_eq!(next_unlock(&once, 5), None);
        assert!(!vests(&once) && vests(&sc));
    }

    #[test]
    fn share_at_a_later_time() {
        let s = DropSlot { amount: "2".into(), amount_wei: "2000000".into(), claimed_wei: "500000".into(), schedule: sched(1_000, MONTH, 3, MONTH, 2_500), ..Default::default() };
        let later = at(&s, 6, 1_000 + u64::from(MONTH) + 5);
        assert_eq!((later.unlocked.as_str(), later.claimable.as_str(), later.claimable_wei.as_str()), ("1", "0.5", "500000"));
        assert_eq!(later.next_unlock_at, Some(1_000 + 2 * u64::from(MONTH)));
    }

    #[test]
    fn parts_and_words() {
        let sc = sched(1_000, MONTH, 3, MONTH, 2_500);
        let p = parts(&sc);
        assert_eq!(p.len(), 4);
        assert!((p[0].0 - 0.25).abs() < 1e-9 && (p[1].0 - 0.25).abs() < 1e-9);
        assert!(plan(&sc, 2_000).starts_with("25% right away, then 3 monthly parts: "));
        assert!(plan(&sched(1_000, 0, 1, 86_400, 0), 0).starts_with("All on "));
        assert_eq!(cadence(7 * 86_400), "weekly");
        assert_eq!(cadence(10 * 86_400), "every 10 days");
        assert_eq!(pct(1_250), "12.5");
    }

    #[test]
    fn amounts_read_well() {
        assert_eq!(pretty("1234567.123456789"), "1,234,567.123456");
        assert_eq!(pretty("0.95"), "0.95");
        assert_eq!(pretty("100"), "100");
        assert_eq!(pretty("0.0000001"), "<0.000001");
        assert_eq!(after_fee(U256::from(1_000_000u64), 500), U256::from(950_000u64));
    }
}
