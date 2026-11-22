// SPDX-License-Identifier: GPL-3.0-or-later
//! A token drop shared in a chat: the card with its slots and the claim button, checked against the
//! viewer's wallets — drawn by `fund_cards.rs`; this file is the type's entry point.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.is_object())?;
    Some(super::super::fund_cards::drop(data, feed.fund, feed.wallets))
}
