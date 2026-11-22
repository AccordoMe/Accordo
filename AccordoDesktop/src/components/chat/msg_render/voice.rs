// SPDX-License-Identifier: GPL-3.0-or-later
//! A voice message: ▶ / ■, the waveform the recorder measured, the time. The player (and the
//! GStreamer behind it) is `media_cards::audio`; this file is the type's entry point — what the
//! message must carry for the card to appear at all.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let a = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.get("file_data").and_then(|d| d.as_str()).is_some())?;
    let playing = feed.playing.filter(|p| p.message_id == m.id);
    Some(super::super::media_cards::audio(m, a, playing, m.sender_id == Some(feed.rights.me)))
}
