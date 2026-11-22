// SPDX-License-Identifier: GPL-3.0-or-later
//! Time in messages (ChatTimestamp.tsx): Discord-style tokens `<t:UNIX>` / `<t:UNIX:STYLE>`.
//! A message carries one instant; every reader sees it in their own timezone. Styles t T d D f F R
//! like Discord, plus `C`: a live countdown card. All drawn here — no web view.

use chrono::{DateTime, Local, TimeZone};
use iced::mouse;
use iced::widget::canvas::{self, path::Arc, Path, Stroke};
use iced::widget::{column, container, row, text, Column};
use iced::{Alignment, Color, Radians, Rectangle, Renderer, Theme};

use crate::app::Message;
use crate::components::ui::{is_emoji, El};
use crate::icons;
use crate::theme::*;

/// The largest |seconds| a token may carry (JS Date's range, as the web checks).
const MAX_ABS_SECONDS: i64 = 8_640_000_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Stamp(i64, char),
}

/// `<t:1763280000:C>` and friends, split out of the text.
pub fn parse(content: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find("<t:") {
        let tail = &rest[i + 3..];
        let digits = tail.char_indices().take_while(|(k, c)| c.is_ascii_digit() || (*k == 0 && *c == '-')).count();
        let (num, after) = tail.split_at(digits);
        let (style, used) = match after.as_bytes() {
            [b'>', ..] => ('f', 1),
            [b':', s, b'>', ..] if b"tTdDfFRC".contains(s) => (*s as char, 3),
            _ => ('?', 0),
        };
        match (num.parse::<i64>(), style) {
            (Ok(unix), s) if s != '?' && (1..=13).contains(&num.trim_start_matches('-').len()) && unix.abs() <= MAX_ABS_SECONDS => {
                if i > 0 {
                    out.push(Piece::Text(rest[..i].to_string()));
                }
                out.push(Piece::Stamp(unix, s));
                rest = &after[used..];
            }
            _ => {
                out.push(Piece::Text(rest[..i + 3].to_string()));
                rest = tail;
            }
        }
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    // Neighbouring text pieces back together.
    let mut merged: Vec<Piece> = Vec::new();
    for p in out {
        match (merged.last_mut(), p) {
            (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(&b),
            (_, p) => merged.push(p),
        }
    }
    merged
}

pub fn token(unix: i64, style: char) -> String {
    if style == 'f' { format!("<t:{unix}>") } else { format!("<t:{unix}:{style}>") }
}

fn local(unix: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(unix, 0).single()
}

/// "in 16 hours", "5 minutes ago", "now".
pub fn relative(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let diff = (at - now).num_seconds();
    const UNITS: [(&str, i64); 7] = [("year", 365 * 86400), ("month", 30 * 86400), ("week", 7 * 86400), ("day", 86400), ("hour", 3600), ("minute", 60), ("second", 1)];
    let (unit, secs) = UNITS.iter().find(|(_, s)| diff.abs() >= *s).copied().unwrap_or(("second", 1));
    let n = (diff as f64 / secs as f64).round() as i64;
    if n == 0 {
        return "now".into();
    }
    let word = if n.abs() == 1 { unit.to_string() } else { format!("{unit}s") };
    if n > 0 { format!("in {n} {word}") } else { format!("{} {word} ago", -n) }
}

pub fn format(unix: i64, style: char) -> String {
    let Some(t) = local(unix) else { return token(unix, style) };
    match style {
        't' => t.format("%H:%M").to_string(),
        'T' => t.format("%H:%M:%S").to_string(),
        'd' => t.format("%d/%m/%Y").to_string(),
        'D' => t.format("%B %-d, %Y").to_string(),
        'F' => t.format("%A, %B %-d, %Y at %H:%M").to_string(),
        'R' => relative(t, Local::now()),
        _ => t.format("%B %-d, %Y at %H:%M").to_string(),
    }
}

/// For places without cards (chat list, quotes, pins): every token as text.
pub fn to_plain(content: &str) -> String {
    if !content.contains("<t:") {
        return content.to_string();
    }
    parse(content)
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => t,
            Piece::Stamp(u, s) => format(u, if s == 'C' { 'f' } else { s }),
        })
        .collect()
}

/// Does it show something that changes by the second (relative times, countdowns)?
pub fn is_live(content: &str) -> bool {
    content.contains("<t:") && parse(content).iter().any(|p| matches!(p, Piece::Stamp(_, 'R' | 'C')))
}

fn spans_of<'a>(text_part: &str, size: f32, color: Color, out: &mut Vec<iced::widget::text::Span<'a, ()>>) {
    let mut run = String::new();
    let mut emoji = false;
    let flush = |run: &mut String, emoji: bool, out: &mut Vec<iced::widget::text::Span<'a, ()>>| {
        if !run.is_empty() {
            out.push(iced::widget::span(std::mem::take(run)).size(size).color(color).font(if emoji { EMOJI } else { INTER }));
        }
    };
    for c in text_part.chars() {
        let e = is_emoji(c);
        if e != emoji {
            flush(&mut run, emoji, out);
            emoji = e;
        }
        run.push(c);
    }
    flush(&mut run, emoji, out);
}

/// A message body: text (emoji in colour), inline time chips, and countdown cards below.
pub fn body<'a>(content: &str, size: f32, color: Color, sent_at: &str) -> El<'a> {
    let mut spans: Vec<iced::widget::text::Span<'a, ()>> = Vec::new();
    // The text can be selected and copied (select_text), as in the web.
    let selectable = |spans| crate::components::select_text::view(spans, size, 1.45);
    if !content.contains("<t:") {
        spans_of(content, size, color, &mut spans);
        return selectable(spans);
    }
    let mut cards: Vec<El<'a>> = Vec::new();
    for p in parse(content) {
        match p {
            Piece::Text(t) => spans_of(&t, size, color, &mut spans),
            Piece::Stamp(u, 'C') => cards.push(countdown(u, sent_at)),
            Piece::Stamp(u, s) => spans.push(
                iced::widget::span(format(u, s))
                    .size(size)
                    .color(hex(0xc7d2fe))
                    .background(rgba(0x6366f1, 0.18))
                    .border(iced::Border { radius: 4.0.into(), ..Default::default() })
                    .padding([0, 4]),
            ),
        }
    }
    let mut col: Column<'a, Message> = Column::new().spacing(6);
    if spans.iter().any(|s| !s.text.trim().is_empty()) {
        col = col.push(selectable(spans));
    }
    for c in cards {
        col = col.push(c);
    }
    col.into()
}

/// The ring of a countdown: what's left of the whole interval, draining to empty.
struct Ring {
    sweep: f32,
    color: Color,
}

impl canvas::Program<Message> for Ring {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let r = bounds.width.min(bounds.height) / 2.0 - 4.0;
        frame.stroke(&Path::circle(center, r), Stroke::default().with_width(5.0).with_color(rgba(0xffffff, 0.08)));
        if self.sweep > 0.0 {
            let start = -std::f32::consts::FRAC_PI_2;
            let arc = Path::new(|b| b.arc(Arc { center, radius: r, start_angle: Radians(start), end_angle: Radians(start + self.sweep * std::f32::consts::TAU) }));
            frame.stroke(&arc, Stroke::default().with_width(5.0).with_color(self.color).with_line_cap(canvas::LineCap::Round));
        }
        vec![frame.into_geometry()]
    }
}

fn countdown<'a>(unix: i64, sent_at: &str) -> El<'a> {
    let now = Local::now();
    let target = local(unix).unwrap_or(now);
    let start = super::display::local(sent_at).filter(|s| *s < target).unwrap_or(now);
    let left = (target - now).num_seconds().max(0);
    let done = left == 0;
    let sweep = if done || target <= start { 0.0 } else { ((target - now).num_milliseconds() as f32 / (target - start).num_milliseconds() as f32).clamp(0.0, 1.0) };
    let accent = if done { hex(0x22c55e) } else { AMBER };
    let days = left / 86400;
    let hms = format!("{:02}:{:02}:{:02}", (left % 86400) / 3600, (left % 3600) / 60, left % 60);
    let mut clock = row![].spacing(0).align_y(Alignment::Center);
    if days > 0 && !done {
        clock = clock.push(text(format!("{days}d ")).size(26).font(BOLD).color(hex(0xfcd34d)));
    }
    clock = clock.push(text(if done { "00:00:00".to_string() } else { hms }).size(26).font(BOLD).color(Color::WHITE));
    let ring = iced::widget::stack![
        canvas::Canvas::new(Ring { sweep, color: accent }).width(64).height(64),
        container(crate::components::ui::icon(icons::timer(), 18, accent)).width(64).height(64).align_x(iced::alignment::Horizontal::Center).align_y(iced::alignment::Vertical::Center),
    ];
    container(
        row![
            ring,
            column![
                text(if done { "TIME IS UP" } else { "COUNTDOWN" }).size(11).font(BOLD).color(MUTED),
                clock,
                text(format!("{} · {}", format(unix, 'F'), relative(target, now))).size(12).color(INDIGO_SOFT),
            ]
            .spacing(1),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding([12, 16])
    .style(boxed(rgba(0xffffff, 0.04), rgba(0xffffff, 0.08), 12.0))
    .into()
}

/// The token styles, as the time window offers them.
pub const STYLES: [(char, &str); 7] = [('C', "Countdown card"), ('R', "Relative"), ('f', "Date & time"), ('F', "Full"), ('t', "Time"), ('D', "Date"), ('d', "Short date")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tokens() {
        assert_eq!(parse("meet <t:1763280000:C> ok"), vec![Piece::Text("meet ".into()), Piece::Stamp(1763280000, 'C'), Piece::Text(" ok".into())]);
        assert_eq!(parse("<t:5>"), vec![Piece::Stamp(5, 'f')]);
        assert_eq!(parse("<t:abc> <t:5:X>"), vec![Piece::Text("<t:abc> <t:5:X>".into())]);
        assert!(is_live("<t:5:R>") && !is_live("<t:5:t>"));
        assert_eq!(token(7, 'f'), "<t:7>");
        assert_eq!(token(7, 'C'), "<t:7:C>");
    }
}
