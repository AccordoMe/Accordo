// SPDX-License-Identifier: GPL-3.0-or-later
//! Voice messages and video notes in the feed (AudioMessagePlayer.tsx, VideoNotePlayer.tsx): a
//! voice message's ▶, waveform and time; a video note's round picture — its first frame
//! (`thumbnail`), then the video itself while it plays — with a ring for how far it got.
//! Playing them: crate::player (GStreamer).

use iced::mouse;
use iced::widget::canvas::{self, path::Arc, Path, Stroke};
use iced::widget::{button, center, column, container, image, row, space, stack, text, Row};
use iced::{Alignment, Color, Fill, Radians, Rectangle, Renderer, Theme};
use serde_json::Value;

use crate::api::chat::ChatMessageItem;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

/// The message playing now.
pub struct Playing {
    pub message_id: i64,
    pub started: std::time::Instant,
    /// Seconds, as the message says (recorded files carry no reliable length).
    pub duration: f32,
    /// A video note's latest frame.
    pub frame: Option<image::Handle>,
    /// Dropping it stops the player (its process with it).
    pub _stop: iced::task::Handle,
    /// The file written for the player (deleted when it stops).
    pub file: std::path::PathBuf,
}

impl Drop for Playing {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

impl Playing {
    pub fn elapsed(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// 0…1 of the way.
    pub fn progress(&self) -> f32 {
        if self.duration > 0.0 { (self.elapsed() / self.duration).clamp(0.0, 1.0) } else { 0.0 }
    }
}

fn duration_of(a: &Value) -> f32 {
    a.get("duration").and_then(Value::as_f64).unwrap_or(0.0) as f32
}

/// A voice message: ▶ / ■, the waveform filling as it plays, the time.
pub fn audio<'a>(m: &'a ChatMessageItem, a: &'a Value, playing: Option<&'a Playing>, mine: bool) -> El<'a> {
    let duration = duration_of(a);
    let progress = playing.map_or(0.0, Playing::progress);
    let accent = if mine { INDIGO } else { SKY };
    let toggle = button(center(icon(if playing.is_some() { icons::square() } else { icons::play() }, 15, Color::WHITE)))
        .width(38)
        .height(38)
        .padding(0)
        .style(move |_, status| button::Style {
            background: Some(if matches!(status, button::Status::Hovered) { Color { a: 0.85, ..accent } } else { accent }.into()),
            border: iced::Border { radius: 19.0.into(), ..Default::default() },
            ..Default::default()
        })
        .on_press(Message::Chat(ChatMsg::Play(m.id)));
    // The peaks the recorder measured (0–100), else a made-up shape like the web's.
    let peaks: Vec<f32> = match a.get("waveform").and_then(Value::as_array) {
        Some(w) if !w.is_empty() => w.iter().filter_map(Value::as_f64).map(|v| v as f32).collect(),
        _ => {
            let seed = a.get("file_name").and_then(Value::as_str).map_or(15, str::len) as f32 * 7.0;
            (0..28).map(|i| ((i as f32 + seed) * 0.45).sin() * 40.0 + (i as f32 * 0.9).cos() * 20.0 + 35.0).map(|v| v.clamp(15.0, 95.0)).collect()
        }
    };
    let n = peaks.len().max(1) as f32;
    let mut bars: Row<'a, Message> = row![].spacing(2).align_y(Alignment::Center).height(26);
    for (i, p) in peaks.iter().enumerate() {
        let played = playing.is_some() && (i as f32 + 0.5) / n <= progress;
        let h = (p / 100.0 * 22.0).round().max(4.0);
        let c = if played { accent } else { rgba(0xffffff, 0.22) };
        bars = bars.push(container(space().width(3).height(h)).style(move |_| iced::widget::container::Style {
            background: Some(c.into()),
            border: iced::Border { radius: 1.5.into(), ..Default::default() },
            ..Default::default()
        }));
    }
    let time = playing.map_or(duration, |p| p.elapsed().min(duration.max(p.elapsed())));
    container(
        row![
            toggle,
            column![bars, row![text(crate::player::clock(time)).size(11).font(iced::Font::MONOSPACE).color(MUTED), space().width(Fill), save(m.id)].align_y(Alignment::Center)].spacing(4).width(Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(300)
    .padding([10, 12])
    .style(boxed(rgba(0xffffff, 0.04), rgba(0xffffff, 0.08), 12.0))
    .into()
}

fn save<'a>(id: i64) -> El<'a> {
    iced::widget::tooltip(
        button(icon(icons::download(), 12, MUTED)).padding(3).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::SaveAttachment(id))),
        container(text("Save").size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

/// How far a video note got, round its edge.
struct Ring {
    progress: f32,
}

impl canvas::Program<Message> for Ring {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let r = bounds.width.min(bounds.height) / 2.0 - 2.0;
        if self.progress > 0.0 {
            let start = -std::f32::consts::FRAC_PI_2;
            let arc = Path::new(|b| b.arc(Arc { center, radius: r, start_angle: Radians(start), end_angle: Radians(start + self.progress * std::f32::consts::TAU) }));
            frame.stroke(&arc, Stroke::default().with_width(3.0).with_color(hex(0x818cf8)).with_line_cap(canvas::LineCap::Round));
        }
        vec![frame.into_geometry()]
    }
}

const NOTE: f32 = 240.0;

/// A video note: the round picture, ▶ in the middle until it plays, the time at the bottom.
pub fn video<'a>(m: &'a ChatMessageItem, a: &'a Value, playing: Option<&'a Playing>) -> El<'a> {
    let duration = duration_of(a);
    let picture: El = match playing.and_then(|p| p.frame.clone()) {
        Some(f) => image(f).width(NOTE).height(NOTE).border_radius(NOTE / 2.0).into(),
        None => {
            let thumb = a.get("thumbnail").and_then(Value::as_str).filter(|t| t.starts_with("data:image/"));
            match thumb {
                Some(t) => crate::components::ui::avatar_pic("", Some(t), NOTE, Some(hex(0x0b0c12).into())),
                None => container(icon(icons::video(), 30, FAINT)).width(NOTE).height(NOTE).center(NOTE).style(round_fill(hex(0x0b0c12).into(), NOTE / 2.0)).into(),
            }
        }
    };
    let center_btn: El = if playing.is_some() {
        space().into()
    } else {
        center(container(icon(icons::play(), 22, Color::WHITE)).width(52).height(52).center(52).style(round_fill(rgba(0x000000, 0.55).into(), 26.0))).width(NOTE).height(NOTE).into()
    };
    let time = playing.map_or(duration, Playing::elapsed);
    let badge = container(container(text(crate::player::clock(time)).size(11).font(iced::Font::MONOSPACE).color(Color::WHITE)).padding([2, 8]).style(round_fill(rgba(0x000000, 0.6).into(), 9.0)))
        .width(NOTE)
        .height(NOTE)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(14);
    let ring = canvas::Canvas::new(Ring { progress: playing.map_or(0.0, Playing::progress) }).width(NOTE).height(NOTE);
    let note = button(stack![picture, ring, center_btn, badge])
        .padding(0)
        .style(ghost(NOTE / 2.0, Color::TRANSPARENT))
        .on_press(Message::Chat(ChatMsg::Play(m.id)));
    row![note, column![space().height(Fill), save(m.id)].height(NOTE)].spacing(6).into()
}
