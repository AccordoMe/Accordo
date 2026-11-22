// SPDX-License-Identifier: GPL-3.0-or-later
//! The windows a module's requests open over the app (docs/MINIAPP_API_RU.md §9; the web builds
//! them with openDetailsWindow): the consent screen at the handshake, "Allow <module>?" for a
//! permission, "Open a link?". Every one starts with who's asking — the module's name and where
//! it comes from — so a module can't pass itself off as Accordo.
//!
//! Payments don't come here: they open Accordo's own pay / cheque window, with "Requested by …".

use std::collections::BTreeMap;

use iced::widget::{button, column, container, row, scrollable, space, text, toggler, Column};
use iced::{Alignment, Color, Fill, Font};

use super::ui::{icon, El};
use crate::app::Message;
use crate::icons;
use crate::module_host::{group_of, permission_text, toggle_label, Host, Wait};
use crate::theme::*;

#[derive(Debug, Clone)]
pub enum ModuleWinMsg {
    Toggle(String, bool),
    Confirm,
    Cancel,
}

/// The consent screen's switches (all on to start with).
#[derive(Debug, Clone, Default)]
pub struct State {
    pub toggles: BTreeMap<String, bool>,
}

impl State {
    pub fn for_wait(w: &Wait) -> Self {
        let toggles = match w {
            Wait::Consent(open) => open.iter().map(|p| (p.clone(), true)).collect(),
            _ => BTreeMap::new(),
        };
        State { toggles }
    }
}

fn msg(m: ModuleWinMsg) -> Message {
    Message::Module(crate::app::ModMsg::Window(m))
}

fn header<'a>(host: &Host) -> El<'a> {
    let initial = host.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    row![
        container(text(initial).size(15).font(BOLD).color(Color::WHITE)).width(34).height(34).center(34).style(boxed(rgba(0x6366f1, 0.35), rgba(0x6366f1, 0.5), 9.0)),
        column![
            text(host.name.clone()).size(13.5).font(BOLD).color(Color::WHITE),
            row![text(host.origin.clone()).size(11).font(Font::MONOSPACE).color(TEXT_SECONDARY), text("· Unverified").size(11).color(AMBER_TEXT)].spacing(4),
        ]
        .spacing(2),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn line<'a>(label: &str, value: &str, mono: bool) -> El<'a> {
    row![
        text(label.to_string()).size(12).color(TEXT_SECONDARY).width(90),
        text(value.to_string()).size(12.5).font(if mono { Font::MONOSPACE } else { SEMIBOLD }).color(TEXT_BODY).width(Fill),
    ]
    .spacing(10)
    .into()
}

fn note<'a>(t: &str) -> El<'a> {
    container(text(t.to_string()).size(11.5).color(TEXT_SECONDARY).line_height(1.45)).width(Fill).padding([8, 10]).style(boxed(rgba(0xffffff, 0.03), BORDER, 8.0)).into()
}

/// The window for the front request (None: it doesn't take one of these).
pub fn view<'a>(host: &'a Host, wait: &'a Wait, st: &'a State) -> Option<El<'a>> {
    let (icon_handle, title, confirm, cancel, body): (_, String, &str, &str, El<'a>) = match wait {
        Wait::Consent(open) => {
            let mut col: Column<'a, Message> = column![].spacing(6);
            let mut last = "";
            for p in open {
                let g = group_of(p);
                if g != last {
                    col = col.push(text(g.to_uppercase()).size(10).font(BOLD).color(FAINT));
                    last = g;
                }
                let on = st.toggles.get(p).copied().unwrap_or(true);
                let key = p.clone();
                col = col.push(
                    row![
                        column![text(toggle_label(p)).size(12.5).font(SEMIBOLD).color(Color::WHITE), text(permission_text(p)).size(11).color(TEXT_SECONDARY)].spacing(1).width(Fill),
                        toggler(on).size(18).style(switch).on_toggle(move |v| msg(ModuleWinMsg::Toggle(key.clone(), v))),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
            col = col.push(note("Switch off what you don't want. You can change this later; every payment is still confirmed on its own."));
            // A fixed height for the list: the buttons below always stay in view; the switches
            // keep clear of the scrollbar.
            let list = scrollable(container(col).padding(iced::Padding { right: 14.0, ..Default::default() })).height(340).style(thin_scroll);
            (icons::shield(), format!("{} would like to", host.name), "Allow selected", "Deny all", list.into())
        }
        Wait::Allow { perms, extra, .. } => {
            let mut col: Column<'a, Message> = column![].spacing(6);
            for p in perms {
                col = col.push(row![icon(icons::check(), 13, GREEN), text(permission_text(p)).size(12.5).color(TEXT_BODY)].spacing(8).align_y(Alignment::Center));
            }
            for (l, v) in extra {
                col = col.push(line(l, v, false));
            }
            col = col.push(note("You can take it back any time."));
            (icons::shield(), format!("Allow {}?", host.name), "Allow", "Cancel", col.into())
        }
        Wait::Link(url) => {
            let site = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
            let col = column![line("Site", &site, false), line("Link", url, true), note("It opens in your browser, outside Accordo.")].spacing(8);
            (icons::triangle_alert(), "Open a link?".to_string(), "Open", "Cancel", col.into())
        }
        _ => return None,
    };
    let card = container(
        column![
            header(host),
            container(space().width(Fill).height(1)).style(crate::theme::rule),
            row![icon(icon_handle, 16, INDIGO_SOFT), text(title).size(15).font(BOLD).color(Color::WHITE)].spacing(8).align_y(Alignment::Center),
            text(host.asked_by()).size(11).color(FAINT),
            body,
            row![
                space().width(Fill),
                button(text(cancel).size(13)).padding([8, 16]).style(tone_button(Tone::Dark)).on_press(msg(ModuleWinMsg::Cancel)),
                button(text(confirm).size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press(msg(ModuleWinMsg::Confirm)),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
    .width(460)
    .padding(18)
    .style(boxed(hex(0x14151c), rgba(0xffffff, 0.08), 14.0));
    Some(super::overlay::modal(card, msg(ModuleWinMsg::Cancel)))
}
