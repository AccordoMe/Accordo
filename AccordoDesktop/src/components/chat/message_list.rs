// SPDX-License-Identifier: GPL-3.0-or-later
//! The messages of a room, grouped by author and day like the web chat — and what you can do with
//! one (ChatPage.tsx's hover action bar, MessageContextMenu.tsx): react, reply, reply in a
//! thread, pin, copy, edit, delete. Hover a message for the bar, right-click it for the menu.

use iced::widget::{button, center, container, row, scrollable, space, svg, text, Column, Row};
use iced::{Alignment, Color, Fill, Padding, Task};

use super::display;
use crate::api::chat::{ChatMessageItem, UserInfo};
use crate::app::Message;
use crate::components::hover::Hover;
use crate::components::ui::{avatar_pic, icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::{self, *};

/// Consecutive messages of one author within this many minutes share a header.
const GROUP_MINUTES: i64 = 5;
/// One-tap reactions (components/chat/emojis.ts).
pub const QUICK_REACTIONS: [&str; 8] = ["🔥", "👍", "❤️", "😂", "🚀", "💎", "🎉", "👀"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// Right-click menu.
    Context,
    /// "Add reaction" picker.
    React,
}

/// What the viewer may do with messages here.
#[derive(Clone, Copy)]
pub struct Rights {
    pub me: i64,
    pub pin: bool,
    /// The room's creator deletes anyone's messages (except payments).
    pub moderate: bool,
    /// Threads are offered (not inside a thread).
    pub threads: bool,
    /// Can write here (reply, react).
    pub write: bool,
}

pub struct Feed<'a> {
    pub messages: Option<&'a [ChatMessageItem]>,
    pub loading: bool,
    pub rights: Rights,
    pub menu: Option<(i64, MenuKind)>,
    /// Members, for the avatars of who replied in a thread.
    pub members: &'a [UserInfo],
    /// A message just jumped to: highlighted.
    pub flash: Option<i64>,
    /// Decoded pictures, and the animation clock (ms).
    pub media: &'a crate::media::Cache,
    pub clock: u64,
    /// The emoji picker's state (reactions use the full picker).
    pub emoji: &'a super::emoji_picker::EmojiUi,
    /// Cheque cards: their loaded state, explorers by network, the viewer's wallets.
    pub cheques: &'a std::collections::HashMap<String, super::cheque_card::ChequeView>,
    pub invoices: &'a std::collections::HashMap<String, super::invoice_card::InvoiceView>,
    /// The viewer's username.
    pub me_name: &'a str,
    pub fund: &'a super::fund_cards::Cards,
    pub explorers: &'a std::collections::HashMap<String, String>,
    pub wallets: &'a [(String, String)],
    /// Privacy: messages are veiled until hovered.
    pub conceal: bool,
    /// Polls on screen, by message id (None: not here, e.g. a thread).
    pub polls: Option<&'a std::collections::HashMap<i64, super::poll_card::PollView>>,
    /// The room is end-to-end encrypted.
    pub private: bool,
    /// The voice message / video note playing now.
    pub playing: Option<&'a super::media_cards::Playing>,
}

fn msg(m: ChatMsg) -> Message {
    Message::Chat(m)
}

pub fn is_financial(m: &ChatMessageItem) -> bool {
    matches!(m.message_type.as_str(), "cheque" | "invoice")
        || m.attachment_data.as_ref().is_some_and(|a| {
            matches!(a.get("type").and_then(|t| t.as_str()), Some("cheque" | "invoice")) || a.get("cheque").is_some() || a.get("invoice").is_some()
        })
}

pub fn can_edit(m: &ChatMessageItem, r: &Rights) -> bool {
    m.sender_id == Some(r.me) && (m.message_type.is_empty() || m.message_type == "text") && !is_financial(m) && !display::looks_encrypted(&m.content)
}

pub fn can_delete(m: &ChatMessageItem, r: &Rights) -> bool {
    m.message_type != "deleted" && !is_financial(m) && (m.sender_id == Some(r.me) || r.moderate)
}

/// The id of the message being jumped to (the one `Feed::flash` marks).
const JUMP_TARGET: &str = "jump-target";

/// Scrolls the chat so the message being jumped to sits in the middle — found by where it really
/// is, not guessed from its place in the list (pictures and cards make rows of every height). The
/// feed is anchored at the bottom, and iced counts its offsets from there.
pub fn scroll_to_flashed() -> Task<Message> {
    use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, Scrollable};
    use iced::advanced::widget::operation::{Operation, Outcome};
    use iced::advanced::widget::Id;
    use iced::{Rectangle, Vector};

    #[derive(Default)]
    struct Find {
        feed: Option<(Rectangle, Rectangle)>,
        target: Option<Rectangle>,
    }
    impl Operation for Find {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(&mut self, id: Option<&Id>, bounds: Rectangle, content: Rectangle, _: Vector, _: &mut dyn Scrollable) {
            if id == Some(&Id::from(crate::pages::chat_page::FEED)) {
                self.feed = Some((bounds, content));
            }
        }
        fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
            if id == Some(&Id::from(JUMP_TARGET)) {
                self.target = Some(bounds);
            }
        }
        fn finish(&self) -> Outcome<()> {
            let (Some((view, content)), Some(target)) = (self.feed, self.target) else { return Outcome::None };
            let room = (content.height - view.height).max(0.0);
            let from_top = (target.y - content.y - (view.height - target.height).max(0.0) / 2.0).clamp(0.0, room);
            Outcome::Chain(Box::new(Scroll { from_bottom: room - from_top }))
        }
    }
    struct Scroll {
        from_bottom: f32,
    }
    impl Operation for Scroll {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(&mut self, id: Option<&Id>, _: Rectangle, _: Rectangle, _: Vector, state: &mut dyn Scrollable) {
            if id == Some(&Id::from(crate::pages::chat_page::FEED)) {
                state.scroll_to(AbsoluteOffset { x: None, y: Some(self.from_bottom) });
            }
        }
    }
    // A moment first: the feed lays out the message (and any history put in front of it).
    Task::perform(tokio::time::sleep(std::time::Duration::from_millis(40)), |_| ()).then(|()| iced::advanced::widget::operate(Find::default())).discard()
}

pub fn view<'a>(feed: Feed<'a>) -> El<'a> {
    let Some(list) = feed.messages.filter(|f| !f.is_empty()) else {
        let note = if feed.loading || feed.messages.is_none() { "Loading messages…" } else { "No messages yet — say hello" };
        return center(text(note).size(13).color(TEXT_SECONDARY)).into();
    };

    let mut out = Column::new();
    let mut i = 0;
    let mut last_day = None;
    while i < list.len() {
        let first = &list[i];
        let first_at = display::local(&first.created_at);
        let day = first_at.map(|t| t.date_naive());
        if day != last_day {
            if let Some(t) = first_at {
                out = out.push(container(day_divider(display::day_label(&t))).padding([12, 0]));
            }
            last_day = day;
        }
        let same_author = |m: &ChatMessageItem| m.sender_id == first.sender_id && m.sender_username == first.sender_username;
        let mut end = i + 1;
        while end < list.len() && same_author(&list[end]) && list[end].reply_to_data.is_none() {
            let (Some(a), Some(b)) = (display::local(&list[end - 1].created_at), display::local(&list[end].created_at)) else { break };
            if b.date_naive() != a.date_naive() || (b - a).num_minutes() > GROUP_MINUTES {
                break;
            }
            end += 1;
        }
        for (k, m) in list[i..end].iter().enumerate() {
            let row = message(m, k == 0, &feed);
            // The message being jumped to carries an id, so `scroll_to_flashed` finds where it is.
            out = out.push(if feed.flash == Some(m.id) { container(row).id(JUMP_TARGET).into() } else { row });
        }
        i = end;
    }
    scrollable(container(out).padding(Padding { top: 12.0, right: 14.0, bottom: 16.0, left: 8.0 }).width(Fill)).id(crate::pages::chat_page::FEED).anchor_bottom().height(Fill).style(thin_scroll).into()
}

/// One message: its line(s), and the floating bar and menu that go with it.
pub fn message<'a>(m: &'a ChatMessageItem, head: bool, feed: &Feed<'a>) -> El<'a> {
    let r = feed.rights;
    let mine = m.sender_id == Some(r.me);
    let name = m.sender_display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| m.sender_username.clone());

    let mut body = Column::new().spacing(3);
    if let Some(quote) = super::msg_render::reply::quote(m) {
        body = body.push(quote);
    }
    if head {
        body = body.push(
            row![
                iced::widget::mouse_area(text(name.clone()).size(14).font(SEMIBOLD).color(if mine { INDIGO_SOFT } else { Color::WHITE })).on_press(msg(ChatMsg::OpenProfile(Box::new(author(m, feed))))).interaction(iced::mouse::Interaction::Pointer),
                text(display::stamp(&m.created_at)).size(11).color(FAINT),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    let (mut content, placeholder) = display::message_text(m);
    if let Some(att) = super::msg_render::attachment(m, feed) {
        body = body.push(att);
        // A poll's "📊 Poll: …" and a draw's title are in the card already, and so is the
        // "🎁 Attached Cheque for …" line a card is sent with when its box was left empty.
        if matches!(m.message_type.as_str(), "poll" | "random_draw" | "audio" | "video") || super::attach_picker::is_default_text(&m.message_type, &content) {
            content.clear();
        }
    }
    let mut line = Row::new().spacing(6).align_y(Alignment::End).push(super::timestamp::body(&content, if placeholder { 13.0 } else { 14.0 }, if placeholder { FAINT } else { TEXT_BODY }, &m.created_at));
    if m.is_edited == Some(true) {
        line = line.push(text("(edited)").size(10.5).color(FAINT));
    }
    if m.is_pinned == Some(true) {
        line = line.push(icon(icons::pin(), 11, AMBER_TEXT));
    }
    if !content.is_empty() || m.is_edited == Some(true) || m.is_pinned == Some(true) {
        body = body.push(line);
    }
    if let Some(chip) = thread_chip(m, feed) {
        body = body.push(chip);
    }
    if let Some(chips) = reactions(m, r) {
        body = body.push(chips);
    }

    let gutter: El = if head { iced::widget::mouse_area(avatar_pic(&name, feed.members.iter().find(|u| Some(u.id) == m.sender_id).and_then(super::display::avatar_of).or(m.sender_avatar.as_deref()), 38.0, None)).on_press(msg(ChatMsg::OpenProfile(Box::new(author(m, feed))))).interaction(iced::mouse::Interaction::Pointer).into() } else { space().width(38).into() };
    let flashed = feed.flash == Some(m.id);
    let content = container(row![gutter, body.width(Fill)].spacing(12))
        .width(Fill)
        .padding(Padding { top: if head { 10.0 } else { 2.0 }, right: 12.0, bottom: 2.0, left: 12.0 })
        .style(move |_| container::Style { background: flashed.then(|| rgba(0x6366f1, 0.25).into()), border: iced::Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() });

    if m.message_type == "deleted" {
        return content.into();
    }
    let open = feed.menu.filter(|(id, _)| *id == m.id).map(|(_, k)| k);
    let menu: Option<El<'a>> = open.map(|kind| match kind {
        MenuKind::Context => context_menu(m, r),
        MenuKind::React => {
            let id = m.id;
            super::emoji_picker::view(feed.emoji, move |e| ChatMsg::React(id, e), None)
        }
    });
    Hover::new(content)
        .toolbar(toolbar(m, r))
        .menu(menu, msg(ChatMsg::CloseMenu))
        .pinned(open.is_some())
        .highlight(rgba(0xffffff, 0.025))
        .conceal(feed.conceal, 62.0, rgba(0xffffff, 0.035))
        .on_right_click(msg(ChatMsg::Menu(m.id, MenuKind::Context)))
        .into()
}

/// Who wrote a message: the room's member entry if there is one (presence, key), else what the
/// message says.
fn author(m: &ChatMessageItem, feed: &Feed<'_>) -> UserInfo {
    feed.members.iter().find(|u| Some(u.id) == m.sender_id).cloned().unwrap_or_else(|| UserInfo {
        id: m.sender_id.unwrap_or_default(),
        username: m.sender_username.clone(),
        display_name: m.sender_display_name.clone().unwrap_or_default(),
        avatar: m.sender_avatar.clone(),
        ..Default::default()
    })
}

fn thread_chip<'a>(m: &'a ChatMessageItem, feed: &Feed<'a>) -> Option<El<'a>> {
    let t = m.thread_summary.as_ref().filter(|t| t.count > 0 && feed.rights.threads)?;
    let mut faces = Row::new().spacing(-6.0);
    for id in t.participant_ids.iter().take(4) {
        let member = feed.members.iter().find(|u| u.id == *id);
        let who = member.map(|u| if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() }).unwrap_or_else(|| id.to_string());
        faces = faces.push(container(avatar_pic(&who, member.and_then(super::display::avatar_of), 18.0, None)).padding(1).style(round_fill(hex(0x1a1b26).into(), 10.0)));
    }
    let mut line = row![faces, text(format!("{} {}", t.count, if t.count == 1 { "reply" } else { "replies" })).size(12.5).font(BOLD).color(INDIGO_SOFT)].spacing(8).align_y(Alignment::Center);
    if let Some(last) = t.last_reply_at.as_deref() {
        line = line.push(text(format!("Last reply {}", display::stamp(last))).size(11.5).color(MUTED));
    }
    Some(
        button(line)
            .padding([5, 10])
            .style(|_, s| button::Style {
                background: Some(rgba(0x6366f1, if matches!(s, button::Status::Hovered) { 0.16 } else { 0.08 }).into()),
                border: border(rgba(0x818cf8, 0.25), 1.0, 10.0),
                text_color: INDIGO_SOFT,
                ..Default::default()
            })
            .on_press(msg(ChatMsg::OpenThread(m.id)))
            .into(),
    )
}

pub fn reactions(m: &ChatMessageItem, r: Rights) -> Option<El<'_>> {
    let list = m.reactions.as_ref().filter(|x| x.values().any(|u| !u.is_empty()))?;
    let mut sorted: Vec<_> = list.iter().filter(|(_, u)| !u.is_empty()).collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    let mut chips: Row<'_, Message> = Row::new().spacing(5).align_y(Alignment::Center);
    for (emoji, users) in sorted {
        let me_too = users.contains(&r.me);
        chips = chips.push(
            button(row![text(emoji.clone()).size(13).font(EMOJI), text(users.len().to_string()).size(11.5).font(BOLD)].spacing(4).align_y(Alignment::Center))
                .padding([2, 8])
                .style(move |_, s| button::Style {
                    background: Some(if me_too { rgba(0x6366f1, 0.2) } else if matches!(s, button::Status::Hovered) { rgba(0xffffff, 0.09) } else { rgba(0xffffff, 0.05) }.into()),
                    border: border(if me_too { rgba(0x6366f1, 0.6) } else { BORDER }, 1.0, 9.0),
                    text_color: if me_too { hex(0xc7d2fe) } else { TEXT_BODY },
                    ..Default::default()
                })
                .on_press_maybe(r.write.then(|| msg(ChatMsg::React(m.id, emoji.clone())))),
        );
    }
    if r.write {
        chips = chips.push(
            button(icon(icons::smile_plus(), 14, MUTED)).padding([3, 7]).style(ghost(9.0, rgba(0xffffff, 0.07))).on_press(msg(ChatMsg::Menu(m.id, MenuKind::React))),
        );
    }
    Some(chips.wrap().vertical_spacing(5).into())
}

fn tool<'a>(handle: svg::Handle, color: Color, tip: &'a str, on: ChatMsg) -> El<'a> {
    iced::widget::tooltip(
        button(center(icon(handle, 15, color))).width(30).height(28).padding(0).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press(msg(on)),
        container(text(tip).size(11.5).color(TEXT_BODY)).padding([4, 8]).style(boxed(hex(0x111216), BORDER, 6.0)),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

fn emoji_button<'a>(emoji: &'a str, size: f32, on: ChatMsg) -> El<'a> {
    button(center(text(emoji).size(size).font(EMOJI))).width(size + 14.0).height(size + 12.0).padding(0).style(ghost(7.0, rgba(0xffffff, 0.08))).on_press(msg(on)).into()
}

/// The bar over a hovered message (Discord's floating action bar).
fn toolbar<'a>(m: &'a ChatMessageItem, r: Rights) -> El<'a> {
    let mut bar = Row::new().spacing(1).align_y(Alignment::Center);
    if r.write {
        for e in QUICK_REACTIONS.iter().take(4) {
            bar = bar.push(emoji_button(e, 15.0, ChatMsg::React(m.id, e.to_string())));
        }
        bar = bar.push(tool(icons::reply(), LABEL, "Reply", ChatMsg::Reply(m.id)));
        if r.threads {
            bar = bar.push(tool(icons::message_square_text(), LABEL, "Reply in thread", ChatMsg::OpenThread(m.id)));
        }
        bar = bar.push(tool(icons::smile_plus(), LABEL, "Add reaction", ChatMsg::Menu(m.id, MenuKind::React)));
    }
    if r.pin {
        let pinned = m.is_pinned == Some(true);
        bar = bar.push(tool(if pinned { icons::pin_off() } else { icons::pin() }, LABEL, if pinned { "Unpin message" } else { "Pin message" }, ChatMsg::TogglePin(m.id)));
    }
    if matches!(m.message_type.as_str(), "image" | "file" | "audio" | "video") {
        bar = bar.push(tool(icons::download(), LABEL, "Save to Downloads", ChatMsg::SaveAttachment(m.id)));
    } else {
        bar = bar.push(tool(icons::copy(), LABEL, "Copy text", ChatMsg::CopyText(m.id)));
    }
    if can_edit(m, &r) {
        bar = bar.push(tool(icons::pencil(), LABEL, "Edit message", ChatMsg::Edit(m.id)));
    }
    if can_delete(m, &r) {
        bar = bar.push(tool(icons::trash(), DANGER_TEXT, "Delete message", ChatMsg::Delete(m.id)));
    }
    container(bar).padding([2, 4]).style(|_| container::Style {
        background: Some(hex(0x1c1d29).into()),
        border: border(rgba(0xffffff, 0.12), 1.0, 8.0),
        shadow: iced::Shadow { color: rgba(0x000000, 0.5), offset: iced::Vector::new(0.0, 4.0), blur_radius: 14.0 },
        ..Default::default()
    })
    .into()
}

fn menu_item<'a>(label: &'a str, handle: svg::Handle, danger: bool, on: ChatMsg) -> El<'a> {
    let color = if danger { DANGER_TEXT } else { TEXT_BODY };
    button(row![text(label).size(13).color(color).width(Fill), icon(handle, 15, if danger { DANGER_TEXT } else { MUTED })].align_y(Alignment::Center))
        .width(Fill)
        .padding([7, 10])
        .style(move |_, s| button::Style {
            background: matches!(s, button::Status::Hovered).then(|| if danger { rgba(0xef4444, 0.14) } else { rgba(0x6366f1, 0.22) }.into()),
            border: border(Color::TRANSPARENT, 0.0, 6.0),
            text_color: color,
            ..Default::default()
        })
        .on_press(msg(on))
        .into()
}

fn divider<'a>() -> El<'a> {
    container(container(space().width(Fill).height(1)).style(theme::rule)).padding([4, 6]).into()
}

fn popup<'a>(content: impl Into<El<'a>>, width: f32) -> El<'a> {
    container(content).width(width).padding(6).style(|_| container::Style {
        background: Some(hex(0x18191f).into()),
        border: border(rgba(0xffffff, 0.1), 1.0, 10.0),
        shadow: iced::Shadow { color: rgba(0x000000, 0.6), offset: iced::Vector::new(0.0, 8.0), blur_radius: 24.0 },
        ..Default::default()
    })
    .into()
}

/// Right-click menu (MessageContextMenu.tsx): the same actions as the bar, in words.
fn context_menu<'a>(m: &'a ChatMessageItem, r: Rights) -> El<'a> {
    let mut col = Column::new().spacing(1);
    if r.write {
        let mut emojis = Row::new().spacing(2);
        for e in QUICK_REACTIONS.iter().take(6) {
            emojis = emojis.push(emoji_button(e, 17.0, ChatMsg::React(m.id, e.to_string())));
        }
        emojis = emojis.push(button(center(icon(icons::smile_plus(), 17, MUTED))).width(31).height(29).padding(0).style(ghost(7.0, rgba(0xffffff, 0.08))).on_press(msg(ChatMsg::Menu(m.id, MenuKind::React))));
        col = col.push(emojis).push(divider());
        col = col.push(menu_item("Reply", icons::reply(), false, ChatMsg::Reply(m.id)));
        if r.threads {
            col = col.push(menu_item("Reply in thread", icons::message_square_text(), false, ChatMsg::OpenThread(m.id)));
        }
    }
    if can_edit(m, &r) {
        col = col.push(menu_item("Edit message", icons::pencil(), false, ChatMsg::Edit(m.id)));
    }
    if r.pin {
        let pinned = m.is_pinned == Some(true);
        col = col.push(menu_item(if pinned { "Unpin message" } else { "Pin message" }, if pinned { icons::pin_off() } else { icons::pin() }, false, ChatMsg::TogglePin(m.id)));
    }
    col = col.push(divider());
    if !display::message_text(m).1 {
        col = col.push(menu_item("Copy text", icons::copy(), false, ChatMsg::CopyText(m.id)));
    }
    col = col.push(menu_item("Copy message ID", icons::hash(), false, ChatMsg::CopyId(m.id)));
    if can_delete(m, &r) {
        col = col.push(divider()).push(menu_item("Delete message", icons::trash(), true, ChatMsg::Delete(m.id)));
    }
    popup(col, 236.0)
}

fn day_divider<'a>(label: String) -> El<'a> {
    let line = || container(space().width(Fill).height(1)).style(theme::rule);
    container(row![line(), text(label).size(11).font(SEMIBOLD).color(FAINT), line()].spacing(10).align_y(Alignment::Center)).padding([0, 12]).into()
}
