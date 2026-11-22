// SPDX-License-Identifier: GPL-3.0-or-later
//! Who's in the room (ChatMembersSidebar.tsx + MemberContextMenu.tsx): docked on the right, with
//! search, presence, badges, a quick @mention and — for those who may — removing people.
//! Right-click someone for their menu. Big public rooms come from the server page by page.

use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, scrollable, space, svg, text, text_input, Column};
use iced::{Alignment, Color, Fill};

use super::display;
use super::manage_room::parse_color;
use crate::api::chat::{ChatRoomItem, RoomRole, UserInfo};
use crate::app::Message;
use crate::components::hover::Hover;
use crate::components::ui::{avatar_pic, icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::store::room_permissions as perms;
use crate::theme::{self, *};

/// A page of members of a big room, from the server.
pub struct Paged {
    pub room_key: String,
    pub query: String,
    pub members: Vec<UserInfo>,
    pub total: i64,
    pub loading: bool,
}

pub struct View<'a> {
    pub room: &'a ChatRoomItem,
    pub me: i64,
    pub is_admin: bool,
    pub search: &'a str,
    /// Big rooms: the server's pages (None: the room's own member list).
    pub paged: Option<&'a Paged>,
    pub pending: &'a [i64],
    pub menu: Option<i64>,
}

fn msg(m: ChatMsg) -> Message {
    Message::Chat(m)
}

/// Rooms whose member list is paged on the server (allowed_users carries only a few).
pub fn is_paged(room: &ChatRoomItem) -> bool {
    room.members_truncated == Some(true)
}

fn name_of(u: &UserInfo) -> String {
    if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() }
}

fn badge<'a>(label: impl Into<String>, fg: Color, bg: Color) -> El<'a> {
    container(text(label.into()).size(8.5).font(BOLD).color(fg)).padding([1, 4]).style(boxed(bg, Color::TRANSPARENT, 3.0)).into()
}

fn item<'a>(label: &'a str, handle: svg::Handle, danger: bool, on: Message) -> El<'a> {
    let color = if danger { DANGER_TEXT } else { TEXT_BODY };
    button(row![icon(handle, 15, if danger { DANGER_TEXT } else { MUTED }), text(label).size(13).color(color)].spacing(10).align_y(Alignment::Center))
        .width(Fill)
        .padding([7, 10])
        .style(move |_, s| button::Style {
            background: matches!(s, button::Status::Hovered).then(|| if danger { rgba(0xef4444, 0.14) } else { rgba(0x6366f1, 0.22) }.into()),
            border: border(Color::TRANSPARENT, 0.0, 6.0),
            text_color: color,
            ..Default::default()
        })
        .on_press(on)
        .into()
}

fn divider<'a>() -> El<'a> {
    container(container(space().width(Fill).height(1)).style(theme::rule)).padding([4, 6]).into()
}

/// The person's menu: profile, mention, message, copy, permissions, remove.
fn member_menu<'a>(u: &'a UserInfo, v: &View<'a>) -> El<'a> {
    let is_self = u.id == v.me;
    let mut col = column![
        container(column![text(name_of(u)).size(13.5).font(BOLD).color(Color::WHITE), text(format!("@{}", u.username)).size(11.5).color(MUTED)].spacing(1)).padding([4, 10]),
        divider(),
        item("View profile", icons::user(), false, msg(ChatMsg::OpenProfile(Box::new(u.clone())))),
        item("Mention", icons::at_sign(), false, msg(ChatMsg::Mention(u.username.clone()))),
    ]
    .spacing(1);
    if !is_self {
        col = col.push(item("Send message", icons::message_circle(), false, msg(ChatMsg::MessageUser(u.id, u.username.clone()))));
    }
    col = col
        .push(divider())
        .push(item("Copy username", icons::copy(), false, msg(ChatMsg::CopyValue(u.username.clone(), "Username copied"))))
        .push(item("Copy user ID", icons::hash(), false, msg(ChatMsg::CopyValue(u.id.to_string(), "User ID copied"))));
    if perms::can_edit_permissions(v.room, v.me, v.is_admin) && Some(u.id) != perms::owner_id(v.room) && v.room.chat_type == "group" {
        col = col.push(divider()).push(item("Permissions", icons::shield_check(), false, msg(ChatMsg::MemberPermissions(u.id))));
    }
    if perms::can_remove(v.room, v.me, v.is_admin, u.id) {
        col = col.push(divider()).push(item("Remove from room", icons::user_minus(), true, msg(ChatMsg::KickMember(u.id, u.username.clone()))));
    }
    container(col).width(230).padding(6).style(|_| container::Style {
        background: Some(hex(0x18191f).into()),
        border: border(rgba(0xffffff, 0.1), 1.0, 10.0),
        shadow: iced::Shadow { color: rgba(0x000000, 0.6), offset: iced::Vector::new(0.0, 8.0), blur_radius: 24.0 },
        ..Default::default()
    })
    .into()
}

fn member_row<'a>(u: &'a UserInfo, v: &View<'a>, roles: &[RoomRole], member_roles: &std::collections::HashMap<String, String>) -> El<'a> {
    let name = name_of(u);
    let (presence, online) = display::presence(u);
    let dot = container(space().width(10).height(10)).style(move |_| container::Style {
        background: Some(if online { hex(0x23a55a) } else { hex(0x80848e) }.into()),
        border: border(hex(0x0f1015), 2.0, 6.0),
        ..Default::default()
    });
    let face = iced::widget::stack![avatar_pic(&name, display::avatar_of(u), 32.0, None), container(dot).width(32).height(32).align_x(iced::alignment::Horizontal::Right).align_y(iced::alignment::Vertical::Bottom)];

    let mut line = row![text(name.clone()).size(13).font(SEMIBOLD).color(if online { Color::WHITE } else { LABEL }).wrapping(Wrapping::None)].spacing(5).align_y(Alignment::Center);
    if u.id == v.me {
        line = line.push(badge("YOU", INDIGO_SOFT, rgba(0x6366f1, 0.25)));
    }
    if Some(u.id) == perms::owner_id(v.room) && v.room.chat_type != "dm" {
        line = line.push(badge("OWNER", AMBER_TEXT, rgba(0xfbbf24, 0.18)));
    }
    if v.pending.contains(&u.id) {
        line = line.push(badge("AWAITING APPROVAL", AMBER_TEXT, rgba(0xf59e0b, 0.18)));
    }
    if let Some(r) = member_roles.get(&u.id.to_string()).and_then(|id| roles.iter().find(|r| &r.id == id)) {
        let c = parse_color(&r.color);
        line = line.push(badge(r.name.clone(), c, Color { a: 0.15, ..c }));
    }
    let sub = row![
        text(format!("@{}", u.username)).size(10.5).color(TEXT_SECONDARY).wrapping(Wrapping::None),
        text("•").size(10).color(rgba(0x8b92a5, 0.4)),
        text(if online { "Online".to_string() } else { presence.trim_start_matches("last seen ").to_string() }).size(10).color(if online { hex(0x23a55a) } else { TEXT_SECONDARY }).wrapping(Wrapping::None),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mut r = row![face, container(column![line, sub].spacing(1)).width(Fill).clip(true)].spacing(10).align_y(Alignment::Center);
    if u.id != v.me {
        r = r.push(
            iced::widget::tooltip(
                button(icon(icons::at_sign(), 13, FAINT)).padding(4).style(ghost(6.0, rgba(0x6366f1, 0.2))).on_press(msg(ChatMsg::Mention(u.username.clone()))),
                container(text(format!("Mention @{}", u.username)).size(11).color(TEXT_BODY)).padding([4, 8]).style(boxed(hex(0x111216), BORDER, 6.0)),
                iced::widget::tooltip::Position::Left,
            ),
        );
    }
    if perms::can_remove(v.room, v.me, v.is_admin, u.id) {
        r = r.push(button(icon(icons::user_minus(), 13, FAINT)).padding(4).style(ghost(6.0, rgba(0xef4444, 0.2))).on_press(msg(ChatMsg::KickMember(u.id, u.username.clone()))));
    }
    let content = iced::widget::mouse_area(container(r).width(Fill).padding([6, 8])).on_press(msg(ChatMsg::OpenProfile(Box::new(u.clone())))).interaction(iced::mouse::Interaction::Pointer);
    let menu = (v.menu == Some(u.id)).then(|| member_menu(u, v));
    Hover::new(content)
        .menu(menu, msg(ChatMsg::CloseMemberMenu))
        .highlight(rgba(0xffffff, 0.06))
        .on_right_click(msg(ChatMsg::MemberMenu(u.id)))
        .into()
}

pub fn view<'a>(v: View<'a>) -> El<'a> {
    let roles: Vec<RoomRole> = v.room.settings.as_ref().and_then(|s| s.get("roles")).and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();
    let member_roles: std::collections::HashMap<String, String> = v.room.settings.as_ref().and_then(|s| s.get("member_roles")).and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();

    let q = v.search.trim().to_lowercase();
    let (mut list, total, has_more, loading): (Vec<&UserInfo>, i64, bool, bool) = match v.paged {
        Some(p) => (p.members.iter().collect(), p.total, (p.members.len() as i64) < p.total, p.loading),
        None => {
            let all: Vec<&UserInfo> = v.room.allowed_users.iter().filter(|u| q.is_empty() || u.username.to_lowercase().contains(&q) || u.display_name.to_lowercase().contains(&q)).collect();
            let n = v.room.member_count.unwrap_or(v.room.allowed_users.len() as i64);
            (all, n, false, false)
        }
    };
    // Online first, then by name (the owner on top).
    let owner = perms::owner_id(v.room);
    list.sort_by_key(|u| (Some(u.id) != owner, u.is_online != Some(true), name_of(u).to_lowercase()));

    let mut rows = Column::new().spacing(1);
    if list.is_empty() {
        rows = rows.push(center(text(if loading { "Loading…" } else { "No members found" }).size(12).color(TEXT_SECONDARY)).height(60));
    }
    for u in &list {
        rows = rows.push(member_row(u, &v, &roles, &member_roles));
    }
    if has_more || (loading && !list.is_empty()) {
        rows = rows.push(
            center(if loading {
                El::from(text("Loading…").size(11).color(FAINT))
            } else {
                button(text(format!("{} of {total} — load more", list.len())).size(11).color(INDIGO_SOFT)).padding(4).style(ghost(6.0, rgba(0x6366f1, 0.12))).on_press(msg(ChatMsg::MoreMembers)).into()
            })
            .height(36),
        );
    }

    container(
        column![
            container(row![text(format!("MEMBERS — {total}")).size(11).font(BOLD).color(TEXT_SECONDARY), space().width(Fill), icon(icons::users(), 13, FAINT)].align_y(Alignment::Center)).padding([16, 16]),
            container(text_input("Search members...", v.search).on_input(|s| msg(ChatMsg::MemberSearch(s))).size(12.5).padding([7, 10]).style(field)).padding([0, 12]),
            scrollable(container(rows).padding([8, 8])).height(Fill).style(thin_scroll),
        ]
        .spacing(4)
        .height(Fill),
    )
    .width(270)
    .height(Fill)
    .style(|_| container::Style { background: Some(hex(0x0f1015).into()), border: iced::Border { color: BORDER, width: 1.0, radius: 0.0.into() }, ..Default::default() })
    .into()
}
