// SPDX-License-Identifier: GPL-3.0-or-later
//! A room's picture: # for public rooms, people for groups, the person (with presence) for DMs.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, space, stack, svg};
use iced::{Background, Color};

use super::display::{self, RoomKind};
use crate::api::chat::ChatRoomItem;
use crate::components::ui::{avatar_pic, icon, El};
use crate::icons;
use crate::theme::*;

pub fn view<'a>(room: &ChatRoomItem, me: i64, size: f32) -> El<'a> {
    let glyph = |handle: svg::Handle, bg: Background, radius: f32| -> El<'a> {
        container(icon(handle, (size * 0.5) as u32, Color::WHITE))
            .width(size)
            .height(size)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(round_fill(bg, radius))
            .into()
    };
    // A room with its own picture (set in Manage Room): that, for any kind but a DM.
    if display::kind(room) != RoomKind::Dm {
        if let Some(pic) = room.avatar.as_deref().filter(|s| !s.trim().is_empty()) {
            if crate::avatars::get(Some(pic)).is_some() {
                return avatar_pic(&room.title, Some(pic), size, None);
            }
        }
    }
    match display::kind(room) {
        RoomKind::Public => glyph(icons::hash(), gradient(hex(0x6366f1), hex(0x38bdf8)), 9.0),
        RoomKind::Suggested if room.chat_type == "public" => glyph(icons::hash(), gradient(hex(0x6366f1), hex(0x38bdf8)), 9.0),
        RoomKind::Group | RoomKind::Suggested => glyph(icons::users(), gradient(hex(0x818cf8), hex(0xc084fc)), size / 2.0),
        RoomKind::Dm => {
            let partner = display::partner(room, me);
            let name = partner.map(|p| p.username.clone()).unwrap_or_else(|| room.title.clone());
            let online = partner.and_then(|p| p.is_online) == Some(true);
            let dot_color = if online { hex(0x23a55a) } else { hex(0x80848e) };
            let dot = container(space().width(9).height(9)).style(move |_| container::Style {
                background: Some(dot_color.into()),
                border: iced::Border { color: LIST, width: 2.0, radius: 6.0.into() },
                ..Default::default()
            });
            stack![
                avatar_pic(&name, partner.and_then(display::avatar_of), size, None),
                container(dot).width(size).height(size).align_x(Horizontal::Right).align_y(Vertical::Bottom),
            ]
            .into()
        }
    }
}
