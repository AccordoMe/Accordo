// SPDX-License-Identifier: GPL-3.0-or-later
//! "Connected apps" (ConnectedAppsCard.tsx): mini-apps you've granted permissions.

use iced::widget::column;

use super::{card, note};
use crate::components::ui::El;
use crate::icons;

pub fn view<'a>() -> El<'a> {
    card(
        icons::app_window(),
        0x818cf8,
        "Connected apps",
        None,
        false,
        column![
            note("Apps you've answered. Switch any permission off here — it stops at once, in the app and on its server."),
            note("No apps yet. When an app asks you for something, it shows up here."),
        ]
        .spacing(12)
        .into(),
    )
}
