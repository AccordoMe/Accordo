// SPDX-License-Identifier: GPL-3.0-or-later
//! What a recording looks like while it runs: the mic's red bar in place of the message box
//! (AudioRecorderBar.tsx), and the camera's round window over the chat (VideoNoteRecorderModal.tsx).
//! The recording itself — the pipeline, the levels, the file — is crate::recorder.

use iced::widget::{button, center, column, container, image, row, space, text};
use iced::{Alignment, Color, Fill, Font};

use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::{ChatMsg, Recording, LIVE_BARS};
use crate::theme::*;

/// The round window's picture's side (the web's 240 px).
const NOTE: f32 = 240.0;

fn msg(m: ChatMsg) -> Message {
    Message::Chat(m)
}

/// "0:07": how long it has been recording (the web's formatTimer).
fn elapsed(r: &Recording) -> String {
    crate::player::clock(r.elapsed() as f32)
}

fn tip<'a>(label: &'a str) -> El<'a> {
    container(text(label).size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x1c1d22), BORDER, 6.0)).into()
}

/// The trash: throws the recording away, nothing is sent.
fn cancel_button<'a>() -> El<'a> {
    iced::widget::tooltip(
        button(center(icon(icons::trash_2(), 16, MUTED)))
            .width(34)
            .height(34)
            .padding(0)
            .style(ghost(17.0, rgba(0xffffff, 0.10)))
            .on_press(msg(ChatMsg::RecordCancel)),
        tip("Cancel"),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

/// The red one: stop, and the recording goes out as the message.
fn finish_button<'a>(side: f32) -> El<'a> {
    let red = hex(0xef4444);
    iced::widget::tooltip(
        button(center(icon(icons::send(), 16, Color::WHITE)))
            .width(side)
            .height(side)
            .padding(0)
            .style(move |_, status| button::Style {
                background: Some(if matches!(status, button::Status::Hovered) { hex(0xf87171) } else { red }.into()),
                border: iced::Border { radius: (side / 2.0).into(), ..Default::default() },
                shadow: iced::Shadow { color: Color { a: 0.4, ..red }, offset: iced::Vector::new(0.0, 2.0), blur_radius: 10.0 },
                text_color: Color::WHITE,
                ..Default::default()
            })
            .on_press(msg(ChatMsg::RecordStop)),
        tip("Finish & send"),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

/// The last levels, right to left: the bar's live waveform.
fn live_bars<'a>(r: &'a Recording) -> El<'a> {
    let mut bars = row![].spacing(2).align_y(Alignment::Center).height(20);
    let from = r.bars.len().saturating_sub(LIVE_BARS);
    for level in &r.bars[from..] {
        let h = (level / 100.0 * 20.0).round().max(3.0);
        bars = bars.push(container(space().width(3).height(h)).style(|_| container::Style {
            background: Some(hex(0xf87171).into()),
            border: iced::Border { radius: 1.5.into(), ..Default::default() },
            ..Default::default()
        }));
    }
    container(bars).width(Fill).height(20).align_y(iced::alignment::Vertical::Center).into()
}

/// A voice message being recorded, in the message box's place: the light, the time, the live
/// bars, the trash and the red send (AudioRecorderBar.tsx).
pub fn bar<'a>(r: &'a Recording) -> El<'a> {
    let left = r.limit().saturating_sub(r.elapsed());
    container(
        row![
            container(space()).width(10).height(10).style(round_fill(hex(0xef4444).into(), 5.0)),
            text(elapsed(r)).size(13).font(Font::MONOSPACE).color(hex(0xf87171)),
            live_bars(r),
            text(format!("{}:{:02} left", left / 60, left % 60)).size(11).color(FAINT),
            cancel_button(),
            finish_button(34.0),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([4, 6])
    .style(|_| container::Style {
        background: Some(rgba(0xef4444, 0.08).into()),
        border: iced::Border { color: rgba(0xef4444, 0.25), width: 1.0, radius: 12.0.into() },
        ..Default::default()
    })
    .into()
}

/// A video note being recorded, over the chat: the round picture the camera sees (mirrored, as
/// the web mirrors a front camera), the time on it, the trash and the red send.
pub fn modal(r: &Recording) -> El<'_> {
    let picture: El = match &r.frame {
        Some(f) => image(f.clone()).width(NOTE).height(NOTE).border_radius(NOTE / 2.0).into(),
        None => container(center(icon(icons::camera(), 40, FAINT))).width(NOTE).height(NOTE).style(round_fill(hex(0x090a10).into(), NOTE / 2.0)).into(),
    };
    let time = container(text(elapsed(r)).size(12).font(Font::MONOSPACE).color(Color::WHITE)).padding([2, 8]).style(round_fill(rgba(0x000000, 0.6).into(), 9.0));
    let window: El = iced::widget::Stack::with_children(vec![
        picture,
        container(time)
            .width(NOTE)
            .height(NOTE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(12)
            .into(),
    ])
    .into();
    // The web's rainbow ring, in one of its colours.
    let ring = container(window).padding(6).style(|_| container::Style {
        border: iced::Border { color: rgba(0x6366f1, 0.75), width: 2.0, radius: ((NOTE + 16.0) / 2.0).into() },
        ..Default::default()
    });
    let header = row![
        icon(icons::camera(), 18, SKY),
        text("Video Message").size(14).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(ChatMsg::RecordCancel)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let card = container(
        column![
            header,
            column![
                ring,
                text(format!("Recording up to {} — Esc cancels", crate::player::clock(r.limit() as f32))).size(11.5).color(MUTED),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
            row![cancel_button(), finish_button(38.0)].spacing(10).align_y(Alignment::Center),
        ]
        .spacing(16)
        .align_x(Alignment::Center)
        .width(Fill),
    )
    .width(360)
    .padding(22)
    .style(boxed(hex(0x141622), rgba(0xffffff, 0.12), 24.0));
    overlay::modal(card, msg(ChatMsg::RecordCancel))
}
