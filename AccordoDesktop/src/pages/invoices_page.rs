// SPDX-License-Identifier: GPL-3.0-or-later
//! Invoices (InvoicesPage.tsx): issue an invoice, incoming ones and your own.

use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Alignment, Color, Fill};

use super::Effect;
use crate::api::data::Token;
use crate::app::Message;
use crate::components::finance::records::{self, ListKind, Lists};
use crate::components::finance::duration_picker::{self, Duration};
use crate::components::finance::{fee_picker, token_select};
use crate::components::contact_picker::{self, Target};
use crate::components::form::{form_card, input, label, note_input, title};
use crate::components::page_tabs::{self, Tab};
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

const DUE_PRESETS: [Duration; 5] = [Duration::Hours(24), Duration::Days(7), Duration::Days(14), Duration::Days(30), Duration::Days(90)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoicesTab {
    Issue,
    Incoming,
    Yours,
}

#[derive(Debug, Clone)]
pub enum InvoicesMsg {
    Tab(InvoicesTab),
    Merchant(String),
    Token(Token),
    Client(String),
    BilledWallet(String),
    /// A contact picked for the merchant, payer or payer-wallet field (name, address).
    Contact(Target, String, String),
    /// Split between several payers: the payer above becomes the first of two.
    Split,
    PayerName(usize, String),
    PayerWallet(usize, String),
    PayerShare(usize, String),
    AddPayer,
    RemovePayer(usize),
    SplitEqually(bool),
    Amount(String),
    AllowRedact(bool),
    Fee(u32),
    Note(String),
    Due(Duration),
    Submit,
}

pub struct State {
    pub tab: InvoicesTab,
    pub merchant: String,
    pub token: Option<Token>,
    pub client: String,
    pub billed_wallet: String,
    pub amount: String,
    pub allow_redact: bool,
    pub fee_bps: u32,
    pub note: String,
    pub due: Duration,
    /// A bill split (two or more): one invoice per payer, created together. Empty: one payer
    /// (`client` / `billed_wallet`).
    pub payers: Vec<Payer>,
    /// Shares are the amount divided evenly; off: each payer's own `share`.
    pub split_equally: bool,
}

/// One payer of a split bill.
#[derive(Debug, Clone, Default)]
pub struct Payer {
    /// Username / email / ID.
    pub name: String,
    /// Only this wallet may pay; empty = anyone.
    pub wallet: String,
    /// Their amount, when not split equally.
    pub share: String,
}

/// Most payers one split takes (createInvoices' limit).
const MAX_PAYERS: usize = crate::evm::invoices::MAX_BATCH;

impl Default for State {
    fn default() -> Self {
        Self {
            tab: InvoicesTab::Issue,
            merchant: String::new(),
            token: None,
            client: String::new(),
            billed_wallet: String::new(),
            amount: String::new(),
            allow_redact: false,
            fee_bps: fee_picker::DEFAULT_BPS,
            note: String::new(),
            due: Duration::Days(14),
            payers: Vec::new(),
            split_equally: true,
        }
    }
}

impl State {
    pub fn split(&self) -> bool {
        self.payers.len() >= 2
    }

    /// Each payer's share in base units (None: not a valid amount yet). Split equally, the
    /// remainder goes to the first payers, so the shares always add up to the amount.
    pub fn shares(&self) -> Vec<Option<u128>> {
        let decimals = self.token.as_ref().map_or(18, |t| t.decimals);
        let units = |v: &str| crate::evm::client::parse_units(v, decimals).ok().and_then(|u| u128::try_from(u).ok());
        if !self.split_equally {
            return self.payers.iter().map(|p| units(&p.share)).collect();
        }
        let n = self.payers.len() as u128;
        match units(&self.amount).filter(|t| *t > 0 && n > 0) {
            Some(total) => (0..n).map(|i| Some(total / n + u128::from(i < total % n))).collect(),
            None => vec![None; self.payers.len()],
        }
    }
}

pub fn update(state: &mut State, msg: InvoicesMsg) -> Effect {
    match msg {
        InvoicesMsg::Tab(tab) => state.tab = tab,
        InvoicesMsg::Merchant(v) => return typed(&mut state.merchant, v, Target::InvoiceMerchant),
        InvoicesMsg::Token(t) => state.token = Some(t),
        InvoicesMsg::Client(v) => return typed(&mut state.client, v, Target::InvoiceClient),
        InvoicesMsg::BilledWallet(v) => return typed(&mut state.billed_wallet, v, Target::InvoiceWallet),
        InvoicesMsg::Contact(target, name, address) => match target {
            Target::InvoicePayerName(i) => {
                if let Some(p) = state.payers.get_mut(i) {
                    p.name = name;
                    p.wallet = address;
                }
            }
            Target::InvoicePayerWallet(i) => {
                if let Some(p) = state.payers.get_mut(i) {
                    p.wallet = address;
                    if p.name.trim().is_empty() {
                        p.name = name;
                    }
                }
            }
            Target::InvoiceMerchant => state.merchant = address,
            Target::InvoiceClient => {
                state.client = name;
                if state.billed_wallet.trim().is_empty() {
                    state.billed_wallet = address;
                }
            }
            Target::InvoiceWallet => state.billed_wallet = address,
            _ => {}
        },
        InvoicesMsg::Split => {
            // A private room's / DM's own id isn't a payer anyone typed.
            let first = if state.client.starts_with("@priv_") || state.client.starts_with("@dm:") { String::new() } else { state.client.clone() };
            state.payers = vec![Payer { name: first, wallet: state.billed_wallet.clone(), share: String::new() }, Payer::default()];
            state.allow_redact = false; // every share is a fixed amount
        }
        InvoicesMsg::PayerName(i, v) => {
            if let Some(p) = state.payers.get_mut(i) {
                return typed(&mut p.name, v, Target::InvoicePayerName(i));
            }
        }
        InvoicesMsg::PayerWallet(i, v) => {
            if let Some(p) = state.payers.get_mut(i) {
                return typed(&mut p.wallet, v, Target::InvoicePayerWallet(i));
            }
        }
        InvoicesMsg::PayerShare(i, v) => {
            if let Some(p) = state.payers.get_mut(i) {
                p.share = v.replace(',', ".");
            }
        }
        InvoicesMsg::AddPayer if state.payers.len() < MAX_PAYERS => state.payers.push(Payer::default()),
        InvoicesMsg::AddPayer => {}
        InvoicesMsg::RemovePayer(i) => {
            if i < state.payers.len() {
                state.payers.remove(i);
            }
            // One left: back to a single payer.
            if state.payers.len() < 2 {
                let last = state.payers.pop().unwrap_or_default();
                state.client = last.name;
                state.billed_wallet = last.wallet;
            }
        }
        InvoicesMsg::SplitEqually(v) => state.split_equally = v,
        InvoicesMsg::Amount(v) => state.amount = v,
        InvoicesMsg::AllowRedact(v) => state.allow_redact = v,
        InvoicesMsg::Fee(b) => state.fee_bps = b,
        InvoicesMsg::Note(v) => state.note = v,
        InvoicesMsg::Due(Duration::Custom) => return Effect::PickTime(crate::components::time_picker::Purpose::InvoiceDue, match state.due { Duration::At(t) => Some(t), _ => None }),
        InvoicesMsg::Due(d) => state.due = d,
        InvoicesMsg::Submit => {
            return match draft(state, chrono::Utc::now().timestamp()) {
                Ok(d) => Effect::IssueInvoice(Box::new(d)),
                Err(e) => Effect::Error(e),
            }
        }
    }
    Effect::None
}

/// A field's new text; an @ typed at its start (or after a space) opens the contacts for it.
fn typed(field: &mut String, v: String, target: Target) -> Effect {
    let at = contact_picker::opens(field, &v);
    *field = v;
    if at { Effect::Contacts(target) } else { Effect::None }
}

/// handleIssueInvoice's checks, in its order and with its words.
pub fn draft(state: &State, now: i64) -> Result<crate::flows::invoices::Draft, String> {
    use alloy_primitives::Address;
    let merchant = state.merchant.trim();
    if merchant.is_empty() {
        return Err("Merchant address is required!".into());
    }
    if !merchant.starts_with("0x") || merchant.len() != 42 {
        return Err("Invalid Merchant EVM address (must start with 0x)!".into());
    }
    let merchant = crate::evm::client::address(merchant).map_err(|_| "Invalid Merchant address checksum!".to_string())?;
    let Some(token) = state.token.clone() else { return Err("Choose a token first".into()) };
    let split = state.split();
    let units = if split {
        0 // the sum of the shares, below
    } else {
        let units = crate::evm::client::parse_units(&state.amount, token.decimals).ok().filter(|u| !u.is_zero()).ok_or("Enter a valid amount")?;
        units.try_into().map_err(|_| "That amount is too large".to_string())?
    };
    let due_at = match state.due {
        Duration::Never => 0,
        d => match d.ends_at(now) {
            Some(t) if t > now + 60 => t as u64,
            _ => return Err("Pick a due date in the future".into()),
        },
    };
    let fee_bps = state.fee_bps.min(u32::from(u16::MAX)) as u16;
    if split {
        // issueSplit's checks, with its words.
        if state.payers.iter().any(|p| p.name.trim().is_empty()) {
            return Err("Name every payer (username, email or ID)".into());
        }
        let mut wallets = Vec::with_capacity(state.payers.len());
        for p in &state.payers {
            let w = p.wallet.trim();
            wallets.push(if w.is_empty() { Address::ZERO } else { crate::evm::client::address(w).map_err(|_| format!("Invalid wallet address for {}", p.name.trim()))? });
        }
        let shares = state.shares();
        if shares.iter().any(|u| u.is_none_or(|u| u == 0)) {
            return Err(if state.split_equally { "Enter the total amount" } else { "Enter every payer's share" }.into());
        }
        let shares: Vec<u128> = shares.into_iter().flatten().collect();
        let total = shares.iter().try_fold(0u128, |a, b| a.checked_add(*b)).ok_or("That amount is too large")?;
        let fmt = |u: u128| crate::evm::client::format_units(alloy_primitives::U256::from(u), token.decimals);
        let split = state.payers.iter().zip(wallets).zip(&shares).map(|((p, payer), u)| crate::flows::invoices::Share { client: p.name.trim().to_string(), payer, units: *u, amount: fmt(*u) }).collect();
        return Ok(crate::flows::invoices::Draft { merchant, amount: fmt(total), token, client: String::new(), payer: Address::ZERO, units: total, fee_bps, note: state.note.clone(), due_at, allow_redact: false, split });
    }
    let w = state.billed_wallet.trim();
    let payer = if w.starts_with("0x") { crate::evm::client::address(w).map_err(|_| "Invalid payer wallet address".to_string())? } else { Address::ZERO };
    Ok(crate::flows::invoices::Draft {
        merchant,
        token,
        client: state.client.trim().to_string(),
        payer,
        amount: state.amount.trim().to_string(),
        units,
        fee_bps,
        note: state.note.clone(),
        due_at,
        allow_redact: state.allow_redact,
        split: Vec::new(),
    })
}

/// After an invoice went out: the next one starts clean (merchant, token, fee stay).
pub fn sent(state: &mut State) {
    state.amount.clear();
    state.note.clear();
    state.allow_redact = false;
    state.client.clear();
    state.billed_wallet.clear();
    state.payers.clear();
}

fn msg(m: InvoicesMsg) -> Message {
    Message::Invoices(m)
}

pub fn view<'a>(state: &'a State, tokens: token_select::Field, lists: &'a Lists, me: &'a str, cards: records::Cards<'a>) -> El<'a> {
    let tabs = page_tabs::view(vec![
        Tab { icon: icons::receipt(), label: "Issue Invoice", count: None, active: state.tab == InvoicesTab::Issue, on_press: msg(InvoicesMsg::Tab(InvoicesTab::Issue)) },
        Tab { icon: icons::arrow_down_left(), label: "Incoming Invoices", count: None, active: state.tab == InvoicesTab::Incoming, on_press: msg(InvoicesMsg::Tab(InvoicesTab::Incoming)) },
        Tab { icon: icons::arrow_up_right(), label: "Your Invoices", count: None, active: state.tab == InvoicesTab::Yours, on_press: msg(InvoicesMsg::Tab(InvoicesTab::Yours)) },
    ]);
    let body = match state.tab {
        InvoicesTab::Issue => issue_form(state, tokens),
        InvoicesTab::Incoming => records::list_view(lists.get(ListKind::IncomingInvoices), ListKind::IncomingInvoices, "Incoming Invoices", "Invoices billed to you.", "No incoming invoices yet", format!("When someone bills @{me}, the invoice appears here."), true, chrono::Utc::now().timestamp(), &cards),
        InvoicesTab::Yours => records::list_view(lists.get(ListKind::MyInvoices), ListKind::MyInvoices, "Your Invoices", "Invoices you issued, with their payment status.", "No invoices yet", "Invoices you issue show up here.".into(), false, chrono::Utc::now().timestamp(), &cards),
    };
    scrollable(container(column![tabs, body].spacing(24)).padding([20, 24]).width(Fill)).height(Fill).style(thin_scroll).into()
}

/// 33.333333333333333333 → "≈33.333333" (the exact share is on-chain and in the review).
fn short_amount(v: &str) -> String {
    match v.split_once('.') {
        Some((whole, frac)) if frac.len() > 6 => format!("≈{whole}.{}", &frac[..6]),
        _ => v.to_string(),
    }
}

/// The split's payers: name, wallet (empty = anyone) and share, one row each.
fn payers_view(state: &State) -> El<'_> {
    use iced::widget::{space, text_input};
    use iced::Length;
    let decimals = state.token.as_ref().map_or(18, |t| t.decimals);
    let shares = state.shares();
    let head = row![
        icon(icons::users(), 13, LABEL),
        label("Payers", true),
        text(format!("{}/{MAX_PAYERS}", state.payers.len())).size(11.5).color(TEXT_SECONDARY),
        space().width(Fill),
        checkbox(state.split_equally).label("Split equally").size(12).text_size(11.5).spacing(5).style(tick).on_toggle(|v| msg(InvoicesMsg::SplitEqually(v))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut rows = column![].spacing(6);
    for (i, p) in state.payers.iter().enumerate() {
        let share: El = if state.split_equally {
            let v = shares.get(i).copied().flatten().map(|u| short_amount(&crate::evm::client::format_units(alloy_primitives::U256::from(u), decimals))).unwrap_or_else(|| "—".into());
            container(text(v).size(12.5).color(TEXT_BODY)).width(96).align_x(iced::alignment::Horizontal::Right).into()
        } else {
            text_input("0.00", &p.share).on_input(move |v| msg(InvoicesMsg::PayerShare(i, v))).size(13.5).padding([10, 12]).width(96).style(field).into()
        };
        rows = rows.push(
            row![
                text((i + 1).to_string()).size(11.5).color(TEXT_SECONDARY).width(18),
                input("@username / email (@ contacts)", &p.name, move |v| msg(InvoicesMsg::PayerName(i, v))).width(Length::FillPortion(10)),
                input("0x… (empty = anyone · @ contacts)", &p.wallet, move |v| msg(InvoicesMsg::PayerWallet(i, v))).width(Length::FillPortion(13)),
                share,
                button(icon(icons::x(), 14, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(InvoicesMsg::RemovePayer(i))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    let add = button(row![icon(icons::plus(), 12, hex(0xc7d2fe)), text("Add payer").size(12).font(SEMIBOLD)].spacing(4).align_y(Alignment::Center))
        .padding([4, 10])
        .style(outline_indigo)
        .on_press_maybe((state.payers.len() < MAX_PAYERS).then(|| msg(InvoicesMsg::AddPayer)));
    column![
        head,
        rows,
        add,
        text("Each payer gets their own invoice for their share — payable only from the wallet shown (empty = anyone). All are created in one transaction.").size(11).color(TEXT_SECONDARY),
    ]
    .spacing(8)
    .into()
}

fn issue_form<'a>(state: &'a State, tokens: token_select::Field) -> El<'a> {
    let split = state.split();
    let payer: El = if split {
        payers_view(state)
    } else {
        column![
            column![label("Target Client / Payer (Username or Email)", true), input("Client username or email", &state.client, |v| msg(InvoicesMsg::Client(v)))].spacing(6),
            column![
                label("Billed Wallet Address (Optional — Single 0x... wallet)", false),
                input("0x... (Leave empty for any wallet)", &state.billed_wallet, |v| msg(InvoicesMsg::BilledWallet(v))),
            ]
            .spacing(6),
            button(row![icon(icons::users(), 13, hex(0xc7d2fe)), text("Split between several payers").size(12).font(BOLD)].spacing(6).align_y(Alignment::Center))
                .padding([5, 10])
                .style(outline_indigo)
                .on_press(msg(InvoicesMsg::Split)),
        ]
        .spacing(16)
        .into()
    };
    // Shares typed by hand: the amount is their sum (read-only).
    let amount: El = if split && !state.split_equally {
        let shares = state.shares();
        let sum = shares.iter().all(Option::is_some).then(|| shares.iter().flatten().sum::<u128>());
        let decimals = state.token.as_ref().map_or(18, |t| t.decimals);
        let sum = sum.map(|u| crate::evm::client::format_units(alloy_primitives::U256::from(u), decimals)).unwrap_or_default();
        iced::widget::text_input("The sum of the shares", &sum).size(13.5).padding([10, 12]).width(Fill).style(field).into()
    } else {
        input("100.00", &state.amount, |v| msg(InvoicesMsg::Amount(v))).into()
    };
    let mut amount_head = row![label("Amount due", true)].spacing(8).align_y(Alignment::Center);
    if !split {
        amount_head = amount_head.push(
            container(checkbox(state.allow_redact).label("Allow redact").size(12).text_size(11.5).spacing(5).style(tick).on_toggle(|v| msg(InvoicesMsg::AllowRedact(v))))
                .padding([2, 6])
                .style(boxed(rgba(0xffffff, 0.03), BORDER, 5.0)),
        );
    }
    let form = column![
        title("Issue an Invoice", "Request payment with on-chain settlement and deliverable escrow."),
        column![
            label("Merchant / Beneficiary Address", true),
            input("0x... (Beneficiary wallet that receives payment)", &state.merchant, |v| msg(InvoicesMsg::Merchant(v))),
            row![icon(icons::wallet(), 11, TEXT_SECONDARY), text("Funds from paid invoices are routed directly to this merchant wallet.").size(11).color(TEXT_SECONDARY)].spacing(5).align_y(Alignment::Center),
        ]
        .spacing(6),
        column![label("Token", false), token_select::view(tokens, state.token.as_ref(), crate::components::token_picker::Purpose::Invoice)].spacing(6),
        payer,
        column![amount_head, amount].spacing(6),
        fee_picker::view("Platform fee (deducted on payment)", state.fee_bps, None, false, |b| msg(InvoicesMsg::Fee(b))),
        column![label("Note / Description (optional)", false), note_input("Milestone description / payment memo", &state.note, |v| msg(InvoicesMsg::Note(v)))].spacing(6),
        duration_picker::view("Payment due date", &DUE_PRESETS, state.due, Some("No due date"), |d| msg(InvoicesMsg::Due(d))),
        button(iced::widget::center(text(if split { "Split the bill" } else { "Issue invoice" }).size(14).font(SEMIBOLD).color(Color::WHITE)).height(20))
            .width(Fill)
            .padding([10, 0])
            .style(tone_button(Tone::Accent))
            .on_press(msg(InvoicesMsg::Submit)),
    ]
    .spacing(16);
    form_card(form)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usdc() -> Token {
        Token { symbol: "USDC".into(), name: "USD Coin".into(), address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into(), decimals: 6, logo: None }
    }

    #[test]
    fn split_like_the_web() {
        let mut st = State { merchant: "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".into(), token: Some(usdc()), client: "zuko".into(), allow_redact: true, due: Duration::Never, ..State::default() };
        update(&mut st, InvoicesMsg::Split);
        assert!(st.split() && !st.allow_redact);
        assert_eq!(st.payers[0].name, "zuko");
        // Every payer needs a name, then the total.
        assert_eq!(draft(&st, 0).unwrap_err(), "Name every payer (username, email or ID)");
        update(&mut st, InvoicesMsg::PayerName(1, "bob".into()));
        assert_eq!(draft(&st, 0).unwrap_err(), "Enter the total amount");
        update(&mut st, InvoicesMsg::PayerWallet(1, "0x123".into()));
        st.amount = "10".into();
        assert_eq!(draft(&st, 0).unwrap_err(), "Invalid wallet address for bob");
        update(&mut st, InvoicesMsg::PayerWallet(1, "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC".into()));
        update(&mut st, InvoicesMsg::AddPayer);
        update(&mut st, InvoicesMsg::PayerName(2, "aang".into()));
        // 10 USDC in three: the remainder goes to the first ones, and it adds up.
        let d = draft(&st, 0).unwrap();
        let units: Vec<u128> = d.split.iter().map(|s| s.units).collect();
        assert_eq!(units, [3_333_334, 3_333_333, 3_333_333]);
        assert_eq!((d.units, d.amount.as_str(), d.allow_redact), (10_000_000, "10", false));
        assert_eq!(d.split[0].payer, alloy_primitives::Address::ZERO); // empty = anyone
        assert_eq!(d.split[1].amount, "3.333333");
        // Shares by hand: each one counts, the total is their sum.
        update(&mut st, InvoicesMsg::SplitEqually(false));
        assert_eq!(draft(&st, 0).unwrap_err(), "Enter every payer's share");
        for (i, v) in ["1,5", "2", "0.25"].into_iter().enumerate() {
            update(&mut st, InvoicesMsg::PayerShare(i, v.into()));
        }
        let d = draft(&st, 0).unwrap();
        assert_eq!((d.units, d.amount.as_str()), (3_750_000, "3.75"));
        // Down to one payer: back to the single form, with what they had.
        update(&mut st, InvoicesMsg::RemovePayer(0));
        update(&mut st, InvoicesMsg::RemovePayer(1));
        assert!(!st.split());
        assert_eq!((st.client.as_str(), st.billed_wallet.as_str()), ("bob", "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"));
        assert!(draft(&st, 0).unwrap().split.is_empty());
    }
}
