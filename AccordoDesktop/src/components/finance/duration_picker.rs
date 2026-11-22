// SPDX-License-Identifier: GPL-3.0-or-later
//! "Cheque expiry" / "Payment due date" / "Campaign duration": preset chips and the resulting date.

use iced::widget::{button, column, container, row, space, text, Row};
use iced::{Alignment, Fill};

use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duration {
    Hours(u32),
    Days(u32),
    /// "Custom" chip: opens the time window.
    Custom,
    /// What the time window gave back (unix seconds).
    At(i64),
    Never,
}

impl Duration {
    fn label(self) -> String {
        match self {
            Duration::Hours(h) => format!("{h} hours"),
            Duration::Days(d) => format!("{d} days"),
            Duration::Custom | Duration::At(_) => "Custom".into(),
            Duration::Never => "No expiry".into(),
        }
    }

    /// The unix time it ends at, from `now`; None for no end (or a custom one not picked yet).
    pub fn ends_at(self, now: i64) -> Option<i64> {
        match self {
            Duration::Hours(h) => Some(now + i64::from(h) * 3_600),
            Duration::Days(d) => Some(now + i64::from(d) * 86_400),
            Duration::At(t) => Some(t),
            Duration::Custom | Duration::Never => None,
        }
    }

    /// "≈ Dec 15, 2026, 09:24 AM" for the chosen preset.
    fn until(self) -> Option<String> {
        let delta = match self {
            Duration::Hours(h) => chrono::Duration::hours(h.into()),
            Duration::Days(d) => chrono::Duration::days(d.into()),
            Duration::At(t) => return chrono::TimeZone::timestamp_opt(&chrono::Local, t, 0).single().map(|t| t.format("%b %d, %Y, %I:%M %p").to_string()),
            _ => return None,
        };
        Some(format!("≈ {}", (chrono::Local::now() + delta).format("%b %d, %Y, %I:%M %p")))
    }
}

/// `never_label`: what "no end" is called here ("No expiry", "No due date"; None hides it).
pub fn view<'a>(caption: &'a str, presets: &[Duration], selected: Duration, never_label: Option<&'a str>, on_pick: impl Fn(Duration) -> Message + 'a) -> El<'a> {
    let mut chips: Row<'a, Message> = row![].spacing(6);
    let mut options = presets.to_vec();
    options.push(Duration::Custom);
    if never_label.is_some() {
        options.push(Duration::Never);
    }
    for d in options {
        // The "Custom" chip shows the picked moment once there is one, and opens the window again.
        let is_on = d == selected || (d == Duration::Custom && matches!(selected, Duration::At(_)));
        let caption = match (d, selected) {
            (Duration::Never, _) => never_label.unwrap_or("No expiry").to_string(),
            (Duration::Custom, Duration::At(t)) => chrono::TimeZone::timestamp_opt(&chrono::Local, t, 0).single().map(|t| t.format("%b %-d, %H:%M").to_string()).unwrap_or_else(|| "Custom".into()),
            (other, _) => other.label(),
        };
        let mut content = row![].spacing(4).align_y(Alignment::Center);
        if d == Duration::Custom {
            content = content.push(icon(icons::calendar(), 11, TEXT_SECONDARY));
        }
        content = content.push(text(caption).size(11).font(if is_on { BOLD } else { INTER }));
        chips = chips.push(button(content).padding([3, 8]).style(chip(is_on)).on_press(on_pick(d)));
    }
    let until = match selected {
        Duration::Never => "never expires".to_string(),
        Duration::Custom => "pick a date".to_string(),
        other => other.until().unwrap_or_default(),
    };
    container(
        column![
            row![
                icon(icons::clock(), 12, TEXT_SECONDARY),
                text(caption).size(12).font(SEMIBOLD).color(LABEL),
                space().width(Fill),
                text(until).size(11).color(TEXT_SECONDARY),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            chips,
        ]
        .spacing(8),
    )
    .width(Fill)
    .padding([10, 12])
    .style(inset)
    .into()
}
