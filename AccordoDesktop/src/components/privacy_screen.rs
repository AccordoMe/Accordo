// SPDX-License-Identifier: GPL-3.0-or-later
//! Covers the whole window (Ctrl+Shift+H, or on switching away when "Hide when the window loses
//! focus" is on): nothing of the chats, wallets or amounts shows on a shared screen or in a
//! screenshot of the desktop. A click or any key brings it back.

use crate::app::{WindowMsg};
use iced::widget::{center, column, container, mouse_area, text};
use iced::{Alignment, Fill};

use super::ui::{icon, El};
use crate::app::Message;
use crate::icons;
use crate::theme::*;

pub fn view<'a>(wallets_locked: bool) -> El<'a> {
    let mut col = column![
        container(icon(icons::eye_off(), 30, INDIGO_SOFT)).padding(18).style(boxed(rgba(0x6366f1, 0.12), rgba(0x6366f1, 0.3), 40.0)),
        text("Accordo is hidden").size(22).font(BOLD),
        text("Click anywhere or press any key to show it again.").size(13.5).color(TEXT_SECONDARY),
    ]
    .spacing(12)
    .align_x(Alignment::Center);
    if wallets_locked {
        col = col.push(text("Your wallets were locked.").size(12).color(AMBER_TEXT));
    }
    mouse_area(container(center(col)).width(Fill).height(Fill).style(|_| iced::widget::container::Style { background: Some(hex(0x0b0c10).into()), ..Default::default() }))
        .on_press(Message::Window(WindowMsg::Unhide))
        .into()
}
