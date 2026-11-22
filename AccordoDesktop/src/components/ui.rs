// SPDX-License-Identifier: GPL-3.0-or-later
//! Small building blocks used everywhere (components/ui, AddressIdenticon, avatar.ts).

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{container, space, svg, text};
use iced::{Background, Color, Element, Fill};

use crate::app::Message;
use crate::theme::{self, *};

pub type El<'a> = Element<'a, Message>;

pub fn icon<'a>(handle: svg::Handle, size: u32, color: Color) -> svg::Svg<'a> {
    svg(handle).width(size).height(size).style(tint(color))
}

pub fn hline<'a>() -> El<'a> {
    container(space().width(Fill).height(1)).style(theme::rule).into()
}

pub fn vline<'a>() -> El<'a> {
    container(space().width(1).height(Fill)).style(theme::rule).into()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end())
    }
}

/// Round avatar with the initial, coloured like getAvatarGradient() unless `bg` is given.
pub fn avatar<'a>(name: &str, size: f32, bg: Option<Background>) -> El<'a> {
    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    container(text(initial).size(size * 0.42).font(BOLD).color(Color::WHITE))
        .width(size)
        .height(size)
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .style(round_fill(bg.unwrap_or_else(|| avatar_gradient(name)), size / 2.0))
        .into()
}

/// The avatar picture when `src` has one (and it's loaded), else the initial — the web's
/// `avatar ? <img> : <initial>`.
pub fn avatar_pic<'a>(name: &str, src: Option<&str>, size: f32, bg: Option<Background>) -> El<'a> {
    match crate::avatars::get(src) {
        Some(crate::avatars::Pic::Raster(h)) => iced::widget::image(h).width(size).height(size).content_fit(iced::ContentFit::Cover).border_radius(size / 2.0).into(),
        Some(crate::avatars::Pic::Vector(h)) => container(iced::widget::svg(h).width(size * 0.86).height(size * 0.86))
            .width(size)
            .height(size)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(round_fill(rgba(0xffffff, 0.06).into(), size / 2.0))
            .into(),
        None => avatar(name, size, bg),
    }
}

/// One of someone's organizations: logo, name, the verified check, their title there.
pub fn org_chip<'a>(o: &crate::api::orgs::OrgBrief) -> El<'a> {
    let mut r = iced::widget::row![avatar_pic(&o.name, o.logo.as_deref(), 18.0, None), text(o.name.clone()).size(12.5).font(BOLD).color(Color::WHITE)].spacing(6).align_y(iced::Alignment::Center);
    if o.verified {
        r = r.push(iced::widget::tooltip(
            icon(crate::icons::circle_check(), 13, hex(0x60a5fa)),
            container(text("Verified by Accordo").size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
            iced::widget::tooltip::Position::Top,
        ));
    }
    if !o.title.is_empty() {
        r = r.push(text(format!("· {}", o.title)).size(12).color(TEXT_SECONDARY));
    }
    r.into()
}

/// Pill with a number (unread counters in the chat list).
/// The server stops counting unread at 100 — shown as "99+".
pub fn count_badge<'a>(count: u32, bg: Color) -> El<'a> {
    container(text(if count > 99 { "99+".to_string() } else { count.to_string() }).size(11).font(BOLD).color(Color::WHITE))
        .padding([1, 6])
        .style(round_fill(bg.into(), 10.0))
        .into()
}

/// Small red counter on a rail icon.
pub fn dot_badge<'a>(count: u32) -> El<'a> {
    let label = if count > 99 { "99+".to_string() } else { count.to_string() };
    container(text(label).size(9).font(BOLD).color(Color::WHITE))
        .padding([0, 4])
        .style(round_fill(RED.into(), 8.0))
        .into()
}

/// Emoji code points (pictographs, symbols, joiners, variation selectors, flags, keycaps).
pub fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF | 0xFE0F | 0x200D | 0x20E3 | 0xE0020..=0xE007F)
}

/// Text whose emoji are drawn with the colour emoji font (the fallback would pick a monochrome
/// one): runs of emoji become their own spans.
pub fn with_emoji<'a>(content: &str, size: f32, color: Color, font: iced::Font) -> El<'a> {
    if !content.chars().any(is_emoji) {
        return text(content.to_string()).size(size).color(color).font(font).line_height(1.45).into();
    }
    let mut spans: Vec<iced::widget::text::Span<'a, ()>> = Vec::new();
    let mut run = String::new();
    let mut run_emoji = false;
    let flush = |run: &mut String, emoji: bool, spans: &mut Vec<iced::widget::text::Span<'a, ()>>| {
        if !run.is_empty() {
            let s = iced::widget::span(std::mem::take(run)).size(size).color(color).font(if emoji { EMOJI } else { font });
            spans.push(s);
        }
    };
    for c in content.chars() {
        // Digits and '#' only join an emoji run when a keycap follows; keep it simple: by class.
        let e = is_emoji(c);
        if e != run_emoji {
            flush(&mut run, run_emoji, &mut spans);
            run_emoji = e;
        }
        run.push(c);
    }
    flush(&mut run, run_emoji, &mut spans);
    iced::widget::rich_text(spans).line_height(1.45).into()
}
