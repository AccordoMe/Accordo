// SPDX-License-Identifier: GPL-3.0-or-later
//! A random draw in a chat: who was picked, and its title — drawn by `poll_card.rs`; this file is
//! the type's entry point (the draw lives in the message's attachment, its title in the text).

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("type").and_then(|t| t.as_str()) == Some("random_draw"))?;
    Some(super::super::poll_card::draw_card(data, &m.content, feed.rights.me, feed.members))
}
