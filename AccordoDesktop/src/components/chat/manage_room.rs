// SPDX-License-Identifier: GPL-3.0-or-later
//! "Manage Room" (ManageRoomModal.tsx + RoomMembersPanel, InviteLinkSection, EveryonePermissions,
//! JoinRequestsTab): General, Members, Requests, Settings. A desktop-sized window with the tabs
//! down the left. What each control does (API calls, sharing the room key) is in
//! pages/room_admin.rs; this is the state of the form and how it looks.

use std::collections::BTreeMap;

use iced::alignment::Horizontal;
use iced::widget::{button, center, checkbox, column, container, mouse_area, opaque, pick_list, row, scrollable, space, svg, text, text_input, toggler, Column};
use iced::{Alignment, Color, Fill, Length, Padding};

use super::room_avatar;
use crate::api::chat::{ChatRoomItem, InviteStatus, JoinRequestItem, RoomRole, UserInfo};
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{avatar_pic, icon, truncate, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::store::room_permissions::{self as perms, Perm, Perms, PERMISSIONS};
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Members,
    Requests,
    Settings,
}

pub const SLOW_MODE: [(i64, &str); 6] = [(0, "Off"), (5, "5 seconds"), (10, "10 seconds"), (30, "30 seconds"), (60, "1 minute"), (300, "5 minutes")];
pub const AUTO_BURN: [(&str, &str); 6] = [("off", "Off"), ("30s", "30 seconds"), ("5m", "5 minutes"), ("1h", "1 hour"), ("24h", "24 hours"), ("7d", "7 days")];
pub const EXPIRY: [(Option<i64>, &str); 3] = [(Some(24), "24 hours"), (Some(24 * 7), "7 days"), (None, "Never")];
pub const USES: [(Option<i64>, &str); 4] = [(None, "No limit"), (Some(1), "1 use"), (Some(10), "10"), (Some(100), "100")];
pub const ROLE_COLORS: [u32; 10] = [0xf87171, 0xfb923c, 0xfbbf24, 0x4ade80, 0x34d399, 0x38bdf8, 0x818cf8, 0xc084fc, 0xf472b6, 0x94a3b8];
const MAX_ADD_ROWS: usize = 20;

/// An option of a drop-down (pick_list wants Display + PartialEq).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice<T> {
    pub value: T,
    pub label: String,
}

impl<T> std::fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// A step that needs a second click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Kick(i64, String),
    MakeChannel,
    RevokeInvite,
}

#[derive(Debug, Clone)]
pub enum ManageMsg {
    Tab(Tab),
    Close,
    // general
    Title(String),
    Topic(String),
    SaveGeneral,
    /// Pick a picture file for the room.
    AvatarPick,
    /// The room's new picture (a data URL), or None: back to the default.
    Avatar(Option<String>),
    // members
    AddRow(usize, String),
    AddRowMore,
    AddRowRemove(usize),
    AddPeople,
    Expand(i64),
    MemberRole(i64, String),
    MemberCustom(i64, bool),
    MemberPerm(i64, Perm),
    DefaultPerm(Perm),
    Remove(i64, String),
    Approve(i64),
    // invite link
    InviteEdit(bool),
    Expiry(Option<i64>),
    MaxUses(Option<i64>),
    Approval(bool),
    CreateInvite,
    CopyInvite,
    RevokeInvite,
    // everyone
    EveryoneCycle(Perm),
    EveryoneReset,
    EveryoneApply,
    ToggleChannel,
    // requests
    Decide(i64, bool),
    // settings
    SlowMode(i64),
    AutoBurn(String),
    Gifs(bool),
    NewRole(String),
    RoleColor(usize),
    AddRole,
    DeleteRole(String),
    // confirmations
    Confirm,
    Cancel,
}

pub struct State {
    pub room_key: String,
    pub tab: Tab,
    pub title: String,
    pub topic: String,
    pub saving: bool,
    pub avatar_saving: bool,
    pub add_rows: Vec<String>,
    pub adding: bool,
    pub expanded: Option<i64>,
    /// The full link (with the #k secret) — known only right after it was made here.
    pub invite_link: Option<String>,
    pub invite: Option<InviteStatus>,
    pub invite_editing: bool,
    pub expiry: Option<i64>,
    pub max_uses: Option<i64>,
    pub approval: bool,
    pub generating: bool,
    /// Everyone: true = give to everyone, false = take from everyone.
    pub everyone: BTreeMap<Perm, bool>,
    pub everyone_busy: bool,
    /// None while loading.
    pub requests: Option<Vec<JoinRequestItem>>,
    pub deciding: Option<i64>,
    pub new_role: String,
    pub new_role_color: usize,
    pub confirm: Option<Confirm>,
}

impl State {
    pub fn new(room: &ChatRoomItem, tab: Tab) -> Self {
        Self {
            room_key: room.room_key.clone(),
            tab,
            title: room.title.clone(),
            topic: room.topic.clone().unwrap_or_default(),
            saving: false,
            avatar_saving: false,
            add_rows: vec![String::new()],
            adding: false,
            expanded: None,
            invite_link: None,
            invite: None,
            invite_editing: false,
            expiry: Some(24 * 7),
            max_uses: None,
            approval: false,
            generating: false,
            everyone: BTreeMap::new(),
            everyone_busy: false,
            requests: None,
            deciding: None,
            new_role: String::new(),
            new_role_color: 0,
            confirm: None,
        }
    }

    pub fn add_usernames(&self) -> Vec<String> {
        self.add_rows.iter().map(|u| u.trim().trim_start_matches('@').to_string()).filter(|u| !u.is_empty()).collect()
    }
}

/// What the viewer may do here (from the room and their permissions).
pub struct Access {
    pub me: i64,
    pub is_admin: bool,
    /// Title, topic, settings (owner or "Manage room").
    pub manage_room: bool,
    pub manage_members: bool,
    pub edit_permissions: bool,
    /// Holds the room key here (may hand it on).
    pub has_key: bool,
}

impl Access {
    pub fn of(room: &ChatRoomItem, me: i64, is_admin: bool, has_key: bool) -> Self {
        let mine = perms::effective(room, me, is_admin);
        Self {
            me,
            is_admin,
            manage_room: perms::is_room_owner(room, me, is_admin) || mine.contains(&Perm::ManageRoom),
            manage_members: mine.contains(&Perm::ManageMembers),
            edit_permissions: perms::can_edit_permissions(room, me, is_admin),
            has_key,
        }
    }
}

fn m(msg: ManageMsg) -> Message {
    Message::Chat(ChatMsg::Manage(msg))
}

pub fn is_group(room: &ChatRoomItem) -> bool {
    room.chat_type == "group"
}

pub fn can_see_requests(room: &ChatRoomItem, access: &Access) -> bool {
    is_group(room) && access.manage_members
}

/// The tab actually shown: Members/Requests exist only for groups (and Requests for managers).
pub fn shown_tab(state: &State, room: &ChatRoomItem, access: &Access) -> Tab {
    match state.tab {
        Tab::Members | Tab::Requests if !is_group(room) => Tab::General,
        Tab::Requests if !can_see_requests(room, access) => Tab::Members,
        t => t,
    }
}

// ── pieces ──

fn section<'a>(handle: svg::Handle, title: &'a str, aside: Option<El<'a>>, body: El<'a>) -> El<'a> {
    let mut head = row![icon(handle, 14, INDIGO_SOFT), text(title).size(13).font(BOLD).color(TEXT_BRIGHT)].spacing(7).align_y(Alignment::Center);
    if let Some(a) = aside {
        head = head.push(space().width(Fill)).push(a);
    }
    container(column![head, body].spacing(12)).width(Fill).padding([16, 18]).style(boxed(rgba(0xffffff, 0.025), BORDER, 12.0)).into()
}

fn hint(s: impl Into<String>) -> El<'static> {
    text(s.into()).size(12).color(TEXT_SECONDARY).line_height(1.45).into()
}

fn field_label(s: &str) -> El<'_> {
    text(s).size(12).font(SEMIBOLD).color(TEXT_SECONDARY).into()
}

fn input<'a>(placeholder: &'a str, value: &'a str, on: impl Fn(String) -> ManageMsg + 'a) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value).size(13.5).padding([10, 12]).style(field).on_input(move |v| m(on(v)))
}

fn small_button<'a>(content: El<'a>, style: impl Fn(&iced::Theme, button::Status) -> button::Style + 'a, on: Option<ManageMsg>) -> El<'a> {
    button(content).padding([6, 12]).style(style).on_press_maybe(on.map(m)).into()
}

fn tag<'a>(label: impl Into<String>, fg: Color, bg: Color) -> El<'a> {
    container(text(label.into()).size(9.5).font(BOLD).color(fg)).padding([1, 6]).style(boxed(bg, Color::TRANSPARENT, 4.0)).into()
}

fn perm_chips<'a>(value: &Perms, disabled: bool, on: impl Fn(Perm) -> ManageMsg) -> El<'a> {
    let mut wrap = iced::widget::Row::new().spacing(6);
    for p in PERMISSIONS.iter() {
        let on_it = value.contains(&p.perm);
        let label = format!("{} {}", if on_it { "✓" } else { "+" }, p.label);
        wrap = wrap.push(
            iced::widget::tooltip(
                button(text(label).size(11.5).font(SEMIBOLD)).padding([5, 11]).style(pill(on_it)).on_press_maybe((!disabled).then(|| m(on(p.perm)))),
                container(text(if p.hint.is_empty() { p.label } else { p.hint }).size(11.5).color(TEXT_BODY)).padding([5, 9]).max_width(280).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
                iced::widget::tooltip::Position::Top,
            ),
        );
    }
    wrap.wrap().vertical_spacing(6).into()
}

fn confirm_box<'a>(question: String, detail: &'a str, yes: &'a str, danger: bool) -> El<'a> {
    let card = container(
        column![
            text(question).size(15).font(BOLD).color(Color::WHITE),
            text(detail).size(12.5).color(TEXT_SECONDARY).line_height(1.5),
            row![
                space().width(Fill),
                button(text("Cancel").size(13)).padding([8, 16]).style(secondary).on_press(m(ManageMsg::Cancel)),
                button(text(yes).size(13).font(SEMIBOLD)).padding([8, 16]).style(tone_button(if danger { Tone::Danger } else { Tone::Accent })).on_press(m(ManageMsg::Confirm)),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
    .width(400)
    .padding(20)
    .style(modal);
    mouse_area(center(opaque(card)).style(backdrop)).on_press(m(ManageMsg::Cancel)).into()
}

// ── tabs ──

fn general<'a>(state: &'a State, room: &'a ChatRoomItem, access: &Access) -> El<'a> {
    if super::display::is_dm(room) {
        return section(
            icons::user(),
            "Direct message",
            None,
            row![
                room_avatar::view(room, access.me, 64.0),
                column![
                    text(super::display::title(room, access.me)).size(16).font(BOLD).color(Color::WHITE),
                    hint("A direct message shows the other person's name and picture.")
                ]
                .spacing(3)
            ]
            .spacing(16)
            .align_y(Alignment::Center)
            .into(),
        );
    }
    let avatar_row = row![
        room_avatar::view(room, access.me, 72.0),
        column![
            text(room.title.clone()).size(17).font(BOLD).color(Color::WHITE),
            text(format!("{} member{}", room.allowed_user_ids.len(), if room.allowed_user_ids.len() == 1 { "" } else { "s" })).size(12).color(TEXT_SECONDARY),
            if access.manage_room {
                let change = button(row![icon(icons::camera(), 12, INDIGO_SOFT), text(if state.avatar_saving { "Saving…" } else { "Change avatar" }).size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                    .padding([5, 10])
                    .style(outline_indigo)
                    .on_press_maybe((!state.avatar_saving).then(|| m(ManageMsg::AvatarPick)));
                let mut r = row![change].spacing(8).align_y(Alignment::Center);
                if room.avatar.as_deref().is_some_and(|a| !a.trim().is_empty()) {
                    r = r.push(button(text("Remove").size(12).color(DANGER_TEXT)).padding([5, 8]).style(ghost(6.0, rgba(0xef4444, 0.1))).on_press_maybe((!state.avatar_saving).then(|| m(ManageMsg::Avatar(None)))));
                }
                El::from(r)
            } else {
                hint("Room avatar")
            },
        ]
        .spacing(6),
    ]
    .spacing(18)
    .align_y(Alignment::Center);

    let details: El = if access.manage_room {
        let ready = !state.title.trim().is_empty() && !state.saving;
        column![
            column![field_label("Room title"), input("Enter chat title…", &state.title, ManageMsg::Title).on_submit(m(ManageMsg::SaveGeneral))].spacing(6),
            column![field_label("Topic (optional)"), input("What is this chat about?", &state.topic, ManageMsg::Topic).on_submit(m(ManageMsg::SaveGeneral))].spacing(6),
            row![
                space().width(Fill),
                button(text(if state.saving { "Saving…" } else { "Save changes" }).size(13).font(SEMIBOLD)).padding([9, 18]).style(primary).on_press_maybe(ready.then(|| m(ManageMsg::SaveGeneral)))
            ],
        ]
        .spacing(14)
        .into()
    } else {
        column![
            column![field_label("Room title"), text(room.title.clone()).size(14).font(SEMIBOLD).color(Color::WHITE)].spacing(3),
            column![field_label("Topic"), text(room.topic.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| "—".into())).size(13).color(LABEL)].spacing(3),
            hint("Only the room owner can change these."),
        ]
        .spacing(12)
        .into()
    };
    column![section(icons::image(), "Appearance", None, avatar_row.into()), section(icons::pencil(), "Name and topic", None, details)].spacing(14).into()
}

fn invite_section<'a>(state: &'a State, room: &'a ChatRoomItem) -> El<'a> {
    let status = state.invite.as_ref().filter(|s| s.active == Some(true));
    let rules_line = status.map(|s| {
        let mut parts = vec![match s.expires_at.as_deref().and_then(super::display::local) {
            Some(t) => {
                let left = t - chrono::Local::now();
                if left.num_minutes() <= 0 {
                    "expired".to_string()
                } else if left.num_hours() >= 48 {
                    format!("{} days left", (left.num_hours() as f64 / 24.0).round())
                } else if left.num_hours() >= 1 {
                    format!("{} h left", left.num_hours())
                } else {
                    format!("{} min left", left.num_minutes().max(1))
                }
            }
            None => "never expires".into(),
        }];
        parts.push(match s.max_uses {
            Some(max) => format!("{}/{max} joined", s.uses),
            None => format!("{} joined", s.uses),
        });
        if s.requires_approval {
            parts.push("approval on".into());
        }
        parts.join(" · ")
    });
    let show_options = state.invite_editing || (state.invite_link.is_none() && status.is_none());
    let body: El = if show_options {
        let chips = |label: &'static str, handle: svg::Handle, items: Vec<El<'a>>| -> El<'a> {
            let mut r = row![row![icon(handle, 12, MUTED), text(label).size(12).color(MUTED)].spacing(5).width(80).align_y(Alignment::Center)].spacing(6).align_y(Alignment::Center);
            for i in items {
                r = r.push(i);
            }
            r.into()
        };
        let expiry = EXPIRY.iter().map(|(h, l)| button(text(*l).size(11.5).font(SEMIBOLD)).padding([5, 11]).style(pill(state.expiry == *h)).on_press(m(ManageMsg::Expiry(*h))).into()).collect();
        let uses = USES.iter().map(|(u, l)| button(text(*l).size(11.5).font(SEMIBOLD)).padding([5, 11]).style(pill(state.max_uses == *u)).on_press(m(ManageMsg::MaxUses(*u))).into()).collect();
        let existing = state.invite_link.is_some() || status.is_some();
        let mut actions = row![button(
            row![icon(icons::link_2(), 14, Color::WHITE), text(if state.generating { "Creating…" } else if existing { "Create new link" } else { "Create invite link" }).size(13).font(SEMIBOLD)]
                .spacing(6)
                .align_y(Alignment::Center)
        )
        .padding([8, 14])
        .style(primary)
        .on_press_maybe((!state.generating).then(|| m(ManageMsg::CreateInvite)))]
        .spacing(8);
        if state.invite_editing {
            actions = actions.push(button(text("Cancel").size(12.5)).padding([8, 12]).style(secondary).on_press(m(ManageMsg::InviteEdit(false))));
        }
        let mut col = column![
            chips("Expires", icons::clock(), expiry),
            chips("Joins", icons::users(), uses),
            row![
                checkbox(state.approval).size(15).style(tick).on_toggle(|v| m(ManageMsg::Approval(v))),
                icon(icons::shield_check(), 13, INDIGO_SOFT),
                text("Admins approve each person who uses the link").size(12.5).color(LABEL)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            actions,
        ]
        .spacing(10);
        if existing {
            col = col.push(text("A new link replaces the current one — the old link stops working.").size(11.5).color(FAINT));
        }
        col.into()
    } else {
        let top: El = match &state.invite_link {
            Some(link) => row![
                container(text(link.clone()).size(12).font(iced::Font::MONOSPACE).color(TEXT_BODY).wrapping(iced::widget::text::Wrapping::None)).width(Fill).padding([9, 12]).clip(true).style(boxed(hex(0x0e0f17), BORDER, 8.0)),
                button(row![icon(icons::copy(), 14, Color::WHITE), text("Copy").size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).padding([8, 14]).style(primary).on_press(m(ManageMsg::CopyInvite)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            None => hint("A link is active. Its full address (with the security part) is shown only right after it's created — make a new one to share it again."),
        };
        column![
            top,
            row![
                button(row![icon(icons::refresh_cw(), 12, MUTED), text("New link…").size(12)].spacing(5).align_y(Alignment::Center)).padding([4, 6]).style(ghost(6.0, rgba(0xffffff, 0.05))).on_press(m(ManageMsg::InviteEdit(true))),
                button(row![icon(icons::ban(), 12, DANGER_TEXT), text("Turn off").size(12).color(DANGER_TEXT)].spacing(5).align_y(Alignment::Center)).padding([4, 6]).style(ghost(6.0, rgba(0xef4444, 0.08))).on_press(m(ManageMsg::RevokeInvite)),
            ]
            .spacing(10),
        ]
        .spacing(8)
        .into()
    };
    let _ = room;
    section(icons::link_2(), "Invite link", rules_line.map(|l| El::from(text(l).size(11.5).color(TEXT_SECONDARY))), body)
}

fn add_people(state: &State) -> El<'_> {
    let mut rows = Column::new().spacing(6);
    for (i, value) in state.add_rows.iter().enumerate() {
        let mut r = row![
            container(text((i + 1).to_string()).size(10.5).font(BOLD).color(TEXT_SECONDARY)).width(22).height(22).align_x(Horizontal::Center).align_y(iced::alignment::Vertical::Center).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 5.0)),
            input("Username / email", value, move |v| ManageMsg::AddRow(i, v)).on_submit(m(ManageMsg::AddPeople)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if state.add_rows.len() > 1 {
            r = r.push(button(icon(icons::x(), 14, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(ManageMsg::AddRowRemove(i))));
        }
        rows = rows.push(r);
    }
    let filled = !state.add_usernames().is_empty() && !state.adding;
    let body = column![
        rows,
        row![
            button(row![icon(icons::plus(), 12, MUTED), text("Add person").size(12)].spacing(5).align_y(Alignment::Center))
                .padding([5, 11])
                .style(pill(false))
                .on_press_maybe((state.add_rows.len() < MAX_ADD_ROWS).then(|| m(ManageMsg::AddRowMore))),
            space().width(Fill),
            button(row![icon(icons::user_plus(), 14, Color::WHITE), text(if state.adding { "Adding…" } else { "Add" }).size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                .padding([8, 16])
                .style(primary)
                .on_press_maybe(filled.then(|| m(ManageMsg::AddPeople))),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    section(icons::user_plus(), "Add people", None, body.into())
}

fn everyone_section<'a>(state: &'a State, room: &'a ChatRoomItem) -> El<'a> {
    let is_channel = !perms::default_permissions(room).contains(&Perm::SendMessages);
    let channel = button(row![icon(icons::megaphone(), 12, if is_channel { AMBER_TEXT } else { INDIGO_SOFT }), text(if is_channel { "Channel · let everyone post" } else { "Make it a channel" }).size(11.5).font(BOLD)].spacing(5).align_y(Alignment::Center))
        .padding([5, 10])
        .style(outline_indigo)
        .on_press_maybe((!state.everyone_busy).then(|| m(ManageMsg::ToggleChannel)));
    let mut chips = iced::widget::Row::new().spacing(6);
    for p in PERMISSIONS.iter() {
        let change = state.everyone.get(&p.perm).copied();
        let (prefix, color) = match change {
            Some(true) => ("+ ", GREEN),
            Some(false) => ("− ", DANGER_TEXT),
            None => ("", MUTED),
        };
        chips = chips.push(
            button(text(format!("{prefix}{}", p.label)).size(11.5).font(SEMIBOLD).color(color))
                .padding([5, 11])
                .style(pill(change.is_some()))
                .on_press_maybe((!state.everyone_busy).then(|| m(ManageMsg::EveryoneCycle(p.perm)))),
        );
    }
    let mut body = column![hint("Tap a permission to give it to everyone (+), again to take it from everyone (−). Applies to the default and to members with their own list."), chips.wrap().vertical_spacing(6)].spacing(10);
    if !state.everyone.is_empty() {
        let give = state.everyone.values().filter(|v| **v).count();
        let take = state.everyone.len() - give;
        let summary = [(give > 0).then(|| format!("give {give}")), (take > 0).then(|| format!("take {take}"))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        body = body.push(
            row![
                button(text(if state.everyone_busy { "Applying…" } else { "Apply to everyone" }).size(12.5).font(SEMIBOLD)).padding([7, 14]).style(primary).on_press_maybe((!state.everyone_busy).then(|| m(ManageMsg::EveryoneApply))),
                button(text("Reset").size(12)).padding([7, 10]).style(ghost(6.0, rgba(0xffffff, 0.05))).on_press(m(ManageMsg::EveryoneReset)),
                text(summary).size(11.5).color(FAINT),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    section(icons::users(), "Everyone", Some(channel.into()), body.into())
}

fn member_row<'a>(state: &'a State, room: &'a ChatRoomItem, access: &Access, u: &'a UserInfo, pending: bool) -> El<'a> {
    let owner = perms::owner_id(room);
    let is_owner = Some(u.id) == owner;
    let roles: Vec<RoomRole> = room.settings.as_ref().and_then(|s| s.get("roles")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let member_roles: std::collections::HashMap<String, String> = room.settings.as_ref().and_then(|s| s.get("member_roles")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let role = member_roles.get(&u.id.to_string()).and_then(|id| roles.iter().find(|r| &r.id == id));
    let own = perms::custom_permissions(room, u.id);
    let effective = perms::effective(room, u.id, false);
    let open = state.expanded == Some(u.id);

    let name = if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() };
    let mut badges = row![text(name.clone()).size(13.5).font(SEMIBOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center);
    if is_owner {
        badges = badges.push(tag("OWNER", AMBER_TEXT, rgba(0xfbbf24, 0.18)));
    }
    if u.id == access.me {
        badges = badges.push(tag("YOU", INDIGO_SOFT, rgba(0x6366f1, 0.25)));
    }
    if let Some(r) = role {
        let c = parse_color(&r.color);
        badges = badges.push(tag(r.name.clone(), c, Color { a: 0.15, ..c }));
    }
    if pending {
        badges = badges.push(tag("AWAITING APPROVAL", AMBER_TEXT, rgba(0xf59e0b, 0.18)));
    }
    let what = if is_owner {
        "can do everything".to_string()
    } else if effective.is_empty() {
        "can only read".to_string()
    } else {
        format!("{}: {}", if own.is_some() { "custom" } else { "default" }, effective.iter().map(|p| p.label().to_lowercase()).collect::<Vec<_>>().join(", "))
    };

    let mut actions = row![].spacing(6).align_y(Alignment::Center);
    if pending && access.has_key && access.manage_members {
        actions = actions.push(small_button(text("Approve").size(11.5).font(SEMIBOLD).into(), primary, Some(ManageMsg::Approve(u.id))));
    }
    if access.manage_members && !roles.is_empty() && !is_owner {
        let mut options = vec![Choice { value: String::new(), label: "No role".into() }];
        options.extend(roles.iter().map(|r| Choice { value: r.id.clone(), label: r.name.clone() }));
        let selected = options.iter().find(|o| Some(&o.value) == member_roles.get(&u.id.to_string())).cloned().or_else(|| options.first().cloned());
        let id = u.id;
        actions = actions.push(pick_list(options, selected, move |c: Choice<String>| m(ManageMsg::MemberRole(id, c.value))).text_size(11.5).padding([4, 8]).style(token_select).menu_style(chain_menu));
    }
    if access.edit_permissions && !is_owner {
        actions = actions.push(small_button(
            row![text("Permissions").size(11.5).font(SEMIBOLD), icon(if open { icons::chevron_up() } else { icons::chevron_down() }, 12, INDIGO_SOFT)].spacing(4).align_y(Alignment::Center).into(),
            pill(open),
            Some(ManageMsg::Expand(u.id)),
        ));
    }
    if perms::can_remove(room, access.me, access.is_admin, u.id) {
        actions = actions.push(
            button(row![icon(icons::user_minus(), 12, DANGER_TEXT), text("Remove").size(11.5).font(SEMIBOLD).color(DANGER_TEXT)].spacing(4).align_y(Alignment::Center))
                .padding([5, 10])
                .style(tone_button(Tone::Danger))
                .on_press(m(ManageMsg::Remove(u.id, u.username.clone()))),
        );
    }

    let mut col = column![row![
        avatar_pic(&name, super::display::avatar_of(u), 34.0, None),
        column![badges, text(format!("@{} · {what}", u.username)).size(11.5).color(TEXT_SECONDARY).wrapping(iced::widget::text::Wrapping::None)].spacing(2).width(Fill),
        actions,
    ]
    .spacing(12)
    .align_y(Alignment::Center)];
    if open {
        let defaults = perms::default_permissions(room);
        let id = u.id;
        col = col.push(container(space().width(Fill).height(1)).style(rule)).push(
            column![
                checkbox(own.is_none()).label("Same as every member").size(15).text_size(12.5).style(tick).on_toggle(move |same| m(ManageMsg::MemberCustom(id, !same))),
                perm_chips(own.as_ref().unwrap_or(&defaults), own.is_none(), move |p| ManageMsg::MemberPerm(id, p)),
            ]
            .spacing(10),
        );
    }
    let (bg, line) = if pending { (rgba(0xf59e0b, 0.06), rgba(0xf59e0b, 0.3)) } else { (rgba(0xffffff, 0.03), rgba(0xffffff, 0.05)) };
    container(col.spacing(10)).width(Fill).padding([10, 12]).style(boxed(bg, line, 10.0)).into()
}

pub fn parse_color(css: &str) -> Color {
    u32::from_str_radix(css.trim_start_matches('#'), 16).map(hex).unwrap_or(MUTED)
}

fn members<'a>(state: &'a State, room: &'a ChatRoomItem, access: &Access, pending: &[i64]) -> El<'a> {
    let mut col = Column::new().spacing(14);
    if access.manage_members && is_group(room) {
        col = col.push(row![invite_section(state, room), add_people(state)].spacing(14));
    }
    if access.edit_permissions {
        col = col.push(everyone_section(state, room));
        col = col.push(section(
            icons::shield_check(),
            "What every member can do",
            None,
            column![
                hint("Applies to everyone you haven't given their own permissions. Roles are only labels — they don't grant anything."),
                perm_chips(&perms::default_permissions(room), false, ManageMsg::DefaultPerm)
            ]
            .spacing(10)
            .into(),
        ));
    }
    let owner = perms::owner_id(room);
    let mut list: Vec<&UserInfo> = room.allowed_users.iter().collect();
    list.sort_by_key(|u| (Some(u.id) != owner, !pending.contains(&u.id)));
    let mut rows = Column::new().spacing(6);
    for u in list {
        rows = rows.push(member_row(state, room, access, u, pending.contains(&u.id)));
    }
    if !pending.is_empty() && !access.has_key {
        rows = rows.push(hint("Members awaiting approval get the room key from someone who holds it on their device."));
    }
    col.push(section(icons::users(), "Members", Some(text(room.allowed_users.len().to_string()).size(12).font(BOLD).color(TEXT_SECONDARY).into()), rows.into())).into()
}

fn account_age(iso: Option<&str>) -> String {
    let Some(t) = iso.and_then(super::display::local) else { return "new account".into() };
    let days = (chrono::Local::now() - t).num_days();
    match days {
        d if d < 1 => "account made today".into(),
        d if d < 30 => format!("account {d} d old"),
        d if d < 365 => format!("account {} mo old", d / 30),
        d => format!("account {} y old", d / 365),
    }
}

fn requests(state: &State) -> El<'_> {
    let Some(list) = &state.requests else { return hint("Loading…") };
    if list.is_empty() {
        return container(
            column![
                icon(icons::inbox(), 32, TEXT_SECONDARY),
                text("No requests right now").size(15).font(SEMIBOLD).color(LABEL),
                text("People who ask to join through an approval-only invite link or the group's showcase card appear here.").size(12.5).color(TEXT_SECONDARY).width(360).align_x(Horizontal::Center),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .width(Fill)
        .padding([60, 20])
        .align_x(Horizontal::Center)
        .into();
    }
    let mut col = column![hint(format!("{} waiting · approving shares the room key with them", list.len()))].spacing(10);
    for r in list {
        let busy = state.deciding == Some(r.user.id);
        let via = if r.via == "view" { "from the showcase" } else { "invite link" };
        let mut meta = vec![format!("{via} · {}", r.at.as_deref().map(super::display::short_time).unwrap_or_default()), account_age(r.account_created_at.as_deref())];
        if r.shared_groups > 0 {
            meta.push(format!("{} group{} with you", r.shared_groups, if r.shared_groups == 1 { "" } else { "s" }));
        }
        let mut card = column![row![
            avatar_pic(&r.user.username, r.user.avatar.as_deref(), 36.0, None),
            column![text(format!("@{}", r.user.username)).size(13.5).font(BOLD).color(Color::WHITE), text(meta.join(" · ")).size(11.5).color(TEXT_SECONDARY)].spacing(2).width(Fill),
            button(row![icon(icons::check(), 13, Color::WHITE), text("Approve").size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center)).padding([6, 12]).style(primary).on_press_maybe((!busy).then(|| m(ManageMsg::Decide(r.user.id, true)))),
            button(row![icon(icons::x(), 13, DANGER_TEXT), text("Decline").size(12).font(SEMIBOLD).color(DANGER_TEXT)].spacing(5).align_y(Alignment::Center)).padding([6, 12]).style(tone_button(Tone::Danger)).on_press_maybe((!busy).then(|| m(ManageMsg::Decide(r.user.id, false)))),
        ]
        .spacing(12)
        .align_y(Alignment::Center)]
        .spacing(8);
        if let Some(note) = r.message.as_deref().filter(|n| !n.trim().is_empty()) {
            card = card.push(container(text(format!("“{note}”")).size(12.5).color(LABEL).line_height(1.45)).width(Fill).padding([8, 10]).style(boxed(rgba(0xffffff, 0.03), Color::TRANSPARENT, 8.0)));
        }
        col = col.push(container(card).width(Fill).padding(12).style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.07), 10.0)));
    }
    col.into()
}

fn settings<'a>(state: &'a State, room: &'a ChatRoomItem, access: &Access) -> El<'a> {
    let get = |k: &str| room.settings.as_ref().and_then(|s| s.get(k)).cloned();
    let slow = get("slow_mode_seconds").and_then(|v| v.as_i64()).unwrap_or(0);
    let burn = get("auto_burn").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "off".into());
    let burn_label = AUTO_BURN.iter().find(|(v, _)| *v == burn).map_or("Off", |(_, l)| *l);
    let gifs = get("allow_gifs_stickers").and_then(|v| v.as_bool()).unwrap_or(true);
    let roles: Vec<RoomRole> = get("roles").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();

    let line = |handle: svg::Handle, label: &'a str, control: El<'a>, note: Option<String>| -> El<'a> {
        let mut c = column![row![icon(handle, 14, INDIGO_SOFT), text(label).size(13).font(SEMIBOLD).color(TEXT_BRIGHT), space().width(Fill), control].spacing(8).align_y(Alignment::Center)].spacing(6);
        if let Some(n) = note {
            c = c.push(hint(n));
        }
        container(c).width(Fill).padding([14, 18]).style(boxed(rgba(0xffffff, 0.025), BORDER, 12.0)).into()
    };

    let slow_control: El = if access.manage_room {
        let options: Vec<Choice<i64>> = SLOW_MODE.iter().map(|(v, l)| Choice { value: *v, label: l.to_string() }).collect();
        let selected = options.iter().find(|o| o.value == slow).cloned();
        pick_list(options, selected, |c: Choice<i64>| m(ManageMsg::SlowMode(c.value))).text_size(12.5).padding([6, 10]).width(150).style(token_select).menu_style(chain_menu).into()
    } else {
        text(if slow > 0 { SLOW_MODE.iter().find(|(v, _)| *v == slow).map_or(format!("{slow}s"), |(_, l)| l.to_string()) } else { "OFF".into() }).size(12.5).font(BOLD).color(if slow > 0 { AMBER_TEXT } else { MUTED }).into()
    };
    let slow_note = (slow > 0).then(|| format!("Members must wait {} between messages. The owner is exempt.", if slow < 60 { format!("{slow}s") } else { format!("{}m", slow / 60) }));

    let burn_control: El = if access.manage_room {
        let options: Vec<Choice<String>> = AUTO_BURN.iter().map(|(v, l)| Choice { value: v.to_string(), label: l.to_string() }).collect();
        let selected = options.iter().find(|o| o.value == burn).cloned();
        pick_list(options, selected, |c: Choice<String>| m(ManageMsg::AutoBurn(c.value))).text_size(12.5).padding([6, 10]).width(150).style(token_select).menu_style(chain_menu).into()
    } else {
        text(burn_label).size(12.5).font(BOLD).color(if burn != "off" { DANGER_TEXT } else { MUTED }).into()
    };
    let burn_note = if burn != "off" {
        format!("New messages are deleted from the server {} after they're sent. Cheques and invoices are kept. People can still screenshot or copy before then.", burn_label.to_lowercase())
    } else {
        "Messages stay until someone deletes them.".into()
    };

    let gifs_control: El = if access.manage_room {
        toggler(gifs).size(20).style(switch).on_toggle(|v| m(ManageMsg::Gifs(v))).into()
    } else {
        text(if gifs { "ON" } else { "OFF" }).size(12.5).font(BOLD).color(if gifs { GREEN } else { MUTED }).into()
    };

    let mut role_list = Column::new().spacing(6);
    for r in &roles {
        let c = parse_color(&r.color);
        let mut line = row![container(text(r.name.clone()).size(12).font(BOLD).color(c)).padding([3, 10]).style(boxed(Color { a: 0.15, ..c }, Color { a: 0.5, ..c }, 6.0)), space().width(Fill)].align_y(Alignment::Center);
        if access.manage_room {
            line = line.push(button(icon(icons::trash(), 13, DANGER_TEXT)).padding(4).style(ghost(6.0, rgba(0xef4444, 0.1))).on_press(m(ManageMsg::DeleteRole(r.id.clone()))));
        }
        role_list = role_list.push(line);
    }
    let mut roles_body = column![role_list].spacing(10);
    if access.manage_room {
        let mut swatches = row![].spacing(6);
        for (i, c) in ROLE_COLORS.iter().enumerate() {
            let picked = state.new_role_color == i;
            swatches = swatches.push(
                button(space().width(18).height(18))
                    .padding(0)
                    .style(move |_, _| button::Style { background: Some(hex(*c).into()), border: border(if picked { Color::WHITE } else { Color::TRANSPARENT }, 2.0, 9.0), ..Default::default() })
                    .on_press(m(ManageMsg::RoleColor(i))),
            );
        }
        let ready = !state.new_role.trim().is_empty();
        roles_body = roles_body.push(swatches).push(
            row![
                input("New role name…", &state.new_role, ManageMsg::NewRole).on_submit(m(ManageMsg::AddRole)),
                button(row![icon(icons::plus(), 13, Color::WHITE), text("Add").size(13).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center)).padding([9, 14]).style(primary).on_press_maybe(ready.then(|| m(ManageMsg::AddRole))),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    } else if roles.is_empty() {
        roles_body = roles_body.push(hint("No roles yet."));
    }

    column![
        line(icons::timer(), "Slow mode", slow_control, slow_note),
        line(icons::flame(), "Disappearing messages", burn_control, Some(burn_note)),
        line(icons::image(), "Allow GIFs & stickers", gifs_control, None),
        section(icons::tag(), "Roles (cosmetic — no permissions yet)", None, roles_body.into()),
    ]
    .spacing(14)
    .into()
}

// ── the window ──

pub fn view<'a>(state: &'a State, room: &'a ChatRoomItem, access: Access, pending: &[i64]) -> El<'a> {
    let tab = shown_tab(state, room, &access);
    let requests_count = room.join_requests_count.unwrap_or(0);
    let nav_item = |t: Tab, handle: svg::Handle, label: &'static str, badge: Option<i64>| -> El<'a> {
        let active = tab == t;
        let mut content = row![icon(handle, 15, if active { Color::WHITE } else { MUTED }), text(label).size(13).font(SEMIBOLD)].spacing(10).align_y(Alignment::Center);
        if let Some(n) = badge.filter(|n| *n > 0) {
            content = content.push(space().width(Fill)).push(container(text(n.to_string()).size(10.5).font(EXTRABOLD).color(hex(0x111111))).padding([1, 6]).style(boxed(AMBER, Color::TRANSPARENT, 8.0)));
        }
        button(content).width(Fill).padding([9, 12]).style(selectable(active, rgba(0x6366f1, 0.22), 8.0)).on_press(m(ManageMsg::Tab(t))).into()
    };
    let mut nav = column![nav_item(Tab::General, icons::pencil(), "General", None)].spacing(4);
    if is_group(room) {
        nav = nav.push(nav_item(Tab::Members, icons::users(), "Members", None));
    }
    if can_see_requests(room, &access) {
        nav = nav.push(nav_item(Tab::Requests, icons::user_plus(), "Requests", Some(requests_count)));
    }
    nav = nav.push(nav_item(Tab::Settings, icons::sliders_horizontal(), "Settings", None));

    let side = container(
        column![
            column![
                room_avatar::view(room, access.me, 56.0),
                text(truncate(&super::display::title(room, access.me), 26)).size(15).font(BOLD).color(Color::WHITE),
                text(match room.chat_type.as_str() {
                    "dm" => "Direct message",
                    "group" => "Private group · end-to-end encrypted",
                    _ => "Public room",
                })
                .size(11.5)
                .color(TEXT_SECONDARY),
            ]
            .spacing(6),
            container(space().width(Fill).height(1)).style(rule),
            nav,
        ]
        .spacing(16),
    )
    .width(230)
    .height(Fill)
    .padding([22, 16])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let (heading, sub) = match tab {
        Tab::General => ("General", "Name, topic and how the room looks"),
        Tab::Members => ("Members", "Who's in, what they can do, invites"),
        Tab::Requests => ("Requests", "People asking to join"),
        Tab::Settings => ("Settings", "Slow mode, disappearing messages, roles"),
    };
    let header = row![
        column![text(heading).size(18).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(m(ManageMsg::Close)),
    ]
    .align_y(Alignment::Center);
    let body = match tab {
        Tab::General => general(state, room, &access),
        Tab::Members => members(state, room, &access, pending),
        Tab::Requests => requests(state),
        Tab::Settings => settings(state, room, &access),
    };
    let main = column![
        container(header).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(rule),
        scrollable(container(body).padding(Padding { top: 18.0, right: 26.0, bottom: 24.0, left: 26.0 })).height(Fill).style(thin_scroll),
    ]
    .width(Fill);

    let window = container(row![side, main]).width(Length::Fixed(980.0)).height(Length::Fixed(680.0)).style(modal);
    let mut layers: Vec<El<'a>> = vec![overlay::modal(window, m(ManageMsg::Close))];
    if let Some(c) = &state.confirm {
        layers.push(match c {
            Confirm::Kick(_, name) => confirm_box(format!("Remove @{name}?"), "They lose access to this room. The room key is rotated the next time a member who manages keys opens it, so they can't read new messages.", "Remove", true),
            Confirm::MakeChannel => confirm_box("Make it a channel?".into(), "Members stop posting in the chat — only you and the people you give \"Send messages\" to can. Everyone can still read and reply in threads.", "Make it a channel", false),
            Confirm::RevokeInvite => confirm_box("Turn off the invite link?".into(), "Nobody can join with it any more. You can create a new one at any time.", "Turn off", true),
        });
    }
    if layers.len() == 1 { layers.pop().unwrap_or_else(|| space().into()) } else { iced::widget::Stack::with_children(layers).into() }
}
