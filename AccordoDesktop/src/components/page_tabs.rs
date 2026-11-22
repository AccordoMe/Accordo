// SPDX-License-Identifier: GPL-3.0-or-later
//! Tabs at the top of a page ("Write & Claim | Incoming Cheques 0 | Outgoing Cheques 0").

use iced::widget::{button, column, container, row, space, svg, text, Row};
use iced::{Alignment, Color, Fill};

use super::ui::{icon, El};
use crate::app::Message;
use crate::theme::{self, *};

pub struct Tab<'a> {
    pub icon: svg::Handle,
    pub label: &'a str,
    /// Shown as a small counter after the label (None: no counter).
    pub count: Option<usize>,
    pub active: bool,
    pub on_press: Message,
}

pub fn view<'a>(tabs: Vec<Tab<'a>>) -> El<'a> {
    let mut line: Row<'a, Message> = row![].spacing(8);
    for tab in tabs {
        let fg = if tab.active { Color::WHITE } else { hex(0xc7cbd6) };
        let mut content = row![icon(tab.icon, 14, fg), text(tab.label).size(13).font(BOLD)].spacing(8).align_y(Alignment::Center);
        if let Some(n) = tab.count {
            content = content.push(
                container(text(n.to_string()).size(10).font(BOLD).color(Color::WHITE))
                    .padding([1, 6])
                    .style(badge(if tab.active { rgba(0xffffff, 0.25) } else { rgba(0x6366f1, 0.35) }, Color::TRANSPARENT)),
            );
        }
        line = line.push(button(content).padding([9, 18]).style(page_tab(tab.active)).on_press(tab.on_press));
    }
    column![line, space().height(4), container(space().width(Fill).height(1)).style(theme::rule)].spacing(8).into()
}
