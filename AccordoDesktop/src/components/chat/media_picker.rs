// SPDX-License-Identifier: GPL-3.0-or-later
//! The pop-up over the message box (GifPicker.tsx, EmojiPicker.tsx): emoji to type, and GIFs /
//! stickers to send. GIFs come through Accordo's proxy (the server holds the KLIPY / GIPHY key;
//! the provider never sees who searches) — with no provider set up there, the GIF tab is hidden.

use iced::widget::{button, center, column, container, image, row, scrollable, space, text, text_input, Column, Row};
use iced::{Alignment, Color, Fill, Length};

use crate::api::gifs::GifItem;
use crate::api::stickers::{StickerAttachment, StickerPack};
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::media::{Cache, Media};
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

pub const GIF_SEARCH: &str = "gif-search";

/// Quick searches under the box (Tenor-style categories).
pub const CATEGORIES: [&str; 12] = ["Love", "Haha", "Wow", "Sad", "Angry", "Yes", "No", "Hi", "Thanks", "GM", "Party", "Money"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GifTab {
    Gifs,
    Stickers,
}

pub struct GifPanel {
    pub tab: GifTab,
    pub query: String,
    /// What's shown for the tab (trending, or results for `query`); None while loading.
    pub items: Option<Vec<GifItem>>,
    pub loading_more: bool,
    pub exhausted: bool,
    /// The server has a GIF provider (None: not asked yet).
    pub enabled: Option<bool>,
    pub packs: Option<Vec<StickerPack>>,
}

impl GifPanel {
    pub fn new() -> Self {
        Self { tab: GifTab::Gifs, query: String::new(), items: None, loading_more: false, exhausted: false, enabled: None, packs: None }
    }
}

pub enum Panel {
    Emoji,
    Gifs(GifPanel),
    /// The "+" menu (the web's attach / Web3 / tools menu).
    Plus,
}

impl Panel {
    /// Opened from the left of the message box (else from the right).
    pub fn on_left(&self) -> bool {
        matches!(self, Panel::Plus)
    }
}

/// The cache key of a still preview (pickers show first frames only: dozens of animations at
/// once would eat memory).
pub fn still_key(path: &str) -> String {
    format!("still:{path}")
}

fn m(msg: ChatMsg) -> Message {
    Message::Chat(msg)
}

fn frame<'a>(content: El<'a>, width: f32, height: impl Into<Length>) -> El<'a> {
    container(content)
        .width(width)
        .height(height)
        .style(|_| container::Style {
            background: Some(hex(0x18191f).into()),
            border: border(rgba(0xffffff, 0.1), 1.0, 12.0),
            shadow: iced::Shadow { color: rgba(0x000000, 0.6), offset: iced::Vector::new(0.0, 8.0), blur_radius: 24.0 },
            ..Default::default()
        })
        .into()
}

/// A tile of the grid: the still preview once loaded, a shimmer box until then.
fn tile<'a>(cache: &'a Cache, path: &str, w: f32, h: f32, on: ChatMsg) -> El<'a> {
    let inner: El = match cache.get(&still_key(path)) {
        Some(Media::Ready(f)) => image(f.at(0).clone()).width(w).height(h).content_fit(iced::ContentFit::Cover).into(),
        Some(Media::Failed) => center(icon(icons::image(), 16, FAINT)).width(w).height(h).into(),
        _ => space().width(w).height(h).into(),
    };
    button(container(inner).width(w).height(h).clip(true).style(boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 8.0)))
        .padding(0)
        .style(ghost(8.0, rgba(0x6366f1, 0.3)))
        .on_press(m(on))
        .into()
}

pub fn gif_view<'a>(p: &'a GifPanel, cache: &'a Cache) -> El<'a> {
    let gifs_on = p.enabled != Some(false);
    let tab = |t: GifTab, label: &'static str| -> El<'a> {
        let active = p.tab == t;
        button(text(label).size(12.5).font(BOLD)).padding([6, 12]).style(segment(active)).on_press(m(ChatMsg::GifTab(t))).into()
    };
    let mut tabs = row![].spacing(4);
    if gifs_on {
        tabs = tabs.push(tab(GifTab::Gifs, "GIFs"));
    }
    tabs = tabs.push(tab(GifTab::Stickers, "Stickers"));
    let header = row![tabs, space().width(Fill), button(icon(icons::x(), 14, MUTED)).padding(3).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(ChatMsg::ClosePanel))].align_y(Alignment::Center);
    let search = text_input(if p.tab == GifTab::Gifs { "Search GIFs" } else { "Search stickers" }, &p.query)
        .id(GIF_SEARCH)
        .on_input(|q| m(ChatMsg::GifQuery(q)))
        .on_submit(m(ChatMsg::GifSearch))
        .size(13)
        .padding([8, 10])
        .style(field);

    let mut chips = Row::new().spacing(4);
    for c in CATEGORIES {
        let on = p.query.trim().eq_ignore_ascii_case(c);
        chips = chips.push(button(text(c).size(11).font(SEMIBOLD)).padding([3, 9]).style(pill(on)).on_press(m(ChatMsg::GifCategory(if on { String::new() } else { c.to_string() }))));
    }
    let chips = chips.wrap().vertical_spacing(4);

    let mut body = Column::new().spacing(10);
    // Accordo's own sticker packs first.
    if p.tab == GifTab::Stickers {
        match &p.packs {
            None => body = body.push(text("Loading stickers…").size(12).color(MUTED)),
            Some(packs) => {
                for pack in packs.iter().filter(|pk| !pk.stickers.is_empty()) {
                    let mut grid = Column::new().spacing(6);
                    for chunk in pack.stickers.chunks(6) {
                        let mut line = Row::new().spacing(6);
                        for s in chunk {
                            line = line.push(tile(cache, &s.url, 70.0, 70.0, ChatMsg::SendSticker(Box::new(StickerAttachment::from(s)))));
                        }
                        grid = grid.push(line);
                    }
                    body = body.push(column![text(pack.name.clone()).size(11).font(BOLD).color(MUTED), grid].spacing(6));
                }
            }
        }
        if gifs_on {
            body = body.push(text("From the GIF library").size(11).font(BOLD).color(MUTED));
        }
    }
    if gifs_on {
        match &p.items {
            None => body = body.push(center(text("Loading…").size(12).color(MUTED)).height(80)),
            Some(items) if items.is_empty() => body = body.push(center(text("Nothing found").size(12).color(MUTED)).height(80)),
            Some(items) => {
                // Stickers are small and square: more per row.
                let (cols, w) = if p.tab == GifTab::Stickers { (4, 104.0) } else { (3, 140.0) };
                let mut grid = Column::new().spacing(6);
                for chunk in items.chunks(cols) {
                    let mut line = Row::new().spacing(6);
                    for g in chunk {
                        let h = if p.tab == GifTab::Stickers {
                            w
                        } else if g.preview_width > 0 {
                            (w * g.preview_height as f32 / g.preview_width as f32).clamp(60.0, 170.0)
                        } else {
                            100.0
                        };
                        let on = if p.tab == GifTab::Gifs { ChatMsg::SendGif(Box::new(g.clone())) } else { ChatMsg::SendSticker(Box::new(StickerAttachment::from(g))) };
                        line = line.push(tile(cache, &g.preview, w, h, on));
                    }
                    grid = grid.push(line);
                }
                body = body.push(grid);
                if !p.exhausted {
                    body = body.push(
                        center(button(text(if p.loading_more { "Loading…" } else { "More" }).size(12)).padding([5, 14]).style(outline_indigo).on_press_maybe((!p.loading_more).then(|| m(ChatMsg::MoreGifs)))).height(40),
                    );
                }
                body = body.push(text(format!("{} shown", items.len())).size(10.5).color(FAINT));
            }
        }
    } else if p.tab == GifTab::Gifs {
        body = body.push(text("GIFs aren't set up on this server.").size(12).color(MUTED));
    }

    // Near the end of the list: the next page comes by itself.
    let list = scrollable(body).height(Fill).style(thin_scroll).on_scroll(|v| {
        let near_end = v.content_bounds().height - (v.absolute_offset().y + v.bounds().height) < 240.0;
        if near_end { m(ChatMsg::MoreGifs) } else { m(ChatMsg::GifScrolled) }
    });
    frame(column![header, search, chips, list].spacing(8).padding(10).into(), 476.0, 520.0)
}

fn plus_item<'a>(handle: iced::widget::svg::Handle, tint: Color, title: &'a str, sub: &'a str, on: Message) -> El<'a> {
    button(
        row![
            container(center(icon(handle, 15, tint))).width(30).height(30).style(boxed(Color { a: 0.14, ..tint }, Color::TRANSPARENT, 8.0)),
            column![text(title).size(12.5).font(BOLD).color(if tint == INDIGO_SOFT || tint == MUTED { Color::WHITE } else { tint }), text(sub).size(10.5).color(MUTED)].spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([7, 9])
    .style(ghost(8.0, rgba(0xffffff, 0.06)))
    .on_press(on)
    .into()
}

/// The + menu: money items only for the sections that are on (App settings → Modules).
fn plus_view<'a>(modules: crate::modules::Modules, room_tools: bool) -> El<'a> {
    use crate::modules::Module;
    use super::attach_picker::{AttachMsg, AttachTab};
    let attach = Message::Picker(crate::components::file_picker::PickerMsg::Open(crate::components::file_picker::Purpose::Attachment));
    let section = |label: &'a str| -> El<'a> { container(text(label).size(10.5).font(BOLD).color(SKY)).padding([6, 9]).into() };
    let rule = || -> El<'a> { container(container(space().width(Fill).height(1)).style(crate::theme::rule)).padding([4, 6]).into() };
    let mut col: iced::widget::Column<'a, Message> = column![
        plus_item(icons::paperclip(), INDIGO_SOFT, "File or photo", "Up to 10 MB", attach),
        plus_item(icons::calendar_clock(), AMBER_TEXT, "Time", "Shown in everyone's own timezone", Message::TimePicker(crate::components::time_picker::TimeMsg::Open(crate::components::time_picker::Purpose::Chat, None))),
    ]
    .spacing(1)
    .padding(6);
    let share: Vec<El<'a>> = [
        (Module::Cheques, icons::gift(), AMBER_TEXT, "Attach Existing Cheque", "Select from active cheques", AttachTab::Cheques),
        (Module::Invoices, icons::receipt(), GREEN, "Attach Existing Invoice", "Select from active invoices", AttachTab::Invoices),
        (Module::Crowdfund, icons::flame(), hex(0xfb923c), "Share Campaign", "Post a crowdfund people can back from chat", AttachTab::Campaigns),
        (Module::Drops, icons::send(), INDIGO_SOFT, "Share Token Drop", "Post a drop — each listed wallet claims from chat", AttachTab::Drops),
    ]
    .into_iter()
    .filter(|(m, ..)| modules.shown(*m))
    .map(|(_, h, tint, title, sub, tab)| plus_item(h, tint, title, sub, Message::Attach(AttachMsg::Open(tab))))
    .collect();
    let create: Vec<El<'a>> = [
        (Module::Cheques, INDIGO_SOFT, "Create New Cheque", "Write and sign on-chain cheque"),
        (Module::Invoices, GREEN, "Issue New Invoice", "Create and issue new invoice"),
    ]
    .into_iter()
    .filter(|(m, ..)| modules.shown(*m))
    .map(|(m, tint, title, sub)| plus_item(icons::plus(), tint, title, sub, Message::Navigate(m.nav())))
    .collect();
    if !share.is_empty() {
        col = col.push(rule()).push(section("WEB3 FINANCE ACTIONS"));
        for e in share {
            col = col.push(e);
        }
    }
    if !create.is_empty() {
        col = col.push(rule());
        for e in create {
            col = col.push(e);
        }
    }
    // Polls, quizzes and draws: the room's admins (manage_room), as the server enforces.
    if room_tools {
        use super::poll_card::PollMsg;
        let p = |m: PollMsg| Message::Chat(ChatMsg::Poll(m));
        col = col
            .push(rule())
            .push(plus_item(icons::chart_bar(), INDIGO_SOFT, "Create Poll", "Ask the room, see results live", p(PollMsg::New(false))))
            .push(plus_item(icons::graduation_cap(), AMBER_TEXT, "Create Quiz", "One right answer, instant feedback", p(PollMsg::New(true))))
            .push(plus_item(icons::dices(), hex(0xf472b6), "Random Draw", "Pick random members of this room", p(PollMsg::NewDraw)));
    }
    frame(col.into(), 262.0, Length::Shrink)
}

/// `room_tools`: may start polls, quizzes and draws here (manage_room — the server checks the same).
pub fn view<'a>(panel: &'a Panel, cache: &'a Cache, emoji: &'a super::emoji_picker::EmojiUi, modules: crate::modules::Modules, room_tools: bool) -> El<'a> {
    let inner = match panel {
        Panel::Emoji => super::emoji_picker::view(emoji, ChatMsg::InsertEmoji, Some(ChatMsg::ClosePanel)),
        Panel::Gifs(p) => gif_view(p, cache),
        Panel::Plus => plus_view(modules, room_tools),
    };
    // Opaque: the pointer over the pop-up doesn't reach the messages under it.
    iced::widget::opaque(container(inner).width(Length::Shrink))
}
