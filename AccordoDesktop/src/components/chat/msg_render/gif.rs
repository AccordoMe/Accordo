// SPDX-License-Identifier: GPL-3.0-or-later
//! A GIF in the feed: at most 320×260, animated by the clock (no click — GIFs play in place).

use super::super::message_list::Feed;
use super::picture;
use crate::api::chat::ChatMessageItem;
use crate::components::ui::El;

pub fn card<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    super::open_attachment(m)?;
    Some(picture::picture(m, feed, 320.0, 260.0, picture::hint(m), None))
}
