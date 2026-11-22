// SPDX-License-Identifier: GPL-3.0-or-later
//! "Public rooms" catalog (BrowsePublicRoomsModal.tsx): search, join, open. Public rooms show in
//! the sidebar only after you join them.

use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill};

use super::display;
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, truncate, El};
use crate::icons;
use crate::pages::chat_page::{Browse, ChatMsg};
use crate::theme::*;

pub fn view(b: &Browse) -> El<'_> {
    let msg = Message::Chat;
    let header = row![
        icon(icons::compass(), 17, INDIGO_SOFT),
        text("Public rooms").size(16).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(ChatMsg::CloseBrowse)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let search = row![
        text_input("Search rooms…", &b.query).size(13).padding([9, 12]).style(field).on_input(|v| Message::Chat(ChatMsg::BrowseQuery(v))).on_submit(msg(ChatMsg::BrowseSearch)),
        button(icon(icons::search(), 15, Color::WHITE)).padding([9, 12]).style(primary).on_press(msg(ChatMsg::BrowseSearch)),
    ]
    .spacing(8);

    let body: El = match &b.results {
        None => center(text("Loading…").size(13).color(TEXT_SECONDARY)).height(200).into(),
        Some(c) if c.rooms.is_empty() => center(text("No public rooms found").size(13).color(TEXT_SECONDARY)).height(200).into(),
        Some(c) => {
            let mut list = Column::new().spacing(6);
            for r in &c.rooms {
                let joining = b.joining.as_deref() == Some(r.room_key.as_str());
                let action = if r.is_member {
                    button(text("Open").size(12).font(SEMIBOLD)).padding([6, 14]).style(secondary).on_press(Message::Chat(ChatMsg::OpenRoom(r.room_key.clone())))
                } else {
                    button(text(if joining { "Joining…" } else { "Join" }).size(12).font(SEMIBOLD))
                        .padding([6, 14])
                        .style(primary)
                        .on_press_maybe((!joining && b.joining.is_none()).then(|| Message::Chat(ChatMsg::Join(r.room_key.clone()))))
                };
                let mut meta = format!("{} member{}", r.member_count, if r.member_count == 1 { "" } else { "s" });
                if let Some(at) = r.last_activity.as_deref() {
                    meta.push_str(&format!(" · active {}", display::short_time(at)));
                }
                list = list.push(
                    container(
                        row![
                            container(icon(icons::hash(), 15, Color::WHITE)).width(32).height(32).center(32).style(round_fill(gradient(hex(0x6366f1), hex(0x38bdf8)), 9.0)),
                            column![
                                text(&r.title).size(13.5).font(BOLD).color(Color::WHITE).wrapping(Wrapping::None),
                                text(truncate(r.topic.as_deref().unwrap_or(""), 90)).size(11.5).color(TEXT_SECONDARY).wrapping(Wrapping::None),
                                text(meta).size(10.5).color(FAINT),
                            ]
                            .spacing(2)
                            .width(Fill),
                            action,
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    )
                    .padding([10, 12])
                    .style(crate::theme::soft_row),
                );
            }
            let total = text(format!("{} room{}", c.total, if c.total == 1 { "" } else { "s" })).size(11).color(FAINT);
            column![total, scrollable(list).height(360).style(thin_scroll)].spacing(8).into()
        }
    };

    overlay::modal(container(column![header, search, body].spacing(14)).width(520).padding(20).style(modal), msg(ChatMsg::CloseBrowse))
}
