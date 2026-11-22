// SPDX-License-Identifier: GPL-3.0-or-later
//! The platform fee: presets, a slider and the exact value (FeePicker.tsx). In basis points,
//! 10–500 (0.1 %–5 %); every form starts at "Supporter", 1.5 %.

use iced::widget::{button, column, container, row, slider, space, text, Row};
use iced::{Alignment, Fill, Font};

use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

pub const DEFAULT_BPS: u32 = 150;
const MIN_BPS: u32 = 10;
const MAX_BPS: u32 = 500;
const PRESETS: [(u32, &str); 6] = [(10, "Min"), (50, "Low"), (100, "Medium"), (150, "Supporter"), (250, "Sponsor"), (500, "Patron")];

fn percent(bps: u32) -> String {
    let p = bps as f64 / 100.0;
    if bps % 100 == 0 { format!("{p:.0}") } else if bps % 10 == 0 { format!("{p:.1}") } else { format!("{p:.2}") }
}

/// `hint`: an explanation under the title (crowdfunding says where the fee goes).
pub fn view<'a>(caption: &'a str, bps: u32, hint: Option<&'a str>, amber: bool, on_change: impl Fn(u32) -> Message + Clone + 'a) -> El<'a> {
    let tone = if amber { AMBER_TEXT } else { INDIGO_SOFT };
    let mut chips: Row<'a, Message> = row![].spacing(6);
    for (value, name) in PRESETS {
        chips = chips.push(
            button(text(format!("{}% {name}", percent(value))).size(11).font(if bps == value { BOLD } else { INTER }))
                .padding([3, 8])
                .style(chip(bps == value))
                .on_press(on_change(value)),
        );
    }
    let mut body = column![row![
        icon(icons::percent(), 12, tone),
        text(caption).size(12).font(SEMIBOLD).color(if amber { AMBER_TEXT } else { LABEL }),
        space().width(Fill),
        text(format!("{}% ({bps} bps)", percent(bps))).size(11.5).font(Font::MONOSPACE).color(if amber { AMBER_TEXT } else { hex(0x38bdf8) }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)]
    .spacing(8);
    if let Some(hint) = hint {
        body = body.push(text(hint).size(11.5).color(TEXT_SECONDARY).line_height(1.45));
    }
    let on_slide = on_change.clone();
    body = body.push(chips).push(
        row![
            slider(MIN_BPS as f64..=MAX_BPS as f64, bps as f64, move |v| on_slide(v.round() as u32)).step(10.0).style(fee_slider),
            container(text(percent(bps)).size(11.5).font(Font::MONOSPACE).color(TEXT_BODY)).padding([3, 10]).style(boxed(hex(0x0e0f17), BORDER, 5.0)),
            text("%").size(11).color(TEXT_SECONDARY),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    );
    container(body).width(Fill).padding([12, 12]).style(if amber { notice } else { inset }).into()
}
