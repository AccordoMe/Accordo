// SPDX-License-Identifier: GPL-3.0-or-later
//! Someone's card (UserProfileModal.tsx), as a desktop window like Manage Room: who they are on
//! the left (banner, avatar, presence, a quick message), the details on the right — overview,
//! public wallets, what you have in common, their place in the open room, and their chat key.

use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, scrollable, space, svg, text, text_input, Column};
use iced::{Alignment, Color, Fill, Length, Padding};

use super::display;
use super::manage_room::parse_color;
use crate::api::chat::{ChatRoomItem, RoomRole, UserInfo, UserRelationshipStats};
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{avatar_pic, icon, El};
use crate::e2ee::crypto::fingerprint;
use crate::e2ee::trust::KeyTrust;
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::store::room_permissions as perms;
use crate::theme::*;

pub const QUICK_INPUT: &str = "profile-quick";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Wallets,
    Common,
    Room,
    Security,
}

#[derive(Debug, Clone)]
pub enum ProfileMsg {
    Tab(Tab),
    Close,
    Quick(String),
    SendQuick,
    CopyId,
    CopyPayLink,
    OpenPayLink,
    CopyAddress(String),
    OpenRoom(String),
    Cheque,
    Invoice,
    EditProfile,
    Permissions,
    Remove,
}

pub struct State {
    pub user: UserInfo,
    /// The full profile (wallets, join date), once loaded.
    pub loaded: bool,
    pub relationship: Option<UserRelationshipStats>,
    pub tab: Tab,
    pub quick: String,
    /// The room it was opened from (its role, permissions).
    pub room_key: Option<String>,
}

impl State {
    pub fn new(user: UserInfo, room_key: Option<String>) -> Self {
        Self { user, loaded: false, relationship: None, tab: Tab::Overview, quick: String::new(), room_key }
    }
}

/// What the card needs from around it.
pub struct Context<'a> {
    pub me: i64,
    pub is_admin: bool,
    pub room: Option<&'a ChatRoomItem>,
    pub pending: bool,
    /// How this device sees their chat key.
    pub trust: Option<KeyTrust>,
}

fn m(msg: ProfileMsg) -> Message {
    Message::Chat(ChatMsg::Profile(msg))
}

fn name_of(u: &UserInfo) -> String {
    if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() }
}

/// The banner's colours, picked from the username (the web's getGradient palettes).
fn banner(seed: &str) -> (Color, Color) {
    const PALETTES: [(u32, u32); 5] = [(0x6366f1, 0xec4899), (0x0ea5e9, 0x6366f1), (0x10b981, 0x3b82f6), (0xf59e0b, 0xec4899), (0x8b5cf6, 0x38bdf8)];
    let mut hash: i32 = 0;
    for c in seed.chars() {
        hash = (c as i32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }
    let (a, b) = PALETTES[(hash.unsigned_abs() as usize) % PALETTES.len()];
    (hex(a), hex(b))
}

fn section<'a>(handle: svg::Handle, title: &'a str, body: El<'a>) -> El<'a> {
    container(column![row![icon(handle, 14, INDIGO_SOFT), text(title).size(13).font(BOLD).color(TEXT_BRIGHT)].spacing(7).align_y(Alignment::Center), body].spacing(12))
        .width(Fill)
        .padding([16, 18])
        .style(boxed(rgba(0xffffff, 0.025), BORDER, 12.0))
        .into()
}

fn line<'a>(label: &'a str, value: El<'a>) -> El<'a> {
    row![text(label).size(11.5).font(BOLD).color(MUTED).width(130), container(value).width(Fill).align_x(iced::alignment::Horizontal::Right)].align_y(Alignment::Center).into()
}

fn chip<'a>(content: El<'a>, on: Option<ProfileMsg>) -> El<'a> {
    button(content).padding([4, 9]).style(outline_indigo).on_press_maybe(on.map(m)).into()
}

fn hint(s: impl Into<String>) -> El<'static> {
    text(s.into()).size(12).color(TEXT_SECONDARY).line_height(1.45).into()
}

fn action<'a>(handle: svg::Handle, tint: Color, label: &'a str, sub: &'a str, on: ProfileMsg, danger: bool) -> El<'a> {
    button(
        row![
            container(center(icon(handle, 17, tint))).width(36).height(36).style(boxed(Color { a: 0.14, ..tint }, Color::TRANSPARENT, 10.0)),
            column![text(label).size(13).font(SEMIBOLD).color(if danger { DANGER_TEXT } else { Color::WHITE }), text(sub).size(11.5).color(TEXT_SECONDARY)].spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([10, 12])
    .style(move |_, s| button::Style {
        background: Some(if matches!(s, button::Status::Hovered) { if danger { rgba(0xef4444, 0.1) } else { rgba(0xffffff, 0.06) } } else { rgba(0xffffff, 0.03) }.into()),
        border: border(if danger { rgba(0xef4444, 0.3) } else { BORDER }, 1.0, 10.0),
        ..Default::default()
    })
    .on_press(m(on))
    .into()
}

fn overview<'a>(s: &'a State, cx: &Context<'a>) -> El<'a> {
    let u = &s.user;
    let own = u.id == cx.me;
    let pay = format!("/pay/{}", u.username);
    let mut about = column![
        line("USER ID", chip(row![icon(icons::copy(), 11, INDIGO_SOFT), text(format!("#{}", u.id)).size(11.5).font(BOLD)].spacing(5).align_y(Alignment::Center).into(), Some(ProfileMsg::CopyId))),
        line(
            "PAY LINK",
            row![
                chip(row![icon(icons::link_2(), 11, INDIGO_SOFT), text(pay).size(11.5).font(BOLD)].spacing(5).align_y(Alignment::Center).into(), Some(ProfileMsg::CopyPayLink)),
                chip(icon(icons::arrow_up_right(), 11, INDIGO_SOFT).into(), Some(ProfileMsg::OpenPayLink)),
            ]
            .spacing(4)
            .into()
        ),
    ]
    .spacing(12);
    if let Some(joined) = u.created_at.as_deref().and_then(display::local) {
        about = about.push(line("MEMBER SINCE", text(joined.format("%B %-d, %Y").to_string()).size(12.5).color(TEXT_BODY).into()));
    }
    if !u.orgs.is_empty() {
        let list = u.orgs.iter().fold(column![].spacing(6).align_x(iced::alignment::Horizontal::Right), |c, o| c.push(crate::components::ui::org_chip(o)));
        about = about.push(line(if u.orgs.len() == 1 { "ORGANIZATION" } else { "ORGANIZATIONS" }, list.into()));
    }
    if let Some(role) = u.role.as_deref().filter(|r| !r.is_empty()) {
        about = about.push(line("ROLE", text(role.to_uppercase()).size(11.5).font(BOLD).color(AMBER_TEXT).into()));
    }
    let mut col = column![section(icons::user(), "About", about.into())].spacing(14);
    let actions: El = if own {
        column![action(icons::settings(), INDIGO_SOFT, "Edit profile", "Name, avatar, email — in Settings", ProfileMsg::EditProfile, false)].into()
    } else {
        column![
            row![
                action(icons::wallet(), AMBER_TEXT, "Write a cheque", "Send crypto they can claim", ProfileMsg::Cheque, false),
                action(icons::receipt(), GREEN, "Issue an invoice", "Ask them to pay you", ProfileMsg::Invoice, false),
            ]
            .spacing(10),
        ]
        .into()
    };
    col = col.push(section(icons::send(), if own { "Your profile" } else { "Payments" }, actions));
    if !s.loaded {
        col = col.push(hint("Loading the full profile…"));
    }
    col.into()
}

fn wallets(s: &State) -> El<'_> {
    let list = s.user.wallets.clone().unwrap_or_default();
    if list.is_empty() {
        return section(icons::wallet(), "Public wallets", hint(if s.loaded { "No public wallets." } else { "Loading…" }));
    }
    let mut col = Column::new().spacing(8);
    for w in list {
        let mut head = row![text(w.name.clone().unwrap_or_else(|| "Wallet".into())).size(13).font(SEMIBOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center);
        if let Some(chain) = w.chain.clone().filter(|c| !c.is_empty()) {
            head = head.push(container(text(chain.to_uppercase()).size(9.5).font(BOLD).color(SKY)).padding([1, 6]).style(boxed(rgba(0x38bdf8, 0.15), Color::TRANSPARENT, 4.0)));
        }
        if w.is_default == Some(true) {
            head = head.push(container(text("DEFAULT").size(9.5).font(BOLD).color(GREEN)).padding([1, 6]).style(boxed(rgba(0x34d399, 0.15), Color::TRANSPARENT, 4.0)));
        }
        col = col.push(
            container(
                row![
                    column![head, text(w.address.clone()).size(12).font(iced::Font::MONOSPACE).color(TEXT_BODY).wrapping(Wrapping::None)].spacing(4).width(Fill),
                    chip(row![icon(icons::copy(), 11, INDIGO_SOFT), text("Copy").size(11.5).font(BOLD)].spacing(5).align_y(Alignment::Center).into(), Some(ProfileMsg::CopyAddress(w.address.clone()))),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([10, 12])
            .style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.06), 10.0)),
        );
    }
    section(icons::wallet(), "Public wallets", col.into())
}

fn common<'a>(s: &'a State, cx: &Context<'a>) -> El<'a> {
    if s.user.id == cx.me {
        return section(icons::users(), "In common", hint("This is you."));
    }
    let Some(r) = &s.relationship else { return section(icons::users(), "In common", hint("Loading…")) };
    let mut rooms = Column::new().spacing(6);
    if r.shared_rooms.is_empty() {
        rooms = rooms.push(hint("No shared rooms."));
    }
    for room in &r.shared_rooms {
        let kind = match room.chat_type.as_str() {
            "dm" => icons::message_circle(),
            "public" => icons::hash(),
            _ => icons::users(),
        };
        rooms = rooms.push(
            button(row![icon(kind, 14, INDIGO_SOFT), text(room.title.clone().unwrap_or_else(|| room.room_key.clone())).size(13).color(TEXT_BODY).width(Fill), icon(icons::arrow_up_right(), 12, FAINT)].spacing(10).align_y(Alignment::Center))
                .width(Fill)
                .padding([8, 10])
                .style(ghost(8.0, rgba(0x6366f1, 0.15)))
                .on_press(m(ProfileMsg::OpenRoom(room.room_key.clone()))),
        );
    }
    let mut money = column![row![
        stat(r.cheques_exchanged, "cheques exchanged", AMBER_TEXT),
        stat(r.invoices_exchanged, "invoices exchanged", GREEN),
    ]
    .spacing(10)]
    .spacing(10);
    if let Some(t) = &r.most_recent_transaction {
        money = money.push(hint(format!(
            "Last: {} · {} {} · {} · {}",
            t.kind,
            t.amount.map(|a| a.to_string()).unwrap_or_else(|| "?".into()),
            t.token.clone().unwrap_or_default(),
            t.status,
            display::stamp(&t.created_at)
        )));
    }
    column![
        section(icons::users(), if r.shared_room_count == 1 { "1 shared room" } else { "Shared rooms" }, rooms.into()),
        section(icons::receipt(), "Payments between you", money.into()),
    ]
    .spacing(14)
    .into()
}

fn stat<'a>(n: i64, label: &'a str, color: Color) -> El<'a> {
    container(column![text(n.to_string()).size(22).font(BOLD).color(color), text(label).size(11.5).color(TEXT_SECONDARY)].spacing(2))
        .width(Fill)
        .padding([12, 14])
        .style(boxed(rgba(0xffffff, 0.03), rgba(0xffffff, 0.06), 10.0))
        .into()
}

fn this_room<'a>(s: &'a State, cx: &Context<'a>) -> El<'a> {
    let Some(room) = cx.room else { return hint("Open their card from a room to see their place in it.") };
    let u = &s.user;
    let owner = perms::owner_id(room) == Some(u.id) && room.chat_type != "dm";
    let roles: Vec<RoomRole> = room.settings.as_ref().and_then(|x| x.get("roles")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let role = room
        .settings
        .as_ref()
        .and_then(|x| x.get("member_roles"))
        .and_then(|v| v.get(u.id.to_string()))
        .and_then(|v| v.as_str())
        .and_then(|id| roles.iter().find(|r| r.id == id));
    let mut info = column![line("ROOM", text(display::title(room, cx.me)).size(12.5).color(TEXT_BODY).into())].spacing(12);
    info = info.push(line(
        "STATUS",
        if owner {
            text("OWNER").size(11.5).font(BOLD).color(AMBER_TEXT).into()
        } else if cx.pending {
            text("AWAITING APPROVAL").size(11.5).font(BOLD).color(AMBER_TEXT).into()
        } else if room.allowed_user_ids.contains(&u.id) || room.chat_type == "public" {
            text("Member").size(12.5).color(TEXT_BODY).into()
        } else {
            text("Not in this room").size(12.5).color(MUTED).into()
        },
    ));
    if let Some(r) = role {
        let c = parse_color(&r.color);
        info = info.push(line("ROLE", container(text(r.name.clone()).size(11.5).font(BOLD).color(c)).padding([2, 8]).style(boxed(Color { a: 0.15, ..c }, Color::TRANSPARENT, 5.0)).into()));
    }
    let can = perms::effective(room, u.id, false);
    let mut chips = iced::widget::Row::new().spacing(6);
    for p in perms::PERMISSIONS.iter().filter(|p| can.contains(&p.perm)) {
        chips = chips.push(container(text(p.label).size(11).font(SEMIBOLD).color(LABEL)).padding([4, 9]).style(boxed(rgba(0x6366f1, 0.12), rgba(0x6366f1, 0.3), 10.0)));
    }
    let mut col = column![
        section(icons::users(), "In this room", info.into()),
        section(icons::shield_check(), if owner { "Can do everything" } else { "Can" }, if can.is_empty() { hint("Only read.") } else { chips.wrap().vertical_spacing(6).into() }),
    ]
    .spacing(14);
    let mut acts = Column::new().spacing(8);
    if perms::can_edit_permissions(room, cx.me, cx.is_admin) && !owner && room.chat_type == "group" && u.id != cx.me {
        acts = acts.push(action(icons::shield_check(), INDIGO_SOFT, "Permissions", "What they may do here", ProfileMsg::Permissions, false));
    }
    if perms::can_remove(room, cx.me, cx.is_admin, u.id) {
        acts = acts.push(action(icons::user_minus(), DANGER_TEXT, "Remove from room", "They lose access; the room key is rotated", ProfileMsg::Remove, true));
    }
    col = col.push(acts);
    col.into()
}

fn security<'a>(s: &'a State, cx: &Context<'a>) -> El<'a> {
    let Some(key) = s.user.e2ee_public_key.as_deref().filter(|k| !k.is_empty()) else {
        return section(icons::lock(), "Chat key", hint("They haven't set up encrypted chats yet — DMs and groups with them can't be encrypted for them until they do."));
    };
    let fp = fingerprint(key);
    let groups: Vec<&str> = fp.split(' ').collect();
    let mut grid = Column::new().spacing(6);
    for chunk in groups.chunks(4) {
        let mut r = row![].spacing(10);
        for g in chunk {
            r = r.push(text(g.to_string()).size(15).font(iced::Font::MONOSPACE).color(TEXT_BRIGHT));
        }
        grid = grid.push(r);
    }
    let (label, color, note) = match cx.trust {
        _ if s.user.id == cx.me => ("THIS COMPUTER'S KEY", GREEN, "Your chat key. Others compare this number with what their app shows for you."),
        Some(KeyTrust::Verified) => ("VERIFIED", GREEN, "You compared this number with them. Messages to them are sealed for this key only."),
        Some(KeyTrust::Trusted) | Some(KeyTrust::New) => ("TRUSTED ON FIRST USE", INDIGO_SOFT, "The key this computer first saw for them. Compare the number with them (in person or another channel) to be sure."),
        Some(KeyTrust::Changed) => ("KEY CHANGED", DANGER_TEXT, "Their key differs from the one this computer knew. Until you accept it (Room Security in the web app), room keys aren't shared with it."),
        None => ("NOT SEEN YET", MUTED, "This computer hasn't used their key yet; it's pinned the first time you share a chat key with them."),
    };
    section(
        icons::lock(),
        "Chat key",
        column![
            container(text(label).size(10.5).font(BOLD).color(color)).padding([2, 8]).style(boxed(Color { a: 0.14, ..color }, Color::TRANSPARENT, 5.0)),
            container(grid).padding([12, 14]).style(boxed(hex(0x0e0f17), BORDER, 10.0)),
            hint(note),
        ]
        .spacing(12)
        .into(),
    )
}

pub fn view<'a>(s: &'a State, cx: Context<'a>) -> El<'a> {
    let u = &s.user;
    let own = u.id == cx.me;
    let name = name_of(u);
    let (presence, online) = display::presence(u);
    let (a, b) = banner(&u.username);

    let face = container(avatar_pic(&name, display::avatar_of(u), 84.0, None)).padding(5).style(round_fill(hex(0x0f1016).into(), 48.0));
    let dot = container(space().width(16).height(16)).style(move |_| container::Style {
        background: Some(if online { hex(0x23a55a) } else { hex(0x80848e) }.into()),
        border: border(hex(0x0f1016), 3.0, 9.0),
        ..Default::default()
    });
    let face = iced::widget::stack![face, container(dot).width(94).height(94).align_x(iced::alignment::Horizontal::Right).align_y(iced::alignment::Vertical::Bottom).padding(6)];

    let mut title = row![text(name.clone()).size(19).font(EXTRABOLD).color(TEXT_BRIGHT)].spacing(6).align_y(Alignment::Center);
    if u.verified == Some(true) {
        title = title.push(icon(icons::shield_check(), 16, SKY));
    }
    let status = row![
        text(format!("@{}", u.username)).size(12.5).color(MUTED),
        text("•").size(12).color(rgba(0xffffff, 0.2)),
        text(if online { "Online".to_string() } else { presence.replace("last seen", "Last seen") }).size(12.5).color(if online { hex(0x23a55a) } else { MUTED }),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let tab = if s.tab == Tab::Room && cx.room.is_none() { Tab::Overview } else { s.tab };
    let nav_item = |t: Tab, handle: svg::Handle, label: &'static str| -> El<'a> {
        let active = tab == t;
        button(row![icon(handle, 15, if active { Color::WHITE } else { MUTED }), text(label).size(13).font(SEMIBOLD)].spacing(10).align_y(Alignment::Center))
            .width(Fill)
            .padding([9, 12])
            .style(selectable(active, rgba(0x6366f1, 0.22), 8.0))
            .on_press(m(ProfileMsg::Tab(t)))
            .into()
    };
    let mut nav = column![nav_item(Tab::Overview, icons::user(), "Overview"), nav_item(Tab::Wallets, icons::wallet(), "Wallets")].spacing(4);
    if !own {
        nav = nav.push(nav_item(Tab::Common, icons::users(), "In common"));
    }
    if cx.room.is_some() {
        nav = nav.push(nav_item(Tab::Room, icons::hash(), "This room"));
    }
    nav = nav.push(nav_item(Tab::Security, icons::lock(), "Chat key"));

    let quick: El = if own {
        space().height(0).into()
    } else {
        let ready = !s.quick.trim().is_empty();
        container(
            row![
                text_input(&format!("Message @{}…", u.username), &s.quick).id(QUICK_INPUT).on_input(|v| m(ProfileMsg::Quick(v))).on_submit(m(ProfileMsg::SendQuick)).size(13).padding([8, 4]).style(bare_input),
                button(center(icon(icons::send(), 13, Color::WHITE))).width(30).height(30).padding(0).style(primary).on_press(m(ProfileMsg::SendQuick)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([3, 4])
        .style(boxed(hex(0x15161c), if ready { rgba(0x6366f1, 0.5) } else { BORDER }, 10.0))
        .into()
    };

    let side = container(
        column![
            iced::widget::stack![
                column![
                    container(space().width(Fill).height(96)).style(move |_| container::Style {
                        background: Some(gradient(a, b)),
                        border: iced::Border { radius: iced::border::Radius { top_left: 14.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 0.0 }, ..Default::default() },
                        ..Default::default()
                    }),
                    space().height(48),
                ],
                container(face).padding(Padding { top: 46.0, right: 0.0, bottom: 0.0, left: 16.0 }),
            ],
            container(column![title, status].spacing(4)).padding([4, 18]),
            container(container(space().width(Fill).height(1)).style(rule)).padding([10, 18]),
            container(nav).padding([0, 12]),
            space().height(Fill),
            container(quick).padding([14, 14]),
        ]
        .height(Fill),
    )
    .width(270)
    .height(Fill)
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let (heading, sub) = match tab {
        Tab::Overview => ("Overview", "Who they are and what you can do"),
        Tab::Wallets => ("Wallets", "Addresses they share publicly"),
        Tab::Common => ("In common", "Rooms and payments you share"),
        Tab::Room => ("This room", "Their role and permissions here"),
        Tab::Security => ("Chat key", "Compare it to be sure your chats are only theirs"),
    };
    let header = row![
        column![text(heading).size(18).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(m(ProfileMsg::Close)),
    ]
    .align_y(Alignment::Center);
    let body = match tab {
        Tab::Overview => overview(s, &cx),
        Tab::Wallets => wallets(s),
        Tab::Common => common(s, &cx),
        Tab::Room => this_room(s, &cx),
        Tab::Security => security(s, &cx),
    };
    let main = column![
        container(header).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(rule),
        scrollable(container(body).padding(Padding { top: 18.0, right: 26.0, bottom: 24.0, left: 26.0 })).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    let window = container(row![side, main]).width(Length::Fixed(900.0)).height(Length::Fixed(600.0)).style(modal);
    overlay::modal(window, m(ProfileMsg::Close))
}
