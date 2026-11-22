// SPDX-License-Identifier: GPL-3.0-or-later
//! A picture in the feed: at most 380×300, keeping its shape, a click opens it full size.

use super::super::message_list::Feed;
use super::picture;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;
use crate::pages::chat_page::ChatMsg;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    super::open_attachment(m)?;
    Some(picture::picture(m, feed, 380.0, 300.0, (0.0, 0.0), Some(ChatMsg::OpenImage(m.id))))
}
