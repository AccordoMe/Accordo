// SPDX-License-Identifier: GPL-3.0-or-later
//! One thread, docked beside the chat (ThreadPanel.tsx): the root message, its replies and a box
//! that posts into the thread. Replies are encrypted with the room key like any message; the
//! server only knows which messages belong to which thread.

use iced::widget::{button, center, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Padding};

use super::message_list::{self, Feed, MenuKind, Rights};
use crate::api::chat::{ChatMessageItem, UserInfo};
use crate::app::Message;
use crate::components::ui::{hline, icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

pub const THREAD_INPUT: &str = "thread-input";

/// What cheque cards need: their states, explorers by network, the viewer's wallets.
pub type CardRefs<'a> = (&'a std::collections::HashMap<String, super::cheque_card::ChequeView>, &'a std::collections::HashMap<String, String>, &'a [(String, String)], &'a std::collections::HashMap<String, super::invoice_card::InvoiceView>, &'a str, &'a super::fund_cards::Cards);

pub struct Thread {
    pub root_id: i64,
    pub room_key: String,
    pub root: Option<ChatMessageItem>,
    pub replies: Vec<ChatMessageItem>,
    pub loading: bool,
    pub draft: String,
    pub sending: bool,
}

impl Thread {
    pub fn new(root_id: i64, room_key: String, root: Option<ChatMessageItem>) -> Self {
        Self { root_id, room_key, root, replies: Vec::new(), loading: true, draft: String::new(), sending: false }
    }

    pub fn find_mut(&mut self, id: i64) -> Option<&mut ChatMessageItem> {
        self.root.iter_mut().chain(self.replies.iter_mut()).find(|m| m.id == id)
    }

    pub fn find(&self, id: i64) -> Option<&ChatMessageItem> {
        self.root.iter().chain(self.replies.iter()).find(|m| m.id == id)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(t: &'a Thread, rights: Rights, menu: Option<(i64, MenuKind)>, members: &'a [UserInfo], encrypted: bool, locked: Option<&'a str>, media: &'a crate::media::Cache, clock: u64, emoji: &'a super::emoji_picker::EmojiUi, cards: CardRefs<'a>) -> El<'a> {
    let rights = Rights { threads: false, ..rights };
    let feed = Feed { messages: None, loading: false, rights, menu, members, flash: None, media, clock, emoji, cheques: cards.0, explorers: cards.1, wallets: cards.2, invoices: cards.3, me_name: cards.4, fund: cards.5, conceal: false, polls: None, private: encrypted, playing: None };
    let mut title = row![icon(icons::message_square_text(), 17, INDIGO_ICON), text("Thread").size(15).font(BOLD).color(Color::WHITE)].spacing(8).align_y(Alignment::Center);
    if encrypted {
        title = title.push(icon(icons::lock(), 12, GREEN));
    }
    let header = container(
        row![title, space().width(Fill), button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::CloseThread))].align_y(Alignment::Center),
    )
    .height(56)
    .padding([0, 16])
    .align_y(iced::alignment::Vertical::Center);

    let mut list = Column::new();
    if t.loading && t.root.is_none() {
        list = list.push(container(text("Loading thread…").size(13).color(MUTED)).padding(12));
    }
    let deleted = t.root.as_ref().is_some_and(|r| r.message_type == "deleted");
    if let Some(root) = &t.root {
        list = list.push(message_list::message(root, true, &feed)).push(
            container(text(format!("{} {}", t.replies.len(), if t.replies.len() == 1 { "reply" } else { "replies" })).size(12).color(MUTED)).padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 62.0 }),
        );
        list = list.push(container(hline()).padding([2, 12]));
    }
    if !t.loading && t.replies.is_empty() {
        list = list.push(container(text("No replies yet — start the discussion.").size(13).color(FAINT)).padding([10, 14]));
    }
    let mut prev: Option<&ChatMessageItem> = None;
    for m in &t.replies {
        let head = prev.is_none_or(|p| p.sender_id != m.sender_id);
        list = list.push(message_list::message(m, head, &feed));
        prev = Some(m);
    }

    let footer: El = if deleted {
        center(text("The original message was deleted — this thread is read-only.").size(12.5).color(MUTED)).height(44).into()
    } else if let Some(why) = locked {
        row![icon(icons::lock(), 14, AMBER_TEXT), text(why).size(12).color(TEXT_SECONDARY)].spacing(8).align_y(Alignment::Center).into()
    } else {
        let ready = !t.draft.trim().is_empty() && !t.sending;
        container(
            row![
                text_input("Reply in thread…", &t.draft)
                    .id(THREAD_INPUT)
                    .on_input(|v| Message::Chat(ChatMsg::ThreadDraft(v)))
                    .on_submit(Message::Chat(ChatMsg::ThreadSend))
                    .size(13.5)
                    .padding([9, 4])
                    .style(bare_input),
                button(center(icon(icons::send(), 15, if ready { Color::WHITE } else { FAINT })))
                    .width(32)
                    .height(32)
                    .padding(0)
                    .style(move |th, s| if ready { primary(th, s) } else { ghost(8.0, rgba(0xffffff, 0.06))(th, s) })
                    .on_press_maybe(ready.then_some(Message::Chat(ChatMsg::ThreadSend))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([3, 6])
        .style(boxed(hex(0x15161c), BORDER, 10.0))
        .into()
    };

    container(
        column![
            header,
            hline(),
            scrollable(container(list).padding([8, 4]).width(Fill)).anchor_bottom().height(Fill).style(thin_scroll),
            hline(),
            container(footer).padding(12),
        ]
        .height(Fill),
    )
    .width(400)
    .height(Fill)
    .style(|_| container::Style { background: Some(hex(0x0f1015).into()), border: iced::Border { color: BORDER, width: 1.0, radius: 0.0.into() }, ..Default::default() })
    .into()
}
