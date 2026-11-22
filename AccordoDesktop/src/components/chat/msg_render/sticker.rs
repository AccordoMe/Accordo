// SPDX-License-Identifier: GPL-3.0-or-later
//! A sticker in the feed: at most 160×160, no click (like the web, a sticker is just there).

use super::super::message_list::Feed;
use super::picture;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    super::open_attachment(m)?;
    Some(picture::picture(m, feed, 160.0, 160.0, picture::hint(m), None))
}
