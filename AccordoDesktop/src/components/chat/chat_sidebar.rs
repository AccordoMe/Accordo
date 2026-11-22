// SPDX-License-Identifier: GPL-3.0-or-later
//! The chat list: toolbar, filters, public chats / groups / direct messages (ChatSidebar.tsx).

use iced::widget::text::Wrapping;
use iced::widget::{button, center, column, container, row, scrollable, space, svg, text, Column};
use iced::{Alignment, Color, Fill, Length, Padding};

use super::display::{self, RoomKind};
use super::room_avatar;
use crate::api::chat::ChatRoomItem;
use crate::app::Message;
use crate::components::chat::new_chat_modal::Kind;
use crate::components::ui::{count_badge, icon, truncate, vline, El};
use crate::icons;
use crate::pages::chat_page::{ChatMsg, Filter, State};
use crate::theme::{self, *};

pub fn view(state: &State) -> El<'_> {
    // Toolbar buttons sized for the desktop window.
    let tool = |handle: svg::Handle, message: Message, hint: &'static str| -> El<'_> {
        iced::widget::tooltip(
            button(center(icon(handle, 17, INDIGO_SOFT))).width(36).height(36).padding(0).style(framed).on_press(message),
            container(text(hint).size(11.5).color(TEXT_BODY)).padding([4, 8]).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
            iced::widget::tooltip::Position::Bottom,
        )
        .into()
    };
    let toolbar = row![
        tool(icons::panel_left_close(), Message::Chat(ChatMsg::CloseRoom), "Close chat"),
        tool(icons::search(), Message::NotYet("Searching chats"), "Search"),
        tool(icons::ellipsis(), Message::NotYet("Chat list options"), "More"),
        tool(icons::link_2(), Message::Chat(ChatMsg::JoinInvite(crate::components::chat::join_invite::JoinMsg::Open)), "Join with a link"),
        tool(icons::plus(), Message::Chat(ChatMsg::OpenNewChat(Kind::Dm)), "New chat"),
    ]
    .spacing(8);

    let pill = |label: &'static str, filter: Filter| -> El<'_> {
        button(text(label).size(12.5).font(SEMIBOLD))
            .padding([6, 13])
            .style(theme::pill(state.filter == filter))
            .on_press(Message::Chat(ChatMsg::Filter(filter)))
            .into()
    };
    let filters = row![pill("All", Filter::All), pill("Unread", Filter::Unread), pill("DMs", Filter::Dms), pill("Groups", Filter::Groups)].spacing(7);

    let mut list = Column::new().spacing(1);
    if !state.rooms.loaded {
        list = list.push(container(text("Loading chats…").size(12).color(TEXT_SECONDARY)).padding([16, 10]));
    } else {
        let suggested = state.visible_rooms(RoomKind::Suggested);
        if !suggested.is_empty() {
            list = list.push(container(text("SUGGESTED").size(11).font(BOLD).color(hex(0xfbbf24))).padding(Padding { top: 12.0, right: 8.0, bottom: 4.0, left: 8.0 }));
            for r in suggested {
                list = list.push(room_row(state, r));
            }
        }
        let publics = state.visible_rooms(RoomKind::Public);
        if !publics.is_empty() || state.filter == Filter::All {
            let browse = button(row![icon(icons::compass(), 12, INDIGO_SOFT), text("Browse").size(11).font(BOLD).color(INDIGO_SOFT)].spacing(4).align_y(Alignment::Center))
                .padding([2, 4])
                .style(ghost(5.0, rgba(0x6366f1, 0.12)))
                .on_press(Message::Chat(ChatMsg::OpenBrowse));
            list = list.push(section("PUBLIC CHATS", Some(browse.into())));
            for r in publics {
                list = list.push(room_row(state, r));
            }
        }
        if state.filter != Filter::Dms {
            let groups = state.visible_rooms(RoomKind::Group);
            list = list.push(section("GROUPS", None));
            if groups.is_empty() {
                list = list.push(hint(icons::users(), "Create a group", Kind::Group));
            }
            for r in groups {
                list = list.push(room_row(state, r));
            }
        }
        if state.filter != Filter::Groups {
            let dms = state.visible_rooms(RoomKind::Dm);
            list = list.push(section("DIRECT MESSAGES", None));
            if dms.is_empty() {
                list = list.push(hint(icons::message_circle_plus(), "Start a conversation", Kind::Dm));
            }
            for r in dms {
                list = list.push(room_row(state, r));
            }
        }
    }

    row![
        container(column![
            container(toolbar).padding([12, 12]),
            container(filters).padding(Padding { top: 0.0, right: 12.0, bottom: 6.0, left: 12.0 }),
            scrollable(container(list).padding([0, 8])).height(Fill).style(thin_scroll),
        ])
        .width(284)
        .height(Fill)
        .style(panel(LIST)),
        vline(),
    ]
    .into()
}

fn room_row<'a>(state: &'a State, room: &'a ChatRoomItem) -> El<'a> {
    let selected = state.selected.as_deref() == Some(room.room_key.as_str());
    let unread = state.unread(&room.room_key);
    let has_unread = unread.unread_count > 0;

    let badge: El = if unread.mention_count > 0 {
        count_badge(unread.unread_count.max(unread.mention_count) as u32, RED)
    } else if has_unread {
        count_badge(unread.unread_count as u32, rgba(0xffffff, 0.2))
    } else {
        space().into()
    };
    let time = room.last_timestamp.as_deref().map(display::short_time).unwrap_or_default();

    let mut title_line = row![text(display::title(room, state.me_id)).size(13).font(BOLD).color(if selected || has_unread { Color::WHITE } else { TEXT }).wrapping(Wrapping::None)]
        .spacing(5)
        .align_y(Alignment::Center);
    let teaser = display::is_teaser(room);
    if teaser {
        title_line = title_line.push(icon(icons::lock(), 9, hex(0xfbbf24)));
    } else if display::is_encrypted_room(room) {
        title_line = title_line.push(icon(icons::lock(), 9, GREEN));
    }
    title_line = title_line.push(space().width(Fill));
    title_line = match room.teaser.as_ref().filter(|_| teaser) {
        // Member count, an icon and no pill: never read as an unread badge.
        Some(t) => title_line.push(row![icon(icons::lock(), 10, FAINT), text(t.members.to_string()).size(10.5).color(FAINT)].spacing(2).align_y(Alignment::Center)),
        None => title_line.push(text(time).size(10.5).color(FAINT)),
    };

    let preview_color = if has_unread { hex(0xc4c9d4) } else { TEXT_SECONDARY };
    let content = row![
        room_avatar::view(room, state.me_id, 30.0),
        column![
            title_line,
            row![
                if teaser {
                    // How alive it is, not what's said.
                    let (hint, sent) = crate::components::chat::group_teaser::row_hint(room);
                    container(text(truncate(&hint, 80)).size(11).color(if sent { GREEN } else { hex(0x7c8595) }).wrapping(Wrapping::None)).width(Fill).clip(true)
                } else if state.blur_previews {
                    // Privacy: a soft bar where the last message would be.
                    container(container(space().width(Fill).height(7)).width(Length::FillPortion(3)).style(|_| container::Style { background: Some(rgba(0xffffff, 0.07).into()), border: iced::Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() }))
                        .width(Fill)
                        .padding([4, 0])
                } else {
                    container(text(truncate(&display::preview(room), 80)).size(11.5).color(preview_color).wrapping(Wrapping::None)).width(Fill).clip(true)
                },
                badge,
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(2)
        .width(Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    button(content)
        .width(Fill)
        .padding([8, 10])
        .style(selectable(selected, rgba(0x6366f1, 0.2), 8.0))
        .on_press(Message::Chat(ChatMsg::OpenRoom(room.room_key.clone())))
        .into()
}

fn section<'a>(label: &'a str, trailing: Option<El<'a>>) -> El<'a> {
    let mut line = row![text(label).size(11).font(BOLD).color(TEXT_SECONDARY), space().width(Fill)].align_y(Alignment::Center);
    if let Some(t) = trailing {
        line = line.push(t);
    }
    container(line).padding(Padding { top: 12.0, right: 8.0, bottom: 4.0, left: 8.0 }).into()
}

/// Dashed call-to-action shown when a section is empty.
fn hint<'a>(handle: svg::Handle, label: &'a str, kind: Kind) -> El<'a> {
    button(row![icon(handle, 14, INDIGO_SOFT), text(label).size(12.5).font(SEMIBOLD).color(INDIGO_SOFT)].spacing(8).align_y(Alignment::Center))
        .width(Fill)
        .padding([8, 10])
        .style(|t, _| {
            let c = dashed_hint(t);
            button::Style { background: c.background, text_color: INDIGO_SOFT, border: c.border, ..Default::default() }
        })
        .on_press(Message::Chat(ChatMsg::OpenNewChat(kind)))
        .into()
}
