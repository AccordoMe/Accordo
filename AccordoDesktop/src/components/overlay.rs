// SPDX-License-Identifier: GPL-3.0-or-later
//! Layers over the page: a centred modal and a right-hand drawer, each on a dimmed backdrop
//! that closes it when clicked.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, mouse_area, opaque};
use iced::Fill;

use super::ui::El;
use crate::app::Message;
use crate::theme;

pub fn modal<'a>(content: impl Into<El<'a>>, on_close: Message) -> El<'a> {
    mouse_area(
        container(opaque(content))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(theme::backdrop),
    )
    .on_press(on_close)
    .into()
}

pub fn drawer<'a>(content: impl Into<El<'a>>, width: f32, on_close: Message) -> El<'a> {
    mouse_area(
        container(opaque(container(content).width(width).height(Fill).style(theme::drawer)))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Right)
            .style(theme::backdrop),
    )
    .on_press(on_close)
    .into()
}
