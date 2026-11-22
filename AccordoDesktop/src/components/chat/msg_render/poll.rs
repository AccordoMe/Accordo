// SPDX-License-Identifier: GPL-3.0-or-later
//! A poll in a chat: the question, its options and the votes — drawn by `poll_card.rs`; this file
//! is the type's entry point (which attachment shape it accepts).

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.get("type").and_then(|t| t.as_str()) == Some("poll"))?;
    let content: crate::api::chat::polls::PollContent = serde_json::from_value(data.clone()).ok()?;
    Some(super::super::poll_card::poll_card(m.id, content, feed.polls.and_then(|p| p.get(&m.id)), feed.rights.me, feed.members, feed.private))
}
