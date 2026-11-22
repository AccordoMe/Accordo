// SPDX-License-Identifier: GPL-3.0-or-later
//! Cheques (ChequesPage.tsx): write a cheque, incoming and outgoing ones.
//! The form is drawn and checked here (handleCreateCheque's checks); the app shows the review
//! window, signs with the Accordo wallet and saves the record.
//!
//! "Swap" writes a P2P swap cheque instead: you lock one token, and one counterparty (a wallet, or
//! anyone with the link) takes it by paying the amount of another token you ask for — in one
//! transaction, so neither side can walk off with the other's money.

use iced::widget::{button, column, container, row, scrollable, space, text, Column};
use iced::{Alignment, Color, Fill};

use alloy_primitives::{Address, U256};

use super::Effect;
use crate::api::data::Token;
use crate::app::Message;
use crate::components::finance::duration_picker::{self, Duration};
use crate::components::finance::records::{self, ListKind, Lists};
use crate::components::finance::{fee_picker, token_select};
use crate::components::contact_picker::{self, Target};
use crate::components::form::{form_card, input, label, note_input, title};
use crate::components::page_tabs::{self, Tab};
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

const MAX_RECIPIENTS: usize = 15;
/// The "Cheque / Swap" switch in the form. Off for now (the owner's call, 2026-11-21): writing swaps
/// works but isn't offered yet; taking a swap someone made still works.
const SWAPS_VISIBLE: bool = false;
const EXPIRY_PRESETS: [Duration; 5] = [Duration::Hours(24), Duration::Days(7), Duration::Days(14), Duration::Days(30), Duration::Days(90)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChequesTab {
    Write,
    Incoming,
    Outgoing,
}

/// One recipient row: a contact, and a wallet — or (`None`) an open slot any wallet can claim.
#[derive(Debug, Clone)]
pub struct Recipient {
    pub name: String,
    pub wallet: Option<String>,
}

impl Default for Recipient {
    /// A new row: a wallet to fill in. An open slot is only added on purpose ("Open slot").
    fn default() -> Self {
        Self { name: String::new(), wallet: Some(String::new()) }
    }
}

#[derive(Debug, Clone)]
pub enum ChequesMsg {
    Tab(ChequesTab),
    Token(Token),
    RecipientName(usize, String),
    RecipientWallet(usize, String),
    /// A contact picked for a recipient's name or wallet field (name, address).
    Contact(Target, String, String),
    SetWallet(usize),
    Remove(usize),
    AddRecipient,
    AddOpenSlot,
    Import,
    Amount(String),
    Fee(u32),
    Note(String),
    Expiry(Duration),
    /// A swap (true) or a plain cheque.
    Swap(bool),
    /// The token / amount a swap asks for in return.
    GetToken(Token),
    GetAmount(String),
    Submit,
}

pub struct State {
    pub tab: ChequesTab,
    pub token: Option<Token>,
    pub recipients: Vec<Recipient>,
    pub amount: String,
    pub fee_bps: u32,
    pub note: String,
    pub expiry: Duration,
    pub swap: bool,
    pub get_token: Option<Token>,
    pub get_amount: String,
}

/// What a swap asks for in return for the locked amount.
#[derive(Debug, Clone)]
pub struct SwapAsk {
    pub token: Token,
    pub amount: String,
    pub total: U256,
}

/// A checked form, ready for the review window.
#[derive(Debug, Clone)]
pub struct Draft {
    pub token: Token,
    /// (what the row is called — a name, @user, email or "" —, its wallet; ZERO = open slot)
    pub recipients: Vec<(String, Address)>,
    /// Who it shows up for (recipient_identificator, one per line; unique).
    pub identifiers: Vec<String>,
    pub amount: String,
    pub total: U256,
    pub fee_bps: u16,
    pub note: String,
    /// 0 = never.
    pub expires_at: u64,
    /// A swap: one recipient (or an open slot) takes `amount` of `token` by paying this.
    pub swap: Option<SwapAsk>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: ChequesTab::Write,
            token: None,
            recipients: vec![Recipient::default()],
            amount: String::new(),
            fee_bps: fee_picker::DEFAULT_BPS,
            note: String::new(),
            expiry: Duration::Days(30),
            swap: false,
            get_token: None,
            get_amount: String::new(),
        }
    }
}

pub fn update(state: &mut State, msg: ChequesMsg) -> Effect {
    match msg {
        ChequesMsg::Tab(tab) => state.tab = tab,
        ChequesMsg::Token(token) => state.token = Some(token),
        ChequesMsg::RecipientName(i, v) => {
            if let Some(r) = state.recipients.get_mut(i) {
                let at = contact_picker::opens(&r.name, &v);
                r.name = v;
                if at {
                    return Effect::Contacts(Target::ChequeName(i));
                }
            }
        }
        ChequesMsg::RecipientWallet(i, v) => {
            if let Some(r) = state.recipients.get_mut(i) {
                let at = contact_picker::opens(r.wallet.as_deref().unwrap_or(""), &v);
                r.wallet = Some(v);
                if at {
                    return Effect::Contacts(Target::ChequeWallet(i));
                }
            }
        }
        // The web's picks: from the name field both, from the wallet field the wallet (and the
        // name if there's none yet).
        ChequesMsg::Contact(target, name, address) => match target {
            Target::ChequeName(i) => {
                if let Some(r) = state.recipients.get_mut(i) {
                    r.name = name;
                    r.wallet = Some(address);
                }
            }
            Target::ChequeWallet(i) => {
                if let Some(r) = state.recipients.get_mut(i) {
                    r.wallet = Some(address);
                    if r.name.trim().is_empty() {
                        r.name = name;
                    }
                }
            }
            _ => {}
        },
        ChequesMsg::SetWallet(i) => if let Some(r) = state.recipients.get_mut(i) { r.wallet.get_or_insert_with(String::new); },
        ChequesMsg::Remove(i) => {
            if state.recipients.len() > 1 {
                state.recipients.remove(i);
            } else {
                state.recipients[0] = Recipient::default();
            }
        }
        ChequesMsg::AddRecipient if state.recipients.len() < MAX_RECIPIENTS => state.recipients.push(Recipient { name: String::new(), wallet: Some(String::new()) }),
        ChequesMsg::AddOpenSlot if state.recipients.len() < MAX_RECIPIENTS => state.recipients.push(Recipient { name: String::new(), wallet: None }),
        ChequesMsg::AddRecipient | ChequesMsg::AddOpenSlot => {}
        ChequesMsg::Import => return Effect::NotYet("Importing recipients"),
        ChequesMsg::Amount(v) => state.amount = v,
        ChequesMsg::Fee(bps) => state.fee_bps = bps,
        ChequesMsg::Note(v) => state.note = v,
        ChequesMsg::Expiry(Duration::Custom) => return Effect::PickTime(crate::components::time_picker::Purpose::ChequeExpiry, match state.expiry { Duration::At(t) => Some(t), _ => None }),
        ChequesMsg::Expiry(d) => state.expiry = d,
        ChequesMsg::Swap(on) => state.swap = on && SWAPS_VISIBLE,
        ChequesMsg::GetToken(token) => state.get_token = Some(token),
        ChequesMsg::GetAmount(v) => state.get_amount = v,
        ChequesMsg::Submit => {
            return match draft(state, chrono::Utc::now().timestamp().max(0) as u64) {
                Ok(d) => Effect::WriteCheque(Box::new(d)),
                Err(e) => Effect::Error(e),
            }
        }
    }
    Effect::None
}

/// The rows that count: one with neither a name nor a wallet field is skipped (isBlankRow).
fn filled(state: &State) -> impl Iterator<Item = (usize, &Recipient)> {
    state.recipients.iter().enumerate().filter(|(_, r)| !(r.name.trim().is_empty() && r.wallet.as_deref().is_some_and(|w| w.trim().is_empty())))
}

/// The web's checks, in its order and with its words.
pub fn draft(state: &State, now: u64) -> Result<Draft, String> {
    let rows: Vec<(usize, &Recipient)> = filled(state).collect();
    if rows.is_empty() || state.amount.trim().is_empty() {
        return Err("Please add at least one recipient and an amount!".into());
    }
    if let Some((i, _)) = rows.iter().find(|(_, r)| r.wallet.as_deref().is_some_and(|w| w.trim().is_empty())) {
        return Err(format!("Recipient #{} needs a wallet address (use \"Open slot\" for anyone)", i + 1));
    }
    let mut identifiers: Vec<String> = Vec::new();
    for (_, r) in &rows {
        let n = r.name.trim();
        if !n.is_empty() && !identifiers.iter().any(|x| x == n) {
            identifiers.push(n.to_string());
        }
    }
    if identifiers.is_empty() {
        return Err("Add a name / user ID for at least one recipient so the cheque shows up for them".into());
    }
    let Some(token) = state.token.clone() else { return Err("Choose a token first".into()) };
    let total = crate::evm::client::parse_units(&state.amount, token.decimals).ok().filter(|t| !t.is_zero()).ok_or("Enter a valid amount")?;
    let expires_at = match state.expiry {
        Duration::Never => 0,
        d => match d.ends_at(now as i64) {
            Some(t) if t > now as i64 + 60 => t as u64,
            _ => return Err("Pick an expiry date in the future".into()),
        },
    };
    if rows.len() > MAX_RECIPIENTS {
        return Err(format!("Too many recipients! Max allowed is {MAX_RECIPIENTS}."));
    }
    let swap = if state.swap {
        if rows.len() > 1 {
            return Err("A swap has one counterparty — remove the other rows".into());
        }
        let Some(get) = state.get_token.clone() else { return Err("Choose the token you want in return".into()) };
        if get.address.eq_ignore_ascii_case(&token.address) {
            return Err("Pick two different tokens — a swap trades one for another".into());
        }
        let get_total = crate::evm::client::parse_units(&state.get_amount, get.decimals).ok().filter(|t| !t.is_zero()).ok_or("Enter how much you want in return")?;
        Some(SwapAsk { token: get, amount: state.get_amount.trim().to_string(), total: get_total })
    } else {
        None
    };
    let mut recipients = Vec::new();
    for (i, r) in &rows {
        let wallet = match r.wallet.as_deref() {
            None => Address::ZERO,
            Some(w) => crate::evm::client::address(w).map_err(|_| format!("Recipient #{} has an invalid wallet address. Please verify it.", i + 1))?,
        };
        recipients.push((r.name.trim().to_string(), wallet));
    }
    Ok(Draft { token, recipients, identifiers, amount: state.amount.trim().to_string(), total, fee_bps: state.fee_bps.min(u32::from(u16::MAX)) as u16, note: state.note.clone(), expires_at, swap })
}

/// After a cheque went out: the next one starts from a clean form (token and fee stay).
pub fn sent(state: &mut State) {
    state.recipients = vec![Recipient::default()];
    state.amount.clear();
    state.get_amount.clear();
    state.note.clear();
}

fn msg(m: ChequesMsg) -> Message {
    Message::Cheques(m)
}

/// `tokens`: the picked token's field; `get_tokens`: the swap's "you get" one.
pub fn view<'a>(state: &'a State, tokens: token_select::Field, get_tokens: token_select::Field, lists: &'a Lists, me: &'a str, cards: records::Cards<'a>) -> El<'a> {
    let tabs = page_tabs::view(vec![
        Tab { icon: icons::pencil(), label: "Write & Claim", count: None, active: state.tab == ChequesTab::Write, on_press: msg(ChequesMsg::Tab(ChequesTab::Write)) },
        Tab { icon: icons::arrow_down_left(), label: "Incoming Cheques", count: None, active: state.tab == ChequesTab::Incoming, on_press: msg(ChequesMsg::Tab(ChequesTab::Incoming)) },
        Tab { icon: icons::arrow_up_right(), label: "Outgoing Cheques", count: None, active: state.tab == ChequesTab::Outgoing, on_press: msg(ChequesMsg::Tab(ChequesTab::Outgoing)) },
    ]);
    let body = match state.tab {
        ChequesTab::Write => write_form(state, tokens, get_tokens),
        ChequesTab::Incoming => records::list_view(lists.get(ListKind::IncomingCheques), ListKind::IncomingCheques, "Incoming Cheques", "Cheques written to you.", "No incoming cheques yet", format!("When someone writes a cheque to @{me}, it appears here."), true, chrono::Utc::now().timestamp(), &cards),
        ChequesTab::Outgoing => records::list_view(lists.get(ListKind::MyCheques), ListKind::MyCheques, "Outgoing Cheques", "Cheques you have written and funded.", "No outgoing cheques yet", "Cheques you write show up here with their claim status.".into(), false, chrono::Utc::now().timestamp(), &cards),
    };
    scrollable(container(column![tabs, body].spacing(24)).padding([20, 24]).width(Fill)).height(Fill).style(thin_scroll).into()
}

fn write_form<'a>(state: &'a State, tokens: token_select::Field, get_tokens: token_select::Field) -> El<'a> {
    use crate::components::token_picker::Purpose;
    let swap = state.swap;
    let mut rows = Column::new().spacing(8);
    // A swap has one counterparty: only the first row.
    for (i, r) in state.recipients.iter().enumerate().take(if swap { 1 } else { MAX_RECIPIENTS }) {
        let number = container(text((i + 1).to_string()).size(11).font(BOLD).color(TEXT_SECONDARY)).width(22).height(22).center(22).style(boxed(rgba(0xffffff, 0.05), BORDER, 5.0));
        let wallet: El = match &r.wallet {
            None => container(
                row![
                    icon(icons::globe(), 12, GREEN),
                    text(if swap { "Anyone with the link can take it" } else { "Open slot: any wallet can claim" }).size(12.5).color(GREEN),
                    space().width(Fill),
                    button(text("Set wallet").size(11).color(TEXT_SECONDARY)).padding([2, 6]).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(ChequesMsg::SetWallet(i))),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([7, 10])
            .style(boxed(rgba(0x10b981, 0.04), BORDER, 7.0))
            .into(),
            Some(w) => input("0x… recipient wallet (@ contacts)", w, move |v| msg(ChequesMsg::RecipientWallet(i, v))).into(),
        };
        rows = rows.push(
            container(
                row![
                    number,
                    input("Name / email (@ contacts)", &r.name, move |v| msg(ChequesMsg::RecipientName(i, v))),
                    wallet,
                    button(icon(icons::trash(), 13, DANGER_TEXT)).padding(5).style(ghost(5.0, rgba(0xef4444, 0.12))).on_press(msg(ChequesMsg::Remove(i))),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(6)
            .style(inset),
        );
    }
    let has_open_slot = state.recipients.iter().any(|r| r.wallet.is_none());

    let mode_btn = |caption: &'a str, on: bool| button(container(text(caption).size(12.5).font(SEMIBOLD)).center_x(Fill)).width(Fill).padding([7, 0]).style(if on { tone_button(Tone::Accent) } else { tone_button(Tone::Dark) }).on_press(msg(ChequesMsg::Swap(caption == "Swap")));
    let mode = container(row![mode_btn("Cheque", !swap), mode_btn("Swap", swap)].spacing(6)).padding(4).style(boxed(rgba(0x000000, 0.2), BORDER, 9.0));

    let mut recipients_header = row![icon(icons::users(), 13, LABEL), label(if swap { "Who can take it" } else { "Recipients" }, true)].spacing(6).align_y(Alignment::Center);
    if !swap {
        recipients_header = recipients_header.push(text(format!("{}/{MAX_RECIPIENTS}", state.recipients.len())).size(11.5).color(TEXT_SECONDARY)).push(space().width(Fill)).push(
            button(row![icon(icons::file_up(), 11, INDIGO_SOFT), text("Import JSON/CSV").size(11)].spacing(4).align_y(Alignment::Center))
                .padding([3, 8])
                .style(outline_indigo)
                .on_press(msg(ChequesMsg::Import)),
        );
    }

    let mut form = column![
        if swap {
            title("Write a swap", "Lock a token for someone. They take it by paying what you ask, in the same transaction.")
        } else {
            title("Write a cheque", "Lock crypto currency for someone. They claim it when the work is done.")
        },
    ]
    .push(SWAPS_VISIBLE.then_some(mode))
    .spacing(16);
    if swap {
        form = form
            .push(row![
                column![label("You give", true), token_select::view(tokens, state.token.as_ref(), Purpose::Cheque), input("0.05", &state.amount, |v| msg(ChequesMsg::Amount(v)))].spacing(6).width(Fill),
                column![label("You get", true), token_select::view(get_tokens, state.get_token.as_ref(), Purpose::SwapGet), input("100", &state.get_amount, |v| msg(ChequesMsg::GetAmount(v)))].spacing(6).width(Fill),
            ]
            .spacing(12))
            .push(column![recipients_header, rows].spacing(8));
    } else {
        form = form
            .push(column![label("Token", false), token_select::view(tokens, state.token.as_ref(), Purpose::Cheque)].spacing(6))
            .push(column![recipients_header, rows].spacing(8))
            .push(
                row![
                    button(row![icon(icons::plus(), 12, hex(0xc7d2fe)), text("Add Recipient").size(11.5).font(BOLD)].spacing(4).align_y(Alignment::Center)).padding([4, 10]).style(outline_indigo).on_press(msg(ChequesMsg::AddRecipient)),
                    button(row![icon(icons::globe(), 12, TEXT_SECONDARY), text("Open slot").size(11.5)].spacing(4).align_y(Alignment::Center)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press(msg(ChequesMsg::AddOpenSlot)),
                ]
                .spacing(8),
            );
    }

    if has_open_slot && swap {
        form = form.push(
            container(
                row![
                    icon(icons::triangle_alert(), 14, AMBER_TEXT),
                    text("Anyone with the link can take this swap — and taking it shows its secret on-chain before it confirms, so a bot could step in. It still pays you exactly what you ask; set their wallet if it should be them.").size(12).color(AMBER_TEXT),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([9, 12])
            .style(notice),
        );
    } else if has_open_slot {
        form = form.push(
            container(
                row![
                    icon(icons::triangle_alert(), 14, AMBER_TEXT),
                    text("Bots can intercept an open slot: the claim reveals the secret on-chain before it confirms. Use the recipient's wallet address whenever you can.").size(12).color(AMBER_TEXT),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([9, 12])
            .style(notice),
        );
    }

    if !swap {
        form = form.push(column![label("Total amount", true), input("0.05", &state.amount, |v| msg(ChequesMsg::Amount(v)))].spacing(6));
    }
    form = form
        .push(fee_picker::view(if swap { "Platform fee (taken from what you get)" } else { "Platform fee (deducted on claim)" }, state.fee_bps, None, false, |b| msg(ChequesMsg::Fee(b))))
        .push(column![label("Note (optional)", false), note_input("Memo / milestone description", &state.note, |v| msg(ChequesMsg::Note(v)))].spacing(6))
        .push(duration_picker::view(if swap { "Swap expiry" } else { "Cheque expiry" }, &EXPIRY_PRESETS, state.expiry, Some("No expiry"), |d| msg(ChequesMsg::Expiry(d))))
        .push(
            button(iced::widget::center(text(if swap { "Write swap" } else { "Write cheque" }).size(14).font(SEMIBOLD).color(Color::WHITE)).height(20))
                .width(Fill)
                .padding([10, 0])
                .style(tone_button(Tone::Accent))
                .on_press(msg(ChequesMsg::Submit)),
        );

    form_card(form)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_opens_contacts_and_a_pick_fills_the_row() {
        let mut st = State::default();
        assert!(matches!(update(&mut st, ChequesMsg::RecipientName(0, "@".into())), Effect::Contacts(Target::ChequeName(0))));
        assert!(matches!(update(&mut st, ChequesMsg::RecipientName(0, "bob@".into())), Effect::None)); // an email
        let a = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
        update(&mut st, ChequesMsg::Contact(Target::ChequeName(0), "Alice".into(), a.into()));
        assert_eq!((st.recipients[0].name.as_str(), st.recipients[0].wallet.as_deref()), ("Alice", Some(a)));
        // From the wallet field: the wallet, and the name stays.
        update(&mut st, ChequesMsg::Contact(Target::ChequeWallet(0), "Other".into(), "0xb0b".into()));
        assert_eq!((st.recipients[0].name.as_str(), st.recipients[0].wallet.as_deref()), ("Alice", Some("0xb0b")));
    }

    fn usdc() -> Token {
        Token { symbol: "USDC".into(), name: "USD Coin".into(), address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into(), decimals: 6, logo: None }
    }

    #[test]
    fn checks_like_the_web() {
        let bob = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
        let mut st = State::default();
        assert!(draft(&st, 0).unwrap_err().contains("at least one recipient"));
        st.amount = "1.5".into();
        st.recipients[0].name = "bob".into();
        assert!(draft(&st, 0).unwrap_err().starts_with("Recipient #1 needs a wallet")); // no open slot by default
        st.recipients[0].wallet = Some(bob.into());
        st.recipients[0].name.clear();
        assert!(draft(&st, 0).unwrap_err().contains("name / user ID"));
        st.recipients[0].name = "bob".into();
        assert_eq!(draft(&st, 0).unwrap_err(), "Choose a token first");
        st.token = Some(usdc());
        let d = draft(&st, 1_000).unwrap();
        assert_eq!(d.total, U256::from(1_500_000u64));
        assert_eq!(d.recipients, vec![("bob".to_string(), crate::evm::client::address(bob).unwrap())]);
        assert_eq!(d.expires_at, 1_000 + 30 * 86_400);

        st.recipients.push(Recipient { name: "alice".into(), wallet: Some("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02914".into()) });
        assert!(draft(&st, 0).unwrap_err().starts_with("Recipient #2 has an invalid"));
        st.recipients[1].wallet = Some(String::new());
        assert!(draft(&st, 0).unwrap_err().starts_with("Recipient #2 needs a wallet"));
        // A row with nothing in it is skipped.
        st.recipients[1].name.clear();
        assert_eq!(draft(&st, 0).unwrap().recipients.len(), 1);

        // An open slot is there only when added on purpose — and then it's written as one.
        st.recipients.truncate(1);
        update(&mut st, ChequesMsg::AddOpenSlot);
        st.recipients[1].name = "carol".into();
        assert_eq!(draft(&st, 0).unwrap().recipients[1], ("carol".to_string(), Address::ZERO));
        st.recipients.pop();

        st.amount = "1.0000001".into(); // 7 decimals for a 6-decimal token
        assert_eq!(draft(&st, 0).unwrap_err(), "Enter a valid amount");
    }

    #[test]
    fn a_swap_needs_its_price_and_one_counterparty() {
        let weth = Token { symbol: "WETH".into(), name: "Wrapped Ether".into(), address: "0x4200000000000000000000000000000000000006".into(), decimals: 18, logo: None };
        let bob = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
        let mut st = State { swap: true, token: Some(usdc()), amount: "100".into(), ..State::default() };
        st.recipients[0].name = "bob".into();
        st.recipients[0].wallet = Some(bob.into());
        assert_eq!(draft(&st, 0).unwrap_err(), "Choose the token you want in return");
        st.get_token = Some(usdc());
        st.get_amount = "1".into();
        assert!(draft(&st, 0).unwrap_err().starts_with("Pick two different tokens"));
        st.get_token = Some(weth);
        st.get_amount = "0".into();
        assert_eq!(draft(&st, 0).unwrap_err(), "Enter how much you want in return");
        st.get_amount = "0.03".into();
        let d = draft(&st, 1_000).unwrap();
        let ask = d.swap.unwrap();
        assert_eq!((d.total, ask.total), (U256::from(100_000_000u64), U256::from(30_000_000_000_000_000u64)));
        assert_eq!(d.recipients, vec![("bob".to_string(), crate::evm::client::address(bob).unwrap())]);
        // One counterparty: a second filled row is refused.
        st.recipients.push(Recipient { name: "alice".into(), wallet: Some("0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC".into()) });
        assert!(draft(&st, 0).unwrap_err().starts_with("A swap has one counterparty"));
        // Back to a cheque: the swap fields don't matter.
        st.swap = false;
        assert!(draft(&st, 0).unwrap().swap.is_none());
    }
}
