// SPDX-License-Identifier: GPL-3.0-or-later
//! The bar above a conversation: room, status, actions (ChatHeader.tsx).

use iced::alignment::Vertical;
use iced::widget::{button, center, column, container, row, space, svg, text};
use iced::{Alignment, Color, Fill};

use super::display::{self, RoomKind};
use super::room_avatar;
use crate::api::chat::ChatRoomItem;
use crate::app::Message;
use crate::components::chat::manage_room::Tab;
use crate::pages::chat_page::ChatMsg;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

pub fn view(room: &ChatRoomItem, me: i64, requests: i64, members_open: bool, search_open: bool) -> El<'_> {
    let subtitle: El = match display::kind(room) {
        RoomKind::Dm => match display::partner(room, me) {
            Some(p) => {
                let (line, online) = display::presence(p);
                text(line).size(12).color(if online { GREEN } else { TEXT_SECONDARY }).into()
            }
            None => text("direct message").size(12).color(TEXT_SECONDARY).into(),
        },
        _ => {
            let members = room.member_count.unwrap_or(room.allowed_user_ids.len() as i64);
            let mut line = format!("{members} member{}", if members == 1 { "" } else { "s" });
            if let Some(topic) = room.topic.as_deref().filter(|t| !t.is_empty()) {
                line.push_str(" · ");
                line.push_str(topic);
            }
            text(line).size(12).color(TEXT_SECONDARY).into()
        }
    };
    let mut name = row![text(display::title(room, me)).size(15).font(BOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center);
    if display::is_encrypted_room(room) {
        name = name.push(icon(icons::lock(), 11, GREEN));
    }
    // Search this chat (lit while its panel is open).
    let search = button(center(icon(icons::search(), 17, if search_open { Color::WHITE } else { MUTED })))
        .width(34)
        .height(34)
        .padding(0)
        .style(move |t, s| if search_open { selectable(true, rgba(0x6366f1, 0.25), 8.0)(t, s) } else { ghost(8.0, rgba(0xffffff, 0.06))(t, s) })
        .on_press(Message::Chat(ChatMsg::Search(crate::pages::chat_search::SearchMsg::Toggle)));
    // The member list (not for DMs: two people).
    let members_button = |room: &ChatRoomItem, open: bool| -> El<'_> {
        if super::display::is_dm(room) {
            return space().width(0).into();
        }
        button(center(icon(icons::users(), 17, if open { Color::WHITE } else { MUTED })))
            .width(34)
            .height(34)
            .padding(0)
            .style(move |t, s| if open { selectable(true, rgba(0x6366f1, 0.25), 8.0)(t, s) } else { ghost(8.0, rgba(0xffffff, 0.06))(t, s) })
            .on_press(Message::Chat(ChatMsg::ToggleMembers))
            .into()
    };
    // Manage Room; a dot for people asking to join.
    let manage = |handle: svg::Handle, tab: Tab, shown: bool, badge: i64| -> El<'_> {
        if !shown {
            return space().width(0).into();
        }
        let b = button(center(icon(handle, 17, MUTED))).width(34).height(34).padding(0).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::OpenManage(tab)));
        if badge > 0 {
            iced::widget::stack![b, container(container(space().width(8).height(8)).style(boxed(AMBER, Color::TRANSPARENT, 4.0))).width(34).align_x(iced::alignment::Horizontal::Right).padding([4, 4])].into()
        } else {
            b.into()
        }
    };
    // Who it is: in a DM a click opens the other person's profile (no need to find one of their
    // messages for it).
    let who = row![room_avatar::view(room, me, 32.0), column![name, subtitle].spacing(1)].spacing(10).align_y(Alignment::Center);
    let who: El = match display::partner(room, me) {
        Some(p) => iced::widget::mouse_area(who).on_press(Message::Chat(ChatMsg::OpenProfile(Box::new(p.clone())))).interaction(iced::mouse::Interaction::Pointer).into(),
        None => who.into(),
    };
    container(
        row![
            who,
            space().width(Fill),
            button(center(icon(icons::pin(), 17, MUTED))).width(34).height(34).padding(0).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::OpenPinned)),
            search,
            members_button(room, members_open),
            manage(icons::settings(), Tab::General, room.is_member != Some(false), requests),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .height(56)
    .padding([0, 18])
    .align_y(Vertical::Center)
    .style(panel(MAIN))
    .into()
}
