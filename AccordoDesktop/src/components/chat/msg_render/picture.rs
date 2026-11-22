// SPDX-License-Identifier: GPL-3.0-or-later
//! The box a picture, a GIF or a sticker sits in: its shape kept inside the limits, the frames the
//! decoder has so far (a GIF animates on the clock), a click to open it full size.
//!
//! Shared by `image.rs`, `gif.rs` and `sticker.rs` — the three types differ only in the limits and
//! whether a click opens them.

use iced::widget::{button, center, column, container, image, space, text};
use iced::{Alignment, Color};

use super::super::message_list::Feed;
use super::open_attachment;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

/// Fits (w, h) into a box, keeping the shape.
pub fn fit(w: f32, h: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    if w <= 0.0 || h <= 0.0 {
        return (max_w.min(240.0), max_h.min(180.0));
    }
    let k = (max_w / w).min(max_h / h).min(1.0);
    (w * k, h * k)
}

/// A message's picture at most `max_w`×`max_h`, clicked → `on` (opening the lightbox, or nothing).
/// `hint` is the size the sender announced, for before the decoder has the frames.
pub fn picture<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>, max_w: f32, max_h: f32, hint: (f32, f32), on: Option<ChatMsg>) -> El<'a> {
    let key = super::super::display::media_key(m);
    let frames = key.as_deref().and_then(|k| feed.media.frames(k));
    let (w, h) = match frames {
        Some(f) => fit(f.width as f32, f.height as f32, max_w, max_h),
        None => fit(hint.0, hint.1, max_w, max_h),
    };
    let inner: El = match (frames, key.as_deref().and_then(|k| feed.media.get(k))) {
        (Some(f), _) => image(f.at(feed.clock).clone()).width(w).height(h).into(),
        (None, Some(crate::media::Media::Failed)) => {
            center(column![icon(icons::image(), 18, FAINT), text("Couldn't load").size(11).color(FAINT)].spacing(4).align_x(Alignment::Center)).width(w).height(h).into()
        }
        _ => space().width(w).height(h).into(),
    };
    let boxed = container(inner).width(w).height(h).clip(true).style(boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 8.0));
    match on {
        Some(on) => button(boxed).padding(0).style(ghost(8.0, Color::TRANSPARENT)).on_press(crate::app::Message::Chat(on)).into(),
        None => boxed.into(),
    }
}

/// The width/height the sender announced in the attachment (0 when it didn't).
pub fn hint(m: &ChatMessageItem) -> (f32, f32) {
    let num = |k: &str| open_attachment(m).and_then(|a| a.get(k)).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    (num("width"), num("height"))
}
