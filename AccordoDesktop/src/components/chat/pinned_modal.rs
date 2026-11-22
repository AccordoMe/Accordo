// SPDX-License-Identifier: GPL-3.0-or-later
//! The room's pinned messages (PinnedMessagesModal.tsx): who, when, what — jump to one, or unpin.
//! Like the web, it lists the pinned messages among those loaded.

use iced::widget::{button, column, container, row, scrollable, space, text, Column};
use iced::{Alignment, Color, Fill};

use super::display;
use crate::api::chat::ChatMessageItem;
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, with_emoji, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

fn msg(m: ChatMsg) -> Message {
    Message::Chat(m)
}

fn describe(m: &ChatMessageItem) -> String {
    let (body, placeholder) = display::message_text(m);
    if !placeholder || display::looks_encrypted(&m.content) {
        return super::timestamp::to_plain(&body);
    }
    match m.message_type.as_str() {
        "image" => "📷 Photo Attachment".into(),
        "cheque" => "🎁 Web3 Cheque".into(),
        "invoice" => "🧾 Web3 Invoice".into(),
        "audio" => "🎙️ Voice Message".into(),
        "video" => "📹 Video Message".into(),
        _ => "📎 File Attachment".into(),
    }
}

pub fn view<'a>(pinned: Vec<&'a ChatMessageItem>, can_unpin: bool) -> El<'a> {
    let header = row![
        icon(icons::pin(), 18, AMBER),
        text(format!("Pinned Messages ({})", pinned.len())).size(16).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(ChatMsg::ClosePinned)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut list = Column::new().spacing(10);
    if pinned.is_empty() {
        list = list.push(container(text("No pinned messages in this room yet.").size(14).color(MUTED)).width(Fill).padding([30, 0]).align_x(iced::alignment::Horizontal::Center));
    }
    for m in pinned {
        let who = m.sender_display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| m.sender_username.clone());
        let mut actions = row![button(text("Jump to msg").size(11).font(BOLD)).padding([4, 9]).style(outline_indigo).on_press(msg(ChatMsg::JumpTo(m.id)))].spacing(6);
        if can_unpin {
            actions = actions.push(button(text("Unpin").size(11).font(BOLD).color(DANGER_TEXT)).padding([4, 9]).style(tone_button(Tone::Danger)).on_press(msg(ChatMsg::TogglePin(m.id))));
        }
        list = list.push(
            container(
                column![
                    row![text(format!("@{who}")).size(13).font(BOLD).color(INDIGO_ICON), text(display::stamp(&m.created_at)).size(11).color(FAINT), space().width(Fill), actions].spacing(8).align_y(Alignment::Center),
                    with_emoji(&describe(m), 13.0, TEXT, INTER),
                ]
                .spacing(8),
            )
            .width(Fill)
            .padding([12, 14])
            .style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.08), 10.0)),
        );
    }

    let card = container(column![container(header).padding([16, 20]), container(space().width(Fill).height(1)).style(rule), scrollable(container(list).padding([16, 20])).height(iced::Length::Shrink).style(thin_scroll)])
        .width(560)
        .max_height(600)
        .style(modal);
    overlay::modal(card, msg(ChatMsg::ClosePinned))
}
