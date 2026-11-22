// SPDX-License-Identifier: GPL-3.0-or-later
//! An invoice sent in a chat: the card with its amount, who bills whom and its state — drawn by
//! `invoice_card.rs`.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.is_object())?;
    let ctx = super::super::invoice_card::CardCtx { invoices: feed.invoices, explorers: feed.explorers, me: feed.me_name, mine: m.sender_id == Some(feed.rights.me) };
    Some(super::super::invoice_card::view(data, &ctx))
}
