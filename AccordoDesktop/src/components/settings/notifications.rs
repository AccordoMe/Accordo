// SPDX-License-Identifier: GPL-3.0-or-later
//! "Notifications" (NotificationsCard.tsx): per-device switches.

use iced::widget::{column, container, row, space, text, toggler};
use iced::{Alignment, Color, Fill};

use super::card;
use crate::app::Message;
use crate::components::ui::El;
use crate::icons;
use crate::pages::settings_page::{SettingsMsg, State};
use crate::theme::*;

pub fn view(state: &State) -> El<'_> {
    let switch_row = |title: &'static str, detail: &'static str, on: bool, msg: fn(bool) -> SettingsMsg| -> El<'_> {
        container(
            row![
                column![text(title).size(13).font(BOLD).color(Color::WHITE), text(detail).size(12).color(TEXT_SECONDARY).line_height(1.4)].spacing(2).width(Fill),
                toggler(on).size(22).style(switch).on_toggle(move |v| Message::Settings(msg(v))),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([10, 12])
        .style(crate::theme::soft_row)
        .into()
    };
    card(
        icons::bell(),
        0x818cf8,
        "Notifications",
        Some(text("this device").size(12).color(TEXT_SECONDARY).into()),
        false,
        column![
            switch_row("Sounds", "A short chime for direct messages, mentions and payments.", state.sounds, SettingsMsg::Sounds),
            switch_row("System notifications", "Pop-ups from your desktop while Accordo is in the background.", state.system_notifications, SettingsMsg::SystemNotifications),
            switch_row("Show message text", "Off: notifications only say \"New message\".", state.preview, SettingsMsg::Preview),
            space().height(0),
        ]
        .spacing(10)
        .into(),
    )
}
