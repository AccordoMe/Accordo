// SPDX-License-Identifier: GPL-3.0-or-later
//! Ctrl+K: jump to a chat by typing part of its name (like Discord / Slack / editors' "Go to
//! file"). Empty: the most recent chats. ↑/↓ pick, Enter opens, Esc closes.

use iced::widget::{column, container, mouse_area, row, space, text, text_input, Column};
use iced::{Alignment, Color, Fill};

use super::overlay;
use super::ui::{count_badge, icon, El};
use crate::api::chat::ChatRoomItem;
use crate::app::Message;
use crate::components::chat::{display, room_avatar};
use crate::icons;
use crate::pages::chat_page;
use crate::theme::*;

pub const INPUT: &str = "quick-switcher";
const MAX: usize = 9;

#[derive(Default)]
pub struct State {
    pub query: String,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub enum SwitcherMsg {
    Query(String),
    Move(i32),
    Open(Option<String>),
    Close,
}

/// Matching chats, best first: name starts with the query, then contains it; recent first.
pub fn matches<'a>(chat: &'a chat_page::State, query: &str) -> Vec<&'a ChatRoomItem> {
    let q = query.trim().to_lowercase();
    let mut list: Vec<(u8, &ChatRoomItem)> = chat
        .rooms
        .rooms
        .iter()
        .filter_map(|r| {
            let title = display::title(r, chat.me_id).to_lowercase();
            if q.is_empty() || title.starts_with(&q) {
                Some((0, r))
            } else if title.contains(&q) || r.room_key.to_lowercase().contains(&q) {
                Some((1, r))
            } else {
                None
            }
        })
        .collect();
    list.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.last_timestamp.cmp(&a.1.last_timestamp)));
    list.into_iter().take(MAX).map(|(_, r)| r).collect()
}

pub fn view<'a>(st: &'a State, chat: &'a chat_page::State) -> El<'a> {
    let found = matches(chat, &st.query);
    let index = st.index.min(found.len().saturating_sub(1));
    let mut list: Column<'_, Message> = column![].spacing(2);
    for (i, r) in found.iter().enumerate() {
        let on = i == index;
        let unread = chat.unread(&r.room_key).unread_count;
        let kind = match display::kind(r) {
            display::RoomKind::Dm => "Direct message",
            display::RoomKind::Group => "Group",
            display::RoomKind::Public => "Public room",
            display::RoomKind::Suggested => "Suggested",
        };
        let mut line = row![
            room_avatar::view(r, chat.me_id, 28.0),
            column![text(display::title(r, chat.me_id)).size(13.5).font(SEMIBOLD).color(Color::WHITE), text(kind).size(11).color(FAINT)].spacing(1),
            space().width(Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if unread > 0 {
            line = line.push(count_badge(unread as u32, rgba(0xffffff, 0.2)));
        }
        if on {
            line = line.push(text("↵").size(13).color(MUTED));
        }
        let key = r.room_key.clone();
        list = list.push(
            mouse_area(container(line).width(Fill).padding([7, 10]).style(move |_| container::Style {
                background: on.then(|| rgba(0x6366f1, 0.2).into()),
                border: border(Color::TRANSPARENT, 0.0, 8.0),
                ..Default::default()
            }))
            .on_press(Message::Switcher(SwitcherMsg::Open(Some(key))))
            .interaction(iced::mouse::Interaction::Pointer),
        );
    }
    if found.is_empty() {
        list = list.push(container(text("No chats match").size(13).color(MUTED)).padding([18, 10]));
    }
    let open_first = found.get(index).map(|r| r.room_key.clone());
    let body = column![
        row![
            icon(icons::search(), 16, MUTED),
            text_input("Where would you like to go?", &st.query)
                .id(INPUT)
                .size(15)
                .padding([6, 0])
                .style(bare_input)
                .on_input(|v| Message::Switcher(SwitcherMsg::Query(v)))
                .on_submit(Message::Switcher(SwitcherMsg::Open(open_first))),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        container(space().width(Fill).height(1)).style(crate::theme::rule),
        text(if st.query.trim().is_empty() { "RECENT CHATS" } else { "CHATS" }).size(10.5).font(BOLD).color(FAINT),
        list,
        row![text("↑↓ to pick · Enter to open · Esc to close").size(11).color(FAINT)],
    ]
    .spacing(10);
    overlay::modal(container(body).width(560).padding(16).style(modal), Message::Switcher(SwitcherMsg::Close))
}
