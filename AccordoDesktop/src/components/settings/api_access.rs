// SPDX-License-Identifier: GPL-3.0-or-later
//! "API Access & Developer Keys": API-key sign-in switch and the key itself (hidden).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Color, Font};

use super::{action, card, note, soft_row};
use crate::api::auth::User;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

pub fn view(user: &User) -> El<'_> {
    let checkbox = container(if user.api_login { icon(icons::check_check(), 10, Color::WHITE) } else { icon(icons::x(), 1, Color::TRANSPARENT) })
        .width(13)
        .height(13)
        .center(13)
        .style(move |_| iced::widget::container::Style {
            background: Some(if user.api_login { INDIGO } else { hex(0xe5e7eb) }.into()),
            border: iced::Border { radius: 3.0.into(), ..Default::default() },
            ..Default::default()
        });
    let login = row![checkbox, text(if user.api_login { "Enabled" } else { "Disabled" }).size(13).font(SEMIBOLD).color(TEXT_BODY)]
        .spacing(7)
        .align_y(Alignment::Center);
    let key = container(text(if user.api_key.is_some() { "sk_•••••••• (hidden)" } else { "none yet" }).size(12).font(Font::MONOSPACE).color(TEXT_BODY))
        .padding([4, 8])
        .style(boxed(hex(0x0b0c12), BORDER, 6.0));

    card(
        icons::code(),
        0xf59e0b,
        "API Access & Developer Keys",
        None,
        false,
        column![
            soft_row("Enable API Login", login.into()),
            soft_row("Active API Key", key.into()),
            note("API-key sessions can use your account but can't change your password-protected settings: wallets, E2EE keys or this key."),
            action("Regenerate API Key", None, Tone::Danger, Message::NotYet("API keys"), true),
        ]
        .spacing(12)
        .into(),
    )
}
