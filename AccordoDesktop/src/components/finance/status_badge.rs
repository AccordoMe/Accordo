// SPDX-License-Identifier: GPL-3.0-or-later
//! Status pills (finance/StatusBadge.tsx): one meaning per colour everywhere — done green,
//! in progress blue, waiting on someone amber, needs attention red, closed grey.

use iced::widget::{container, row, space, text};
use iced::{Alignment, Color};

use crate::components::ui::El;
use crate::theme::*;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Tone {
    Done,
    Live,
    Waiting,
    Alert,
    Closed,
}

fn colours(t: Tone) -> (Color, Color, Color, Color) {
    // text, background, border, dot
    match t {
        Tone::Done => (hex(0x34d399), rgba(0x10b981, 0.14), rgba(0x10b981, 0.35), hex(0x10b981)),
        Tone::Live => (hex(0x7dd3fc), rgba(0x38bdf8, 0.14), rgba(0x38bdf8, 0.35), hex(0x38bdf8)),
        Tone::Waiting => (hex(0xfbbf24), rgba(0xf59e0b, 0.14), rgba(0xf59e0b, 0.35), hex(0xf59e0b)),
        Tone::Alert => (hex(0xfca5a5), rgba(0xef4444, 0.14), rgba(0xef4444, 0.38), hex(0xef4444)),
        Tone::Closed => (hex(0xcbd5e1), rgba(0x94a3b8, 0.12), rgba(0x94a3b8, 0.3), hex(0x94a3b8)),
    }
}

/// A cheque's status as people read it.
pub fn cheque(status: &str) -> (&'static str, Tone) {
    match status.to_lowercase().as_str() {
        "partially_claimed" => ("Partly claimed", Tone::Live),
        "claimed" => ("Claimed", Tone::Done),
        "refunded" => ("Refunded", Tone::Closed),
        "expired" => ("Expired", Tone::Closed),
        _ => ("Unclaimed", Tone::Waiting),
    }
}

/// An invoice's status as people read it (the invoice card comes next).
#[allow(dead_code)]
pub fn invoice(status: &str) -> (&'static str, Tone) {
    match status.to_lowercase().as_str() {
        "partially_paid" => ("Partly paid", Tone::Live),
        "overdue" => ("Overdue", Tone::Alert),
        "paid" => ("Paid", Tone::Done),
        "cancelled" | "canceled" => ("Cancelled", Tone::Closed),
        "expired" => ("Expired", Tone::Closed),
        _ => ("Unpaid", Tone::Waiting),
    }
}

pub fn view<'a>((label, tone): (&'static str, Tone)) -> El<'a> {
    let (fg, bg, line, dot) = colours(tone);
    container(
        row![container(space().width(6).height(6)).style(move |_| container::Style { background: Some(dot.into()), border: border(Color::TRANSPARENT, 0.0, 3.0), ..Default::default() }), text(label).size(11).font(BOLD).color(fg)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([3, 9])
    .style(boxed(bg, line, 10.0))
    .into()
}
