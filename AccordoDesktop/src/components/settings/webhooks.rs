// SPDX-License-Identifier: GPL-3.0-or-later
//! "Webhooks" (WebhooksCard.tsx): signed POSTs to your server when an invoice is paid.
//! The status is read from the API; changing it is left to the web app for now.

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Font};

use super::{card, note, pill};
use crate::api::account::parse_time;
use crate::api::webhooks::WebhookStatus;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::settings_page::SettingsMsg;
use crate::theme::*;

pub fn view<'a>(status: Option<&'a WebhookStatus>, url: &'a str, docs_open: bool) -> El<'a> {
    let active = status.is_some_and(|s| s.active);
    let state: El = match status {
        None => pill("…", TEXT_SECONDARY, rgba(0xffffff, 0.06), rgba(0xffffff, 0.0)),
        Some(s) if s.active => {
            let until = s.until.as_deref().and_then(parse_time).map(|t| t.format("%b %d, %Y").to_string()).unwrap_or_default();
            container(text(format!("Active · until {until}")).size(10.5).font(BOLD).color(GREEN)).padding([2, 7]).style(badge(rgba(0x10b981, 0.12), rgba(0x10b981, 0.0))).into()
        }
        Some(_) => pill("Not active", MUTED, rgba(0xffffff, 0.06), rgba(0xffffff, 0.0)),
    };

    let mut intro = String::from("When one of your invoices is paid — confirmed from the blockchain — Accordo sends a signed POST to your server, so your shop can release the order on its own.");
    if !active {
        intro.push_str(" Webhooks are a paid add-on ($5 / month); ask us to turn them on for your account.");
    }

    let small = |handle, caption: &'static str, message: SettingsMsg| -> El<'a> {
        button(row![icon(handle, 12, LABEL), text(caption).size(11)].spacing(5).align_y(Alignment::Center))
            .padding([5, 9])
            .style(tone_button(Tone::Dark))
            .on_press_maybe(active.then(|| Message::Settings(message)))
            .into()
    };
    let mut body = column![
        note(intro),
        column![
            text("Default endpoint (every invoice without its own)").size(11.5).font(SEMIBOLD).color(LABEL),
            row![
                text_input("https://your-shop.com/webhooks/accordo", url).size(12).padding([8, 10]).style(field).font(Font::MONOSPACE).on_input_maybe(active.then_some(|v| Message::Settings(SettingsMsg::WebhookUrl(v)))),
                button(text("Save").size(11.5).font(SEMIBOLD)).padding([8, 12]).style(tone_button(Tone::Accent)).on_press_maybe(active.then(|| Message::NotYet("Saving the webhook endpoint"))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(6),
        row![
            small(icons::key_round(), if status.is_some_and(|s| s.has_secret) { "New signing secret" } else { "Create signing secret" }, SettingsMsg::NotYet("Webhook secrets")),
            small(icons::send(), "Send test event", SettingsMsg::NotYet("Test events")),
            button(icon(icons::refresh_cw(), 12, LABEL)).padding([5, 8]).style(tone_button(Tone::Dark)).on_press(Message::Settings(SettingsMsg::RefreshWebhooks)),
        ]
        .spacing(6),
        button(
            row![icon(if docs_open { icons::chevron_down() } else { icons::chevron_right() }, 12, TEXT_SECONDARY), text("How to check the signature").size(11.5).color(TEXT_SECONDARY)]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .padding(0)
        .style(link_button)
        .on_press(Message::Settings(SettingsMsg::WebhookDocs)),
    ]
    .spacing(12);
    if docs_open {
        body = body.push(
            container(note("Each request carries X-Accordo-Timestamp and X-Accordo-Signature: \"sha256=\" + the hex HMAC-SHA256 of \"<timestamp>.<raw body>\" with your signing secret. Compute it on your side, compare in constant time, and reject timestamps older than 5 minutes. Retries reuse X-Accordo-Delivery — dedupe on it."))
                .padding([8, 10])
                .style(crate::theme::soft_row),
        );
    }
    body = body.push(column![text("Recent deliveries").size(11.5).font(SEMIBOLD).color(LABEL), text("Nothing sent yet").size(11.5).color(FAINT)].spacing(4));

    card(icons::webhook(), 0x818cf8, "Webhooks", Some(state), false, body.into())
}
