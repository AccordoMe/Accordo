// SPDX-License-Identifier: GPL-3.0-or-later
//! How rooms and messages read on screen (utils/roomDisplay.ts and the chat list / feed text):
//! a DM is the other person; bodies that stay encrypted (no key here) show a lock line.

use chrono::{DateTime, Datelike, Local};

use crate::api::chat::{ChatMessageItem, ChatRoomItem, UserInfo};

/// The lock line for bodies this computer can't open (the web's KEY_UNAVAILABLE_TEXT).
pub const ENCRYPTED_TEXT: &str = "🔒 Encrypted message — key unavailable on this device";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomKind {
    /// Shown to you through its view rules, not joined yet: a group's showcase card (a teaser) or
    /// a public room you can read (ChatPage.tsx isSuggested). Joining moves it to its usual section.
    Suggested,
    Public,
    Group,
    Dm,
}

/// A person's picture: their own, else the Discord one (what the server's `avatar` falls back to).
pub fn avatar_of(u: &crate::api::chat::UserInfo) -> Option<&str> {
    u.avatar.as_deref().filter(|s| !s.trim().is_empty()).or(u.discord_avatar.as_deref())
}

pub fn is_dm(room: &ChatRoomItem) -> bool {
    room.chat_type == "dm" || room.room_key.starts_with("dm:")
}

/// A group you're not in, shown as its card: no messages, no members (GroupTeaser.tsx).
pub fn is_teaser(room: &ChatRoomItem) -> bool {
    room.is_teaser == Some(true)
}

pub fn kind(room: &ChatRoomItem) -> RoomKind {
    if is_teaser(room) || (room.chat_type == "public" && room.is_member == Some(false)) {
        RoomKind::Suggested
    } else if room.chat_type == "public" {
        RoomKind::Public
    } else if is_dm(room) {
        RoomKind::Dm
    } else {
        RoomKind::Group
    }
}

/// Everything but public rooms is end-to-end encrypted.
pub fn is_encrypted_room(room: &ChatRoomItem) -> bool {
    room.chat_type != "public"
}

/// The other person of a DM.
pub fn partner(room: &ChatRoomItem, me: i64) -> Option<&UserInfo> {
    is_dm(room).then(|| room.allowed_users.iter().find(|u| u.id != me)).flatten()
}

/// A DM is shown as the other person, never by the room's own stored title.
pub fn title(room: &ChatRoomItem, me: i64) -> String {
    if let Some(p) = partner(room, me) {
        return if p.display_name.is_empty() { p.username.clone() } else { p.display_name.clone() };
    }
    room.custom_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| room.title.clone())
}

/// AES-GCM payloads look like `{"iv": "...", "data": "...", "k": "..."}` (utils/e2ee.ts).
pub fn looks_encrypted(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with('{') && t.contains("\"iv\"") && t.contains("\"data\"")
}

/// The chat list's second line.
pub fn preview(room: &ChatRoomItem) -> String {
    match room.last_message.as_deref() {
        None | Some("") => if room.is_member == Some(false) { "Join to read and write".into() } else { "No messages yet".into() },
        Some(m) if looks_encrypted(m) => "🔒 Encrypted message".into(),
        Some(m) => super::timestamp::to_plain(m).replace('\n', " "),
    }
}

/// A message's body as shown, and whether it's a placeholder (drawn dimmed).
pub fn message_text(m: &ChatMessageItem) -> (String, bool) {
    if m.message_type == "deleted" {
        return ("Message deleted".into(), true);
    }
    if looks_encrypted(&m.content) {
        return (ENCRYPTED_TEXT.into(), true);
    }
    if m.content.trim().is_empty() && m.message_type != "text" {
        // Cards (cheques, invoices, campaigns, drops) show as themselves too.
        if matches!(m.message_type.as_str(), "cheque" | "invoice" | "campaign" | "drop") && m.attachment_data.as_ref().is_some_and(|a| a.get("encrypted").is_none()) {
            return (String::new(), false);
        }
        // Pictures and files show as themselves (message_list), with no text line.
        if is_media(m) && m.attachment_data.as_ref().is_some_and(|a| a.get("encrypted").is_none()) {
            return (String::new(), false);
        }
        if is_media(m) {
            return (ENCRYPTED_TEXT.into(), true);
        }
        return (format!("[{}] — open it in the web app for now", m.message_type.replace('_', " ")), true);
    }
    (m.content.clone(), false)
}

/// Messages that are a picture, a file, a GIF or a sticker.
pub fn is_media(m: &ChatMessageItem) -> bool {
    matches!(m.message_type.as_str(), "image" | "file" | "audio" | "video" | "gif" | "sticker")
}

/// The cache key of a message's picture: attachments by message, GIFs and stickers by their path.
pub fn media_key(m: &ChatMessageItem) -> Option<String> {
    let a = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none())?;
    match m.message_type.as_str() {
        "image" => a.get("file_data").and_then(|d| d.as_str()).filter(|d| d.starts_with("data:image/")).map(|_| format!("msg:{}", m.id)),
        "gif" | "sticker" => a.get("url").and_then(|u| u.as_str()).filter(|u| u.starts_with("/api/gifs/media/") || u.starts_with("/api/stickers/")).map(str::to_string),
        _ => None,
    }
}

pub fn local(iso: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(iso).ok().map(|t| t.with_timezone(&Local)).or_else(|| {
        // Some timestamps come without a zone: they're UTC on this server.
        chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|n| n.and_utc().with_timezone(&Local))
    })
}

/// "18:41" today, "Yesterday", "Mon" within a week, "Nov 3" before that.
pub fn short_time(iso: &str) -> String {
    let Some(t) = local(iso) else { return String::new() };
    let today = Local::now().date_naive();
    let day = t.date_naive();
    match (today - day).num_days() {
        0 => t.format("%H:%M").to_string(),
        1 => "Yesterday".into(),
        2..=6 => t.format("%a").to_string(),
        _ if day.year() == today.year() => t.format("%b %-d").to_string(),
        _ => t.format("%b %-d, %Y").to_string(),
    }
}

/// A message's header time, Discord style: "Today at 18:41", "Yesterday at 09:05", "Nov 3, 2025 18:41".
pub fn stamp(iso: &str) -> String {
    let Some(t) = local(iso) else { return String::new() };
    let today = Local::now().date_naive();
    let day = t.date_naive();
    let clock = t.format("%H:%M");
    match (today - day).num_days() {
        0 => format!("Today at {clock}"),
        1 => format!("Yesterday at {clock}"),
        _ if day.year() == today.year() => format!("{} {clock}", t.format("%b %-d")),
        _ => format!("{} {clock}", t.format("%b %-d, %Y")),
    }
}

/// The feed's day divider: "Today", "Yesterday", "Monday, Nov 10", "Nov 10, 2025".
pub fn day_label(t: &DateTime<Local>) -> String {
    let today = Local::now().date_naive();
    let day = t.date_naive();
    match (today - day).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => t.format("%A, %b %-d").to_string(),
        _ if day.year() == today.year() => t.format("%b %-d").to_string(),
        _ => t.format("%b %-d, %Y").to_string(),
    }
}

/// "online" / "last seen 5 min ago" / "last seen Nov 3".
pub fn presence(user: &UserInfo) -> (String, bool) {
    if user.is_online == Some(true) {
        return ("online".into(), true);
    }
    let Some(seen) = user.last_seen_at.as_deref().and_then(local) else { return ("last seen recently".into(), false) };
    let minutes = (Local::now() - seen).num_minutes().max(0);
    let text = match minutes {
        0..=1 => "last seen just now".into(),
        m if m < 60 => format!("last seen {m} min ago"),
        m if m < 60 * 24 => format!("last seen {} h ago", m / 60),
        _ => format!("last seen {}", short_time(user.last_seen_at.as_deref().unwrap_or_default())),
    };
    (text, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_ciphertext() {
        assert!(looks_encrypted(r#"{"iv":"a","data":"b","k":"c"}"#));
        assert!(!looks_encrypted("hello {iv}"));
        assert!(!looks_encrypted(r#"{"iv":"a"}"#));
    }

    #[test]
    fn dm_is_the_other_person() {
        let me = UserInfo { id: 1, username: "me".into(), ..Default::default() };
        let alina = UserInfo { id: 2, username: "alina".into(), display_name: "Alina".into(), ..Default::default() };
        let room = ChatRoomItem { room_key: "dm:1:2".into(), chat_type: "dm".into(), title: "stored".into(), allowed_users: vec![me, alina], ..Default::default() };
        assert_eq!(title(&room, 1), "Alina");
        assert_eq!(kind(&room), RoomKind::Dm);
    }
}
