// SPDX-License-Identifier: GPL-3.0-or-later
//! The full emoji picker (EmojiPicker.tsx, emoji-mart): every Unicode 15 emoji by category,
//! search by name and keyword, recently used, skin tones. The data is built into the app
//! (assets/emoji.json, from emoji-mart's — see scripts/emoji_data.mjs).

use std::sync::OnceLock;

use iced::widget::{button, center, column, container, row, scrollable, text, text_input, tooltip, Column, Row};
use iced::{Alignment, Fill};
use serde_json::Value;

use crate::app::Message;
use crate::components::ui::El;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

pub const EMOJI_SEARCH: &str = "emoji-search";
const PER_LINE: usize = 9;
const MAX_RECENT: usize = 27;
const MAX_RESULTS: usize = 180;

pub struct Emoji {
    pub native: String,
    pub name: String,
    keywords: String,
    /// Light → dark; empty when the emoji has no skin tones.
    skins: Vec<String>,
}

pub struct Category {
    pub icon: &'static str,
    pub label: &'static str,
    pub emojis: Vec<Emoji>,
}

fn label(id: &str) -> (&'static str, &'static str) {
    match id {
        "people" => ("😀", "Smileys & People"),
        "nature" => ("🐻", "Animals & Nature"),
        "foods" => ("🍔", "Food & Drink"),
        "activity" => ("⚽", "Activity"),
        "places" => ("✈️", "Travel & Places"),
        "objects" => ("💡", "Objects"),
        "symbols" => ("🔣", "Symbols"),
        "flags" => ("🏁", "Flags"),
        _ => ("•", "Other"),
    }
}

pub fn categories() -> &'static [Category] {
    static DATA: OnceLock<Vec<Category>> = OnceLock::new();
    DATA.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("../../../assets/emoji.json")).unwrap_or_default();
        v.get("categories")
            .and_then(Value::as_array)
            .map(|cats| {
                cats.iter()
                    .map(|c| {
                        let (icon, label) = label(c.get("id").and_then(Value::as_str).unwrap_or_default());
                        let emojis = c
                            .get("emojis")
                            .and_then(Value::as_array)
                            .map(|list| {
                                list.iter()
                                    .filter_map(|e| {
                                        let a = e.as_array()?;
                                        Some(Emoji {
                                            native: a.first()?.as_str()?.to_string(),
                                            name: a.get(1).and_then(Value::as_str).unwrap_or_default().to_string(),
                                            keywords: a.get(2).and_then(Value::as_str).unwrap_or_default().to_lowercase(),
                                            skins: a.get(3).and_then(Value::as_array).map(|s| s.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Category { icon, label, emojis }
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Skin tones as the picker offers them (none, then light → dark).
pub const SKINS: [&str; 6] = ["✋", "✋🏻", "✋🏼", "✋🏽", "✋🏾", "✋🏿"];

/// What the picker remembers between openings (this session).
pub struct EmojiUi {
    /// 0: recently used; 1..: the categories.
    pub category: usize,
    pub query: String,
    pub skin: usize,
    pub recent: Vec<String>,
}

impl Default for EmojiUi {
    fn default() -> Self {
        Self { category: 1, query: String::new(), skin: 0, recent: Vec::new() }
    }
}

impl EmojiUi {
    pub fn used(&mut self, e: &str) {
        self.recent.retain(|x| x != e);
        self.recent.insert(0, e.to_string());
        self.recent.truncate(MAX_RECENT);
    }
}

impl Emoji {
    pub fn with_skin(&self, skin: usize) -> &str {
        if skin > 0 { self.skins.get(skin - 1).map_or(self.native.as_str(), String::as_str) } else { &self.native }
    }
}

fn cell<'a>(e: String, tip: String, on: Message) -> El<'a> {
    tooltip(
        button(center(text(e).size(21).font(EMOJI))).width(38).height(36).padding(0).style(ghost(7.0, rgba(0xffffff, 0.09))).on_press(on),
        container(text(tip).size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x111216), BORDER, 6.0)),
        tooltip::Position::Top,
    )
    .into()
}

fn grid<'a>(items: Vec<(String, String)>, pick: &dyn Fn(String) -> ChatMsg) -> El<'a> {
    let mut col = Column::new().spacing(1);
    for chunk in items.chunks(PER_LINE) {
        let mut line = Row::new().spacing(1);
        for (e, name) in chunk {
            line = line.push(cell(e.clone(), name.clone(), Message::Chat(pick(e.clone()))));
        }
        col = col.push(line);
    }
    col.into()
}

/// The picker; `pick` says what choosing an emoji does (type it, react with it).
pub fn view<'a>(ui: &'a EmojiUi, pick: impl Fn(String) -> ChatMsg + 'a, close: Option<ChatMsg>) -> El<'a> {
    let cats = categories();
    let q = ui.query.trim().to_lowercase();

    let mut tabs = Row::new().spacing(1);
    let tab = |i: usize, icon: &'static str, tip: &'static str| -> El<'a> {
        let active = q.is_empty() && ui.category == i;
        tooltip(
            button(center(text(icon).size(17).font(EMOJI))).width(34).height(30).padding(0).style(selectable(active, rgba(0x6366f1, 0.25), 7.0)).on_press(Message::Chat(ChatMsg::EmojiCategory(i))),
            container(text(tip).size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x111216), BORDER, 6.0)),
            tooltip::Position::Bottom,
        )
        .into()
    };
    tabs = tabs.push(tab(0, "🕘", "Recently used"));
    for (i, c) in cats.iter().enumerate() {
        tabs = tabs.push(tab(i + 1, c.icon, c.label));
    }

    let (title, items): (String, Vec<(String, String)>) = if !q.is_empty() {
        let found: Vec<(String, String)> = cats
            .iter()
            .flat_map(|c| c.emojis.iter())
            .filter(|e| e.name.to_lowercase().contains(&q) || e.keywords.contains(&q))
            .take(MAX_RESULTS)
            .map(|e| (e.with_skin(ui.skin).to_string(), e.name.clone()))
            .collect();
        (format!("Results for “{}”", ui.query.trim()), found)
    } else if ui.category == 0 {
        let list = if ui.recent.is_empty() {
            ["👍", "❤️", "😂", "🔥", "🎉", "😮", "😢", "🙏", "👀"].iter().map(|e| (e.to_string(), String::new())).collect()
        } else {
            ui.recent.iter().map(|e| (e.clone(), String::new())).collect()
        };
        ("Recently used".into(), list)
    } else {
        let c = &cats[(ui.category - 1).min(cats.len().saturating_sub(1))];
        (c.label.to_string(), c.emojis.iter().map(|e| (e.with_skin(ui.skin).to_string(), e.name.clone())).collect())
    };

    let body: El = if items.is_empty() { center(text("No emoji found").size(12).color(MUTED)).height(80).into() } else { grid(items, &pick) };
    let skin_button = button(center(text(SKINS[ui.skin]).size(17).font(EMOJI))).width(34).height(32).padding(0).style(ghost(7.0, rgba(0xffffff, 0.08))).on_press(Message::Chat(ChatMsg::EmojiSkin));
    let mut head = row![
        text_input("Search emoji", &ui.query).id(EMOJI_SEARCH).on_input(|v| Message::Chat(ChatMsg::EmojiQuery(v))).size(13).padding([7, 10]).style(field),
        tooltip(skin_button, container(text("Skin tone").size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x111216), BORDER, 6.0)), tooltip::Position::Bottom),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if let Some(c) = close {
        head = head.push(button(crate::components::ui::icon(crate::icons::x(), 14, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(c)));
    }
    container(
        column![
            head,
            tabs,
            container(container(iced::widget::space().width(Fill).height(1)).style(rule)).padding([2, 0]),
            text(title).size(11).font(BOLD).color(MUTED),
            scrollable(body).height(Fill).style(thin_scroll),
        ]
        .spacing(6)
        .padding(10),
    )
    .width(372)
    .height(400)
    .style(|_| container::Style {
        background: Some(hex(0x18191f).into()),
        border: border(rgba(0xffffff, 0.1), 1.0, 12.0),
        shadow: iced::Shadow { color: rgba(0x000000, 0.6), offset: iced::Vector::new(0.0, 8.0), blur_radius: 24.0 },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_loads() {
        let cats = super::categories();
        assert_eq!(cats.len(), 8);
        assert!(cats.iter().map(|c| c.emojis.len()).sum::<usize>() > 1800);
        let thumbs = cats.iter().flat_map(|c| c.emojis.iter()).find(|e| e.native == "👍").unwrap();
        assert_eq!(thumbs.with_skin(5), "👍🏿");
    }
}
