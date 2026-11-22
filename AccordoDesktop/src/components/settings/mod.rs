// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings cards (src/components/settings/ and the cards inside SettingsPage.tsx).

pub mod account_profile;
pub mod api_access;
pub mod chat_encryption;
pub mod connected_apps;
pub mod notifications;
pub mod master_vault;
pub mod sessions;
pub mod sign_in_security;
pub mod webhooks;

use iced::widget::{button, center, column, container, row, space, svg, text};
use iced::{Alignment, Color, Fill};

use super::ui::{icon, El};
use crate::app::Message;
use crate::theme::*;

/// A settings card: tinted icon tile, title, optional thing on the right, then the body.
pub fn card<'a>(handle: svg::Handle, tint: u32, title: &'a str, trailing: Option<El<'a>>, accent: bool, body: El<'a>) -> El<'a> {
    let mut header = row![
        container(icon(handle, 16, hex(tint))).width(28).height(28).center(28).style(icon_tile(rgba(tint, 0.15))),
        text(title).size(18).font(SEMIBOLD).color(Color::WHITE),
        space().width(Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(t) = trailing {
        header = header.push(t);
    }
    container(column![header, space().height(8), body].spacing(16)).width(Fill).padding(24).style(settings_card(accent)).into()
}

/// A faint inset row: label on the left, value on the right.
pub fn soft_row<'a>(label: &'a str, value: El<'a>) -> El<'a> {
    container(row![text(label).size(15).color(TEXT_SECONDARY), space().width(Fill), value].spacing(10).align_y(Alignment::Center))
        .width(Fill)
        .padding([12, 14])
        .style(soft_row_style)
        .into()
}

fn soft_row_style(theme: &iced::Theme) -> container::Style {
    crate::theme::soft_row(theme)
}

pub fn note<'a>(content: impl text::IntoFragment<'a>) -> El<'a> {
    text(content).size(12).color(TEXT_SECONDARY).line_height(1.5).into()
}

pub fn pill<'a>(label: impl text::IntoFragment<'a>, fg: Color, bg: Color, line: Color) -> El<'a> {
    container(text(label).size(10.5).font(BOLD).color(fg)).padding([2, 7]).style(badge(bg, line)).into()
}

/// Card button; `full` stretches it across the card.
pub fn action<'a>(label: &'a str, handle: Option<svg::Handle>, tone: Tone, message: Message, full: bool) -> El<'a> {
    let fg = match tone {
        Tone::Accent => Color::WHITE,
        Tone::Dark => hex(0xe2e8f0),
        Tone::Danger => DANGER_TEXT,
    };
    let mut content = row![].spacing(6).align_y(Alignment::Center);
    if let Some(h) = handle {
        content = content.push(icon(h, 13, fg));
    }
    content = content.push(text(label).size(12).font(SEMIBOLD));
    let b = if full { button(center(content).height(16)).width(Fill) } else { button(content) };
    b.padding([8, 12]).style(tone_button(tone)).on_press(message).into()
}
