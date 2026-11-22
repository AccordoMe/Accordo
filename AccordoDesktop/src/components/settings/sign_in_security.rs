// SPDX-License-Identifier: GPL-3.0-or-later
//! "Sign-in Security" (SignInSecurityCard.tsx): two-factor login and recovery codes.

use iced::widget::{column, text};

use super::{action, card, note, soft_row};
use crate::api::auth::User;
use crate::app::Message;
use crate::components::ui::El;
use crate::icons;
use crate::theme::*;

pub fn view(user: &User) -> El<'_> {
    let two_factor = if user.totp_enabled { text("On").size(13).font(BOLD).color(GREEN) } else { text("Off").size(13).font(BOLD).color(TEXT_SECONDARY) };
    let codes_color = if user.recovery_codes_left == 0 { AMBER } else { TEXT_BODY };
    card(
        icons::shield_check(),
        0x34d399,
        "Sign-in Security",
        None,
        false,
        column![
            soft_row("Two-factor login", two_factor.into()),
            soft_row("Recovery codes", text(format!("{} left", user.recovery_codes_left)).size(13).font(BOLD).color(codes_color).into()),
            note("Add a code from your phone to every sign-in, so a leaked password alone is not enough."),
            action(if user.totp_enabled { "Manage two-factor" } else { "Turn on two-factor" }, None, Tone::Accent, Message::NotYet("Two-factor setup"), true),
        ]
        .spacing(12)
        .into(),
    )
}
