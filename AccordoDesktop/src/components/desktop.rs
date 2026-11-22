// SPDX-License-Identifier: GPL-3.0-or-later
//! The frameless window's own controls (components/desktop/WindowControls.tsx).

use iced::widget::{button, center, container, mouse_area, row, space, svg, text};
use iced::{Alignment, Color, Fill, Padding};

use super::ui::El;
use crate::app::{Message, WindowAction, WindowMsg};
use crate::icons;
use crate::theme::{self, *};

/// App settings (gear), then minimize / maximize / close.
pub fn window_controls<'a>() -> El<'a> {
    let control = |handle: svg::Handle, size: u32, close: bool, action: WindowAction| -> El<'a> {
        button(center(svg(handle).width(size).height(size).style(move |_, status| svg::Style {
            color: Some(if close && matches!(status, svg::Status::Hovered) { Color::WHITE } else { MUTED }),
        })))
        .width(34)
        .height(30)
        .padding(0)
        .style(window_control(close))
        .on_press(Message::Window(WindowMsg::Action(action)))
        .into()
    };
    let gear = button(center(svg(icons::settings()).width(15).height(15).style(|_, status| svg::Style {
        color: Some(if matches!(status, svg::Status::Hovered) { Color::WHITE } else { MUTED }),
    })))
    .width(34)
    .height(30)
    .padding(0)
    .style(window_control(false))
    .on_press(Message::AppSettings(crate::components::app_settings_modal::AppSettingsMsg::Open));
    row![
        gear,
        control(icons::minus(), 15, false, WindowAction::Minimize),
        control(icons::square(), 12, false, WindowAction::ToggleMaximize),
        control(icons::x(), 16, true, WindowAction::Close),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

/// Makes `content` move the window when pressed on an empty spot; double click maximizes.
pub fn drag_area<'a>(content: impl Into<El<'a>>) -> El<'a> {
    mouse_area(content)
        .on_press(Message::Window(WindowMsg::Action(WindowAction::Drag)))
        .on_double_click(Message::Window(WindowMsg::Action(WindowAction::ToggleMaximize)))
        .into()
}

/// Slim title bar for screens without the TopBar (sign-in): drag + window buttons.
pub fn fallback_title_bar<'a>() -> El<'a> {
    let bar = container(
        row![
            svg(icons::logo()).width(16).height(16),
            text("Accordo").size(12).font(BOLD).color(hex(0xcbd5e1)),
            // ACCORDO_PROFILE: which test window this is (the top bar shows it once signed in).
            match crate::api::client::profile() {
                Some(p) => El::from(container(text(p).size(10.5).font(BOLD).color(AMBER_TEXT)).padding([0, 6]).style(boxed(rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.35), 5.0))),
                None => space().width(0).into(),
            },
            space().width(Fill),
            window_controls(),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .height(36)
    .padding(Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 12.0 })
    .align_y(iced::alignment::Vertical::Center)
    .style(theme::panel(hex(0x0d0e14)));
    drag_area(bar)
}
