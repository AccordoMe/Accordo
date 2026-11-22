// SPDX-License-Identifier: GPL-3.0-or-later
//! "Session & Account Actions" (SessionsCard.tsx + the logout buttons of SettingsPage.tsx):
//! where the account is signed in — from the API, with sign-out per device — and logging out.

use iced::widget::{button, center, column, container, rich_text, row, space, span, text, Column};
use iced::{Alignment, Color, Fill, Font};

use super::{card, note, pill};
use crate::api::account::{ago, parse_time, DeviceSession};
use crate::api::auth::User;
use crate::app::{Message, SessionMsg};
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::settings_page::SettingsMsg;
use crate::theme::*;

pub fn view<'a>(user: &'a User, sessions: Option<&'a [DeviceSession]>, busy: Option<&'a str>) -> El<'a> {
    let msg = Message::Settings;
    let intro = rich_text![
        span::<(), Font>("Signed in as ").color(TEXT_SECONDARY),
        span(format!("@{}", user.username)).color(Color::WHITE).font(BOLD),
        span(". Sign out a device you don't recognise, or this one.").color(TEXT_SECONDARY),
    ]
    .size(13);

    let mut list: Column<'a, Message> = column![text("Where you're signed in").size(11.5).font(SEMIBOLD).color(LABEL)].spacing(6);
    match sessions {
        None => list = list.push(note("Loading…")),
        Some(items) => {
            for s in items {
                let glyph = if s.via_api_key {
                    icons::key_round()
                } else if ["Android", "iPhone", "iPad", "app"].iter().any(|k| s.device.contains(k)) {
                    icons::smartphone()
                } else {
                    icons::laptop()
                };
                let signed_in = s.signed_in_at.as_deref().and_then(parse_time).map(|t| t.format("%b %d, %Y").to_string()).unwrap_or_default();
                let mut detail = format!("Signed in {signed_in}");
                if !s.current {
                    if let Some(at) = &s.last_active_at {
                        detail.push_str(&format!(" · active {}", ago(at)));
                    }
                }
                let mut title = row![text(format!("{}{}", s.device, if s.via_api_key { " · API key" } else { "" })).size(12.5).font(SEMIBOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center);
                if s.current {
                    title = title.push(pill("THIS DEVICE", GREEN, rgba(0x10b981, 0.15), rgba(0x10b981, 0.0)));
                }
                let mut line = row![icon(glyph, 17, TEXT_SECONDARY), column![title, text(detail).size(11).color(TEXT_SECONDARY)].spacing(2).width(Fill)].spacing(12).align_y(Alignment::Center);
                if !s.current {
                    let id = s.id.clone();
                    line = line.push(
                        button(text(if busy == Some(s.id.as_str()) { "…" } else { "Sign out" }).size(11).font(SEMIBOLD))
                            .padding([4, 9])
                            .style(tone_button(Tone::Dark))
                            .on_press_maybe(busy.is_none().then(|| msg(SettingsMsg::EndSession(id)))),
                    );
                }
                list = list.push(container(line).width(Fill).padding([10, 14]).style(crate::theme::soft_row));
            }
            let others = items.iter().any(|s| !s.current);
            list = list.push(
                button(row![icon(icons::log_out(), 13, LABEL), text(if busy == Some("others") { "Signing out…" } else if others { "Sign out all other sessions" } else { "Sign out everywhere else" }).size(11.5).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                    .padding([6, 10])
                    .style(tone_button(Tone::Dark))
                    .on_press_maybe(busy.is_none().then(|| msg(SettingsMsg::EndOthers))),
            );
        }
    }

    card(
        icons::log_out(),
        0xf87171,
        "Session & Account Actions",
        None,
        false,
        column![
            intro,
            list,
            note("We keep the device name and roughly when it was last active — not your IP address or location."),
            space().height(2),
            button(center(row![icon(icons::log_out(), 14, DANGER_TEXT), text("Log Out from Accordo").size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).height(18))
                .width(Fill)
                .padding([8, 0])
                .style(tone_button(Tone::Danger))
                .on_press(Message::Session(SessionMsg::Logout)),
            button(center(row![icon(icons::trash(), 13, DANGER_TEXT), text("Log out & remove keys from this device").size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).height(16))
                .width(Fill)
                .padding([7, 0])
                .style(|_, status| iced::widget::button::Style {
                    background: Some(rgba(0xef4444, if matches!(status, iced::widget::button::Status::Hovered) { 0.08 } else { 0.0 }).into()),
                    text_color: DANGER_TEXT,
                    border: iced::Border { color: rgba(0xef4444, 0.6), width: 1.0, radius: 7.0.into() },
                    ..Default::default()
                })
                .on_press(Message::Session(SessionMsg::Logout)),
            note("On a shared or someone else's computer: also deletes your keys from this device. Normal log out keeps them, so your chats open right away next time."),
        ]
        .spacing(12)
        .into(),
    )
}
