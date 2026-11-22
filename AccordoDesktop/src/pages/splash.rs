// SPDX-License-Identifier: GPL-3.0-or-later
//! Shown for a moment at start-up while a saved session is checked with the server.

use iced::widget::{center, column, svg, text};
use iced::Alignment;

use crate::components::ui::El;
use crate::icons;
use crate::theme::*;

pub fn view<'a>() -> El<'a> {
    center(
        column![svg(icons::logo()).width(56).height(56), text("Signing in…").size(13).color(TEXT_SECONDARY)]
            .spacing(14)
            .align_x(Alignment::Center),
    )
    .style(panel(BG_DARK))
    .into()
}
