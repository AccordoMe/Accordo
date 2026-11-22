// SPDX-License-Identifier: GPL-3.0-or-later
//! A file under a message: its name, what it is and its size, and a Save button. Also what a voice
//! message or video note falls back to when the file itself isn't here (sealed, or too old).

use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, text};
use iced::{Alignment, Color, Fill};

use super::super::message_list::Feed;
use super::open_attachment;
use crate::api::chat::ChatMessageItem;
use crate::app::Message;
use crate::components::ui::{icon, truncate, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

pub fn card<'a>(m: &'a ChatMessageItem, _feed: &Feed<'a>) -> Option<El<'a>> {
    let a = open_attachment(m)?;
    let name = a.get("file_name").and_then(|v| v.as_str()).unwrap_or("file").to_string();
    let size = a.get("file_size").and_then(|v| v.as_u64()).map(crate::media::human_size).unwrap_or_default();
    let (handle, tint, kind) = match m.message_type.as_str() {
        "audio" => (icons::music(), hex(0xc084fc), "Audio"),
        "video" => (icons::video(), SKY, "Video"),
        _ => (icons::file_text(), INDIGO_SOFT, "File"),
    };
    Some(
        container(
            row![
                container(center(icon(handle, 18, tint))).width(38).height(38).style(boxed(Color { a: 0.14, ..tint }, Color::TRANSPARENT, 9.0)),
                column![text(truncate(&name, 48)).size(13).font(SEMIBOLD).color(Color::WHITE).wrapping(Wrapping::None), text(format!("{kind} · {size}")).size(11.5).color(MUTED)]
                    .spacing(2)
                    .width(Fill),
                button(row![icon(icons::download(), 13, INDIGO_SOFT), text("Save").size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center))
                    .padding([5, 10])
                    .style(outline_indigo)
                    .on_press(Message::Chat(ChatMsg::SaveAttachment(m.id))),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(360)
        .padding([10, 12])
        .style(boxed(rgba(0xffffff, 0.04), rgba(0xffffff, 0.08), 10.0))
        .into(),
    )
}
