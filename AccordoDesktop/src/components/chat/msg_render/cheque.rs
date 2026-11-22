// SPDX-License-Identifier: GPL-3.0-or-later
//! A cheque sent in a chat: the card with its amount, who it's for and its state — drawn by
//! `cheque_card.rs`, which needs the viewer's wallets and the loaded cheque state.

use super::super::message_list::Feed;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let data = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.is_object())?;
    let ctx = super::super::cheque_card::CardCtx { cheques: feed.cheques, explorers: feed.explorers, wallets: feed.wallets };
    Some(super::super::cheque_card::view(data, &ctx))
}
