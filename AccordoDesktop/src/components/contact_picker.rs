// SPDX-License-Identifier: GPL-3.0-or-later
//! Contacts (ContactSearchModal.tsx): the address book as a picker. Typing @ at the start of an
//! address field, or after a space, opens it for that field; Ctrl+Shift+T opens it only to manage
//! the list. Search, pick with a click or Enter, add someone new right here (used straight away
//! when a field asked), copy an address, delete.
//!
//! Contacts are kept per account in state.json (`AppStore::contacts`), on this computer only.
//! Closing the picker leaves the typed @ in the field, so "@username" can still be typed through.

use iced::widget::{button, column, container, mouse_area, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Length};

use super::overlay;
use super::ui::{icon, El};
use crate::app::Message;
use crate::icons;
use crate::store::app_store::Contact;
use crate::theme::*;

pub const SEARCH: &str = "contact-search";
pub const NAME: &str = "contact-name";
pub const ADDRESS: &str = "contact-address";

/// Which field asked: where the pick goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Ctrl+Shift+T: only the list, nothing to fill in.
    Manage,
    /// A cheque recipient's name field: the name and the wallet.
    ChequeName(usize),
    /// A cheque recipient's wallet field: the wallet (and the name, if it's empty).
    ChequeWallet(usize),
    InvoiceMerchant,
    /// The payer field: the name (and the wallet, if it's empty).
    InvoiceClient,
    InvoiceWallet,
    /// A split payer's name field: the name and the wallet.
    InvoicePayerName(usize),
    /// A split payer's wallet field: the wallet (and the name, if it's empty).
    InvoicePayerWallet(usize),
    #[cfg(feature = "drops")]
    DropWallet(usize),
}

#[derive(Debug, Clone)]
pub enum ContactsMsg {
    Open(Target),
    Close,
    Query(String),
    Move(i32),
    /// Enter in the search: the highlighted contact.
    Use,
    /// A click on a contact (its id).
    Pick(String),
    Remove(String),
    Copy(String),
    StartAdd,
    Name(String),
    Address(String),
    Save,
    CancelAdd,
}

#[derive(Debug, Default)]
pub struct AddForm {
    pub name: String,
    pub address: String,
}

#[derive(Debug)]
pub struct State {
    pub target: Target,
    pub query: String,
    pub index: usize,
    pub add: Option<AddForm>,
}

impl State {
    pub fn new(target: Target) -> Self {
        Self { target, query: String::new(), index: 0, add: None }
    }

    /// A field asked for a contact (not just managing the list).
    pub fn picks(&self) -> bool {
        self.target != Target::Manage
    }

    /// The add form, filled from the search: a pasted address is most likely who they want to
    /// add, anything else their name.
    pub fn start_add(&mut self) {
        let q = self.query.trim();
        let mut f = AddForm::default();
        if is_address(q) {
            f.address = q.to_string();
        } else {
            f.name = q.to_string();
        }
        self.add = Some(f);
    }
}

/// 0x + 40 hex characters (the web's check; contacts are EVM wallets).
pub fn is_address(v: &str) -> bool {
    let v = v.trim();
    v.len() == 42 && v.starts_with("0x") && v[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The web's trigger: one @ typed at the start of the field or after a space.
pub fn opens(before: &str, after: &str) -> bool {
    let Some(head) = after.strip_suffix('@') else { return false };
    head == before && head.chars().last().is_none_or(char::is_whitespace)
}

/// The add form's problem, if any (the web's words).
pub fn problem(f: &AddForm) -> Option<&'static str> {
    if f.name.trim().is_empty() {
        Some("Give the contact a name")
    } else if !is_address(&f.address) {
        Some("That isn't a wallet address (0x + 40 hex characters)")
    } else {
        None
    }
}

/// Contacts matching the search, in the order they were added.
pub fn matches<'a>(contacts: &'a [Contact], query: &str) -> Vec<&'a Contact> {
    let q = query.trim().to_lowercase();
    contacts.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q) || c.address.to_lowercase().contains(&q)).collect()
}

fn msg(m: ContactsMsg) -> Message {
    Message::Contacts(m)
}

fn avatar<'a>(name: &str) -> El<'a> {
    let initial: String = name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    container(text(initial).size(13).font(BOLD).color(hex(0xc7d2fe))).width(32).height(32).center(32).style(boxed(rgba(0x6366f1, 0.2), Color::TRANSPARENT, 16.0)).into()
}

fn add_panel(f: &AddForm, picks: bool) -> El<'_> {
    let bad_address = !f.address.trim().is_empty() && !is_address(&f.address);
    let address = text_input("0x… wallet address", &f.address)
        .id(ADDRESS)
        .on_input(|v| msg(ContactsMsg::Address(v)))
        .on_submit(msg(ContactsMsg::Save))
        .size(13.5)
        .padding([10, 12])
        .font(Font::MONOSPACE)
        .style(move |t, s| {
            let mut st = field(t, s);
            if bad_address {
                st.border.color = DANGER_TEXT;
            }
            st
        });
    let note: El = match problem(f) {
        Some(p) if bad_address || (!f.name.is_empty() && f.address.is_empty()) => text(p).size(11.5).color(if bad_address { DANGER_TEXT } else { FAINT }).into(),
        _ => space().height(0).into(),
    };
    column![
        text_input("Name", &f.name).id(NAME).on_input(|v| msg(ContactsMsg::Name(v))).on_submit(msg(ContactsMsg::Save)).size(13.5).padding([10, 12]).style(field),
        address,
        note,
        row![
            space().width(Fill),
            button(text("Cancel").size(13).color(MUTED)).padding([8, 12]).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(ContactsMsg::CancelAdd)),
            button(text(if picks { "Save & use" } else { "Save" }).size(13).font(SEMIBOLD)).padding([8, 16]).style(tone_button(Tone::Accent)).on_press_maybe(problem(f).is_none().then(|| msg(ContactsMsg::Save))),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

pub fn view<'a>(st: &'a State, contacts: &'a [Contact]) -> El<'a> {
    let found = matches(contacts, &st.query);
    let picks = st.picks();
    let index = st.index.min(found.len().saturating_sub(1));
    let mut list: Column<'a, Message> = column![].spacing(2);
    for (i, c) in found.iter().enumerate() {
        let on = picks && st.add.is_none() && i == index;
        let line = row![
            avatar(&c.name),
            column![
                text(c.name.as_str()).size(14).font(SEMIBOLD).color(Color::WHITE),
                text(c.address.as_str()).size(12).font(Font::MONOSPACE).color(TEXT_SECONDARY),
            ]
            .spacing(1)
            .width(Fill),
            button(icon(icons::copy(), 13, FAINT)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(ContactsMsg::Copy(c.address.clone()))),
            button(icon(icons::trash(), 13, FAINT)).padding(5).style(ghost(6.0, rgba(0xef4444, 0.12))).on_press(msg(ContactsMsg::Remove(c.id.clone()))),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let item = container(line).width(Fill).padding([8, 10]).style(move |_| container::Style {
            background: on.then(|| rgba(0x6366f1, 0.16).into()),
            border: border(Color::TRANSPARENT, 0.0, 10.0),
            ..Default::default()
        });
        let item: El = if picks {
            mouse_area(item).on_press(msg(ContactsMsg::Pick(c.id.clone()))).interaction(iced::mouse::Interaction::Pointer).into()
        } else {
            item.into()
        };
        list = list.push(item);
    }
    if found.is_empty() {
        let empty = if contacts.is_empty() { "No contacts yet — add the first one below." } else { "Nobody matches that." };
        list = list.push(container(text(empty).size(13).color(TEXT_SECONDARY)).center_x(Fill).padding([28, 12]));
    }

    let mut search = text_input("Search by name or address…", &st.query).id(SEARCH).on_input(|v| msg(ContactsMsg::Query(v))).size(14).padding([6, 0]).style(bare_input);
    if picks {
        search = search.on_submit(msg(ContactsMsg::Use));
    }
    let footer: El = match &st.add {
        Some(f) => add_panel(f, picks),
        None => button(container(row![icon(icons::plus(), 14, INDIGO_SOFT), text("Add contact").size(13).font(SEMIBOLD).color(INDIGO_SOFT)].spacing(6).align_y(Alignment::Center)).center_x(Fill))
            .width(Fill)
            .padding([9, 12])
            .style(outline_indigo)
            .on_press(msg(ContactsMsg::StartAdd))
            .into(),
    };
    let hint = if picks { "↑↓ to move · Enter or a click to use · Esc to close" } else { "Kept on this computer, for this account · Esc to close" };
    let body = column![
        row![
            icon(icons::users(), 17, INDIGO_SOFT),
            text("Contacts").size(16).font(BOLD).color(Color::WHITE),
            space().width(Fill),
            button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(ContactsMsg::Close)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        container(row![icon(icons::search(), 14, FAINT), search].spacing(8).align_y(Alignment::Center)).padding([2, 12]).style(boxed(rgba(0xffffff, 0.04), BORDER, 10.0)),
        scrollable(list).height(Length::Fixed(300.0)).style(thin_scroll),
        container(space().width(Fill).height(1)).style(rule),
        footer,
        text(hint).size(11).color(FAINT),
    ]
    .spacing(12);
    overlay::modal(container(body).width(460).padding(18).style(modal), msg(ContactsMsg::Close))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_opens_only_at_the_start_or_after_a_space() {
        assert!(opens("", "@"));
        assert!(opens("pay ", "pay @"));
        assert!(!opens("bob", "bob@")); // an email
        assert!(!opens("", "@@"));
        assert!(!opens("@", "")); // deleting
        assert!(!opens("", "a@")); // a paste
    }

    #[test]
    fn addresses_and_search() {
        let a = "0x00000000000000000000000000000000000000Ab";
        assert!(is_address(a) && is_address(&format!(" {a} ")));
        assert!(!is_address("0x123") && !is_address(&a.replace("Ab", "zz")));
        let c = |n: &str, a: &str| Contact { id: n.into(), name: n.into(), address: a.into() };
        let list = vec![c("Alice", a), c("Bob", "0xb0b")];
        assert_eq!(matches(&list, "ali").len(), 1);
        assert_eq!(matches(&list, "0XB0").len(), 1);
        assert_eq!(matches(&list, " ").len(), 2);
        let mut st = State::new(Target::Manage);
        st.query = a.into();
        st.start_add();
        assert_eq!(st.add.as_ref().map(|f| f.address.as_str()), Some(a));
        assert_eq!(problem(st.add.as_ref().unwrap()), Some("Give the contact a name"));
    }
}
