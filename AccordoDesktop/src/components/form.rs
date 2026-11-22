// SPDX-License-Identifier: GPL-3.0-or-later
//! Form building blocks shared by the finance pages: labels, inputs, the form card.

use iced::widget::{container, row, text, text_input, TextInput};
use iced::{Alignment, Color, Fill};

use super::ui::El;
use crate::app::Message;
use crate::theme::*;

/// "Token", "Total amount *" — the red star marks required fields.
pub fn label<'a>(caption: &'a str, required: bool) -> El<'a> {
    let mut line = row![text(caption).size(12.5).font(MEDIUM).color(LABEL)].spacing(3).align_y(Alignment::Center);
    if required {
        line = line.push(text("*").size(12.5).color(RED));
    }
    line.into()
}

/// The global `input` look, full width.
pub fn input<'a>(placeholder: &str, value: &str, on_input: impl Fn(String) -> Message + 'a) -> TextInput<'a, Message> {
    text_input(placeholder, value).on_input(on_input).size(13.5).padding([10, 12]).width(Fill).style(field)
}

/// A taller input for notes (the web uses a textarea).
pub fn note_input<'a>(placeholder: &str, value: &str, on_input: impl Fn(String) -> Message + 'a) -> TextInput<'a, Message> {
    input(placeholder, value, on_input).padding([14, 12])
}

/// The card around a form ("Write a cheque", "Issue an Invoice").
pub fn form_card<'a>(content: impl Into<El<'a>>) -> El<'a> {
    container(content).width(Fill).padding(24).style(settings_card(false)).into()
}

pub fn title<'a>(heading: &'a str, subtitle: &'a str) -> El<'a> {
    iced::widget::column![
        text(heading).size(20).font(BOLD).color(Color::WHITE),
        text(subtitle).size(13).color(TEXT_SECONDARY),
    ]
    .spacing(4)
    .into()
}
