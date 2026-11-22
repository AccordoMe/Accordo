// SPDX-License-Identifier: GPL-3.0-or-later
//! A group shown to you through its view rules that you're not in (GroupTeaser.tsx): who it is and
//! how alive it is, never its messages — those stay end-to-end encrypted. Asking to join puts you
//! on its managers' approval list; once approved the group opens as usual.

use iced::widget::{button, center, column, container, row, text, text_editor};
use iced::{Alignment, Color, Fill};

use super::join_invite::NOTE_MAX;
use super::room_avatar;
use crate::api::chat::ChatRoomItem;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

#[derive(Debug, Clone)]
pub enum TeaserMsg {
    Note(text_editor::Action),
    Ask(String),
    Withdraw(String),
    Done(Result<&'static str, String>),
}

fn m(t: TeaserMsg) -> Message {
    Message::Chat(ChatMsg::Teaser(t))
}

/// "active 5 min ago" (activityAgo): enough to tell a lively group from a quiet one.
pub fn activity_ago(iso: Option<&str>) -> String {
    let Some(at) = iso.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok().or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|n| n.and_utc().fixed_offset()))) else {
        return "no messages yet".into();
    };
    let s = (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_seconds().max(0);
    match s {
        0..60 => "active just now".into(),
        60..3600 => format!("active {} min ago", s / 60),
        3600..86400 => format!("active {} h ago", s / 3600),
        _ => format!("active {} d ago", s / 86400),
    }
}

/// The sidebar's second line for a teaser row: (text, it's "Request sent").
pub fn row_hint(room: &ChatRoomItem) -> (String, bool) {
    match &room.teaser {
        Some(t) if t.requested => ("Request sent".into(), true),
        _ => (room.topic.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Tap to ask to join").to_string(), false),
    }
}

pub fn view<'a>(room: &'a ChatRoomItem, me: i64, note: &'a text_editor::Content, busy: bool) -> El<'a> {
    let t = room.teaser.clone().unwrap_or_default();
    let stat = |handle, s: String| row![icon(handle, 14, MUTED), text(s).size(13).color(MUTED)].spacing(5).align_y(Alignment::Center);
    let members = format!("{} member{}{}", t.members, if t.members == 1 { "" } else { "s" }, if t.online > 0 { format!(" · {} online", t.online) } else { String::new() });
    let mut stats = row![stat(icons::users(), members)].spacing(16);
    if t.last_activity.is_some() {
        stats = stats.push(stat(icons::clock(), activity_ago(t.last_activity.as_deref())));
    }
    let mut col = column![room_avatar::view(room, me, 84.0), text(room.title.clone()).size(22).font(BOLD).color(Color::WHITE)].spacing(14).align_x(Alignment::Center);
    if let Some(topic) = room.topic.as_deref().filter(|s| !s.trim().is_empty()) {
        col = col.push(container(text(topic.to_string()).size(14).color(hex(0xa1a8b8)).line_height(1.55).center()).max_width(480));
    }
    col = col.push(stats);
    col = col.push(row![icon(icons::lock(), 12, FAINT), text("Messages are end-to-end encrypted — members only").size(12).color(FAINT)].spacing(6).align_y(Alignment::Center));

    let action: El = if t.requested {
        let mut c = column![container(row![icon(icons::check(), 15, GREEN), text("Request sent — waiting for an admin").size(14).font(SEMIBOLD).color(GREEN)].spacing(6).align_y(Alignment::Center))
            .padding([10, 18])
            .style(boxed(rgba(0x10b981, 0.1), rgba(0x10b981, 0.35), 10.0))]
        .spacing(8)
        .align_x(Alignment::Center);
        if t.can_withdraw {
            c = c.push(button(text("Withdraw request").size(12.5).color(hex(0x8b92a5))).padding([2, 6]).style(ghost(6.0, rgba(0xffffff, 0.05))).on_press_maybe((!busy).then(|| m(TeaserMsg::Withdraw(room.room_key.clone())))));
        }
        c.into()
    } else if let Some(closed) = t.closed.as_deref() {
        let s = if closed == "full" { "This group is full right now" } else { "This group isn't taking new members right now" };
        container(text(s).size(14).color(MUTED)).padding([10, 18]).style(boxed(rgba(0xffffff, 0.04), rgba(0xffffff, 0.06), 10.0)).into()
    } else {
        let len = note.text().trim_end().chars().count();
        column![
            text_editor(note)
                .placeholder("Why should they let you in? (optional)")
                .height(80)
                .size(13.5)
                .padding([10, 12])
                .style(crate::components::chain_settings::editor_style)
                .on_action(|a| m(TeaserMsg::Note(a))),
            row![
                text("The group's admins read this. It isn't encrypted — don't put anything secret here.").size(11).color(FAINT).width(Fill),
                text(format!("{len}/{NOTE_MAX}")).size(11).color(if len > NOTE_MAX { DANGER_TEXT } else { FAINT }),
            ]
            .spacing(8),
            button(center(text(if busy { "Sending…" } else { "Ask to join" }).size(15).font(BOLD)).height(22))
                .width(Fill)
                .padding([10, 26])
                .style(primary)
                .on_press_maybe((!busy && len <= NOTE_MAX).then(|| m(TeaserMsg::Ask(room.room_key.clone())))),
        ]
        .spacing(8)
        .width(420)
        .into()
    };
    center(col.push(action)).padding(24).into()
}
