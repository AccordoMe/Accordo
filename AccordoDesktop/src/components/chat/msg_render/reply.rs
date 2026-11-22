// SPDX-License-Identifier: GPL-3.0-or-later
//! The quoted message above a reply — "↱ @alina  what they said". A message that has no readable
//! text (a picture, a cheque, a voice message…) says what it is instead, by type.

use iced::widget::text::Wrapping;
use iced::widget::{container, row, space, text};
use iced::{Alignment, Padding};

use super::super::display;
use super::super::timestamp;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::{icon, truncate, El};
use crate::icons;
use crate::theme::*;

/// One line of what was answered. `None` when the message isn't a reply.
pub fn quote(m: &ChatMessageItem) -> Option<El<'_>> {
    let q = m.reply_to_data.as_ref()?;
    let who = q.get("sender_display_name").or_else(|| q.get("sender_username")).and_then(|v| v.as_str()).unwrap_or("someone");
    let kind = q.get("message_type").and_then(|v| v.as_str()).unwrap_or("text");
    let said = q
        .get("content")
        .and_then(|v| v.as_str())
        .filter(|c| !c.is_empty() && !display::looks_encrypted(c))
        .map(|c| timestamp::to_plain(c).replace('\n', " "));
    let said = match (said, kind) {
        (Some(c), _) => c,
        (None, "image") => "📷 Photo".into(),
        (None, "cheque") => "🎁 Cheque".into(),
        (None, "gif") => "GIF".into(),
        (None, "sticker") => "Sticker".into(),
        (None, "audio") => "🎙️ Voice message".into(),
        (None, "video") => "📹 Video message".into(),
        (None, _) if q.get("encrypted").is_some() => "🔒 Encrypted message".into(),
        _ => "📎 Attachment".into(),
    };
    Some(
        container(
            row![
                container(space().width(22).height(8)).style(|_| iced::widget::container::Style {
                    border: iced::Border { color: rgba(0xffffff, 0.18), width: 0.0, radius: 6.0.into() },
                    ..Default::default()
                }),
                icon(icons::reply(), 11, FAINT),
                text(format!("@{who}")).size(12).font(SEMIBOLD).color(INDIGO_SOFT),
                text(truncate(&said, 90)).size(12).color(MUTED).wrapping(Wrapping::None),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding { top: 0.0, right: 0.0, bottom: 2.0, left: 0.0 })
        .into(),
    )
}
