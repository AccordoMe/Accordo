// SPDX-License-Identifier: GPL-3.0-or-later
//! A crowdfunding campaign shared in a chat: the live card with its progress and the button that
//! opens it — drawn by `fund_cards.rs`; this file is the type's entry point.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.is_object())?;
    Some(super::super::fund_cards::campaign(data, feed.fund, feed.me_name))
}
