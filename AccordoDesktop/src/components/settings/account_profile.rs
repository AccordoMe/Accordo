// SPDX-License-Identifier: GPL-3.0-or-later
//! "Account Profile": avatar, name, role, email and username.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, stack, text, text_input, Row};
use iced::{Alignment, Color};

use super::{card, pill, soft_row};
use crate::api::auth::User;
use crate::app::Message;
use crate::app::AvatarMsg;
use crate::components::ui::{avatar_pic, icon, El};
use crate::icons;
use crate::theme::*;

/// The web's "Web3 presets" (SettingsPage.tsx AVATAR_PRESETS).
const PRESETS: [&str; 5] = [
    "https://api.dicebear.com/7.x/bottts/svg?seed=AccordoAlpha",
    "https://api.dicebear.com/7.x/bottts/svg?seed=CryptoPioneer",
    "https://api.dicebear.com/7.x/bottts/svg?seed=Web3Guardian",
    "https://api.dicebear.com/7.x/bottts/svg?seed=CyberRunner",
    "https://api.dicebear.com/7.x/bottts/svg?seed=NeonVault",
];

fn am(m: AvatarMsg) -> Message {
    Message::Avatar(m)
}

/// "Custom Avatar": upload a photo, paste a link, or pick a preset; then Save.
fn editor<'a>(value: &str, saving: bool) -> El<'a> {
    let mut presets = Row::new().spacing(8).align_y(Alignment::Center);
    for p in PRESETS {
        let on = value == p;
        presets = presets.push(
            button(avatar_pic("?", Some(p), 32.0, Some(rgba(0xffffff, 0.05).into())))
                .padding(1)
                .style(move |_, _| button::Style { border: iced::Border { color: if on { hex(0x818cf8) } else { rgba(0xffffff, 0.1) }, width: if on { 2.0 } else { 1.0 }, radius: 18.0.into() }, ..Default::default() })
                .on_press(am(AvatarMsg::Url(p.to_string()))),
        );
    }
    container(
        column![
            row![
                text("Custom Avatar").size(12).font(SEMIBOLD).color(Color::WHITE),
                iced::widget::space().width(iced::Fill),
                button(row![icon(icons::upload(), 12, TEXT_BODY), text("Upload Photo").size(11)].spacing(4).align_y(Alignment::Center)).padding([4, 10]).style(tone_button(Tone::Dark)).on_press_maybe((!saving).then(|| am(AvatarMsg::Upload))),
            ]
            .align_y(Alignment::Center),
            row![
                text_input("Or paste image URL (https://...)", value).size(12).padding([6, 10]).style(field).on_input(|v| am(AvatarMsg::Url(v))).on_submit(am(AvatarMsg::Save)),
                button(text(if saving { "Saving…" } else { "Save" }).size(12).font(SEMIBOLD)).padding([6, 12]).style(primary).on_press_maybe((!saving).then(|| am(AvatarMsg::Save))),
            ]
            .spacing(8),
            column![text("Or select a Web3 preset:").size(11).color(TEXT_SECONDARY), presets].spacing(6),
        ]
        .spacing(10),
    )
    .width(iced::Fill)
    .padding([12, 14])
    .style(boxed(rgba(0xffffff, 0.03), rgba(0x6366f1, 0.25), 10.0))
    .into()
}

/// One limit, compact: "Groups   3 / 50" with a thin bar; "No limit" (admins), "Not enabled"
/// (0), or a size ("up to 500").
fn limit_row<'a>(l: &crate::api::auth::AccountLimit) -> El<'a> {
    let (value, color, share): (String, Color, Option<f32>) = match (l.used, l.limit) {
        (_, None) => ("No limit".into(), GREEN, None),
        (None, Some(0)) => ("Not enabled".into(), FAINT, None),
        (None, Some(n)) => (format!("up to {n}"), TEXT_BODY, None),
        (Some(_), Some(0)) => ("Not enabled".into(), FAINT, None),
        (Some(u), Some(n)) => (format!("{u} / {n}"), if u >= n { AMBER_TEXT } else { TEXT_BODY }, Some((u as f32 / n as f32).clamp(0.0, 1.0))),
    };
    let mut col = column![row![text(l.label).size(12.5).color(TEXT_SECONDARY), iced::widget::space().width(iced::Fill), text(value).size(12.5).font(SEMIBOLD).color(color)].align_y(Alignment::Center)].spacing(5);
    if let Some(share) = share {
        let full = share >= 1.0;
        col = col.push(iced::widget::progress_bar(0.0..=1.0, share).girth(3).style(move |_| iced::widget::progress_bar::Style {
            background: rgba(0xffffff, 0.06).into(),
            bar: if full { AMBER_TEXT.into() } else { INDIGO.into() },
            border: iced::Border { radius: 2.0.into(), ..Default::default() },
        }));
    }
    container(col).width(iced::Fill).padding([8, 14]).style(crate::theme::soft_row).into()
}

pub fn view<'a>(user: &'a User, editing: Option<&'a str>, saving: bool, limits: Vec<crate::api::auth::AccountLimit>) -> El<'a> {
    let camera = container(icon(icons::camera(), 11, Color::WHITE))
        .width(22)
        .height(22)
        .center(22)
        .style(|_| iced::widget::container::Style {
            background: Some(INDIGO.into()),
            border: iced::Border { color: hex(0x111216), width: 2.0, radius: 11.0.into() },
            ..Default::default()
        });
    let picture = stack![
        avatar_pic(user.name(), user.picture(), 56.0, Some(gradient(INDIGO, hex(0xa855f7)))),
        container(camera).width(58).height(58).align_x(Horizontal::Right).align_y(Vertical::Bottom),
    ];
    // The label an admin set (member unless changed); admins also get ADMIN.
    let label = user.role.as_deref().filter(|r| !r.is_empty()).unwrap_or("member").to_uppercase();
    let mut role = row![pill(label, INDIGO_SOFT, rgba(0x6366f1, 0.15), rgba(0x6366f1, 0.3))].spacing(6);
    if user.is_admin {
        role = role.push(pill("ADMIN", hex(0xfca5a5), rgba(0xef4444, 0.15), rgba(0xef4444, 0.3)));
    }
    let header = container(
        row![
            picture,
            column![
                row![text(user.name()).size(16).font(BOLD).color(Color::WHITE), role].spacing(8).align_y(Alignment::Center),
                text(format!("@{}", user.username)).size(12.5).color(TEXT_SECONDARY),
                {
                    let mut links = row![button(text(if editing.is_some() { "Close Avatar" } else { "Change Avatar" }).size(11).font(BOLD).color(INDIGO_ICON)).padding(0).style(link_button).on_press(am(AvatarMsg::Edit))].spacing(10);
                    if user.avatar.as_deref().is_some_and(|a| !a.is_empty()) {
                        links = links.push(button(text("Reset").size(11).color(DANGER_TEXT)).padding(0).style(link_button).on_press_maybe((!saving).then(|| am(AvatarMsg::Reset))));
                    }
                    links
                },
            ]
            .spacing(2),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .width(iced::Fill)
    .padding([12, 14])
    .style(crate::theme::soft_row);

    let email: El = match &user.email {
        Some(email) => {
            let mut value = row![text(email).size(15).font(BOLD).color(Color::WHITE)].spacing(8).align_y(Alignment::Center);
            value = value.push(if user.email_verification_required {
                pill("Unverified", AMBER_TEXT, rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.3))
            } else {
                pill("Verified", GREEN, rgba(0x10b981, 0.15), rgba(0x10b981, 0.3))
            });
            value.into()
        }
        None => text("none").size(15).color(FAINT).into(),
    };

    card(
        icons::user(),
        0x60a5fa,
        "Account Profile",
        None,
        false,
        column![
            header,
            editing.map(|v| editor(v, saving)).unwrap_or_else(|| iced::widget::space().height(0).into()),
            soft_row("Email", email),
            soft_row("Username", text(format!("@{}", user.username)).size(15).font(BOLD).color(Color::WHITE).into()),
            match user.orgs.first() {
                // The first (verified first); the rest are in the Organizations card.
                Some(o) => soft_row(if user.orgs.len() > 1 { "Organizations" } else { "Organization" }, {
                    let mut r = row![crate::components::ui::org_chip(o)].spacing(8).align_y(Alignment::Center);
                    if user.orgs.len() > 1 {
                        r = r.push(text(format!("+{}", user.orgs.len() - 1)).size(12).color(TEXT_SECONDARY));
                    }
                    r.into()
                }),
                None => iced::widget::space().height(0).into(),
            },
            // In whatever room the card has left: it never grows the card; rows that don't fit
            // whole aren't shown.
            crate::components::fit_column::FitColumn::new(
                std::iter::once(text("LIMITS").size(10.5).font(BOLD).color(FAINT).into()).chain(limits.iter().map(limit_row)).collect(),
                6.0,
            )
            .titled(),
        ]
        .spacing(12)
        .into(),
    )
}
