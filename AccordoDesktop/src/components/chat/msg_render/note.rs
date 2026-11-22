// SPDX-License-Identifier: GPL-3.0-or-later
//! A video note (the round one): its first frame as the picture, then the video while it plays.
//! `media_cards::video` draws it; this file is the type's entry point.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let a = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.get("file_data").and_then(|d| d.as_str()).is_some())?;
    let playing = feed.playing.filter(|p| p.message_id == m.id);
    Some(super::super::media_cards::video(m, a, playing))
}
