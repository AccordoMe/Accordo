// SPDX-License-Identifier: GPL-3.0-or-later
//! What a message shows under its text, by type: a picture, a file, a voice message, a video
//! note, a poll or a draw, a cheque, an invoice, a campaign or a drop.
//!
//! One file per type — `attachment()` below is the map from `message_type` to the file that draws
//! it, and each of those files is free to grow without touching the others. The feed itself
//! (grouping by author, avatars, the hover bar, menus) is `message_list.rs`; the cards that live
//! in their own modules stay there (`cheque_card.rs`, `invoice_card.rs`, `poll_card.rs`,
//! `fund_cards.rs`, `media_cards.rs` for the players) — the files here hold the type's entry
//! point: the checks its data needs, the sizes, the context the card wants.

use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;
use super::message_list::Feed;

pub mod campaign;
pub mod cheque;
pub mod draw;
pub mod drop;
pub mod file;
pub mod gif;
pub mod image;
pub mod invoice;
pub mod note;
pub mod picture;
pub mod poll;
pub mod reply;
pub mod sticker;
pub mod voice;

/// What to draw under a message's text — or instead of it: the message's type decides.
/// `None` when there's nothing to draw (no attachment, or it's sealed: the text says so).
pub fn attachment<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    match m.message_type.as_str() {
        "poll" => poll::card(m, feed),
        "random_draw" => draw::card(m, feed),
        "cheque" => cheque::card(m, feed),
        "invoice" => invoice::card(m, feed),
        "campaign" => campaign::card(m, feed),
        "drop" => drop::card(m, feed),
        "audio" => voice::card(m, feed).or_else(|| file::card(m, feed)),
        "video" => note::card(m, feed).or_else(|| file::card(m, feed)),
        "image" => image::card(m, feed),
        "gif" => gif::card(m, feed),
        "sticker" => sticker::card(m, feed),
        "file" => file::card(m, feed),
        // Some servers send the file without a type: the MIME says what it is. A picture stays a
        // picture (the web does the same), so `image` is checked by the type above.
        _ if super::display::is_media(m) => {
            let mime = mime(m);
            if m.message_type != "image" && mime.starts_with("video/") {
                note::card(m, feed)
            } else if m.message_type != "image" && mime.starts_with("audio/") {
                voice::card(m, feed)
            } else {
                file::card(m, feed)
            }
        }
        // Anything else (an article, a tip, a transfer…) has its text, or its own card elsewhere.
        _ => None,
    }
}

/// The attachment's MIME type ("" when there is none or it's sealed).
pub fn mime(m: &ChatMessageItem) -> String {
    m.attachment_data
        .as_ref()
        .and_then(|a| a.get("file_type"))
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string()
}

/// The attachment, if it's here in the clear (a sealed one can't be drawn).
pub fn open_attachment(m: &ChatMessageItem) -> Option<&serde_json::Value> {
    m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none())
}
