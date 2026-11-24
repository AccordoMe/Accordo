// SPDX-License-Identifier: GPL-3.0-or-later
//! The search panel beside the chat (SearchPanel.tsx): the box with its suggestions, the filters as
//! chips, how much history was read, and the results. The work is `pages/chat_search.rs`.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Column, Row};
use iced::{Alignment, Border, Color, Fill, Padding};

use crate::app::Message;
use crate::components::chat::{display, timestamp};
use crate::components::ui::{hline, icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::pages::chat_search::{self, Hint, Item, Key, RoomIndex, Search, SearchMsg, MAX_MESSAGES, MAX_RESULTS, SEARCH_INPUT};
use crate::theme::*;

fn msg(m: SearchMsg) -> Message {
    Message::Chat(ChatMsg::Search(m))
}

/// `encrypted`: the chat is end-to-end encrypted (the server never sees the search); `me`: the
/// signed-in username (`from:me`).
pub fn view<'a>(s: &'a Search, index: Option<&'a RoomIndex>, encrypted: bool, me: &str) -> El<'a> {
    let parsed = chat_search::parse(&s.query, me);
    let suggestions = chat_search::suggestions(&s.query, index, me);

    // The box: magnifier, the query, ✕ to clear it; then ✕ to close the panel.
    let mut field = row![
        icon(icons::search(), 15, MUTED),
        text_input("Search — try from: has: before:", &s.query)
            .id(SEARCH_INPUT)
            .on_input(|v| msg(SearchMsg::Query(v)))
            .on_submit(msg(SearchMsg::Submit))
            .size(13.5)
            .padding([8, 0])
            .style(bare_input),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !s.query.is_empty() {
        field = field.push(button(icon(icons::x(), 14, MUTED)).padding(2).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(SearchMsg::Query(String::new()))));
    }
    let header = container(
        row![
            container(field).width(Fill).padding(Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 12.0 }).style(boxed(rgba(0x000000, 0.3), rgba(0xffffff, 0.1), 10.0)),
            button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(SearchMsg::Close)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .height(56)
    .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 16.0 })
    .align_y(iced::alignment::Vertical::Center);

    let mut panel: Column<'a, Message> = column![header, hline()];

    // The filters typed, as chips (✕ takes one out); a wrong one in red with why.
    if !parsed.filters.is_empty() {
        let mut chips: Row<'a, Message> = row![].spacing(6);
        for f in &parsed.filters {
            let (fg, bg, line) = if f.error.is_some() { (hex(0xfca5a5), rgba(0xef4444, 0.12), rgba(0xef4444, 0.35)) } else { (hex(0xc7d2fe), rgba(0x6366f1, 0.18), rgba(0x6366f1, 0.35)) };
            let label = match f.error {
                Some(why) => format!("{} — {why}", f.value),
                None => f.value.clone(),
            };
            chips = chips.push(
                container(
                    row![
                        text(format!("{}:", f.name)).size(12).font(BOLD).color(fg),
                        text(label).size(12).color(fg),
                        button(icon(icons::x(), 11, fg)).padding(1).style(ghost(4.0, rgba(0xffffff, 0.08))).on_press(msg(SearchMsg::RemoveFilter(f.raw.clone()))),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding(Padding { top: 3.0, right: 4.0, bottom: 3.0, left: 8.0 })
                .style(boxed(bg, line, 6.0)),
            );
        }
        panel = panel.push(container(chips.wrap().vertical_spacing(6)).padding(Padding { top: 8.0, right: 16.0, bottom: 0.0, left: 16.0 }));
    }

    // How much was read, and that it's read here.
    let read = index.map_or(0, |ix| ix.items.len());
    let reading = !s.key_missing && index.is_none_or(RoomIndex::reading);
    let status = if reading {
        format!("Reading history on this computer… {read} messages")
    } else {
        let mut line = format!("{read} messages searched on this computer");
        if encrypted {
            line.push_str(" — the server never sees your search");
        }
        if read >= MAX_MESSAGES {
            line.push_str(" (most recent 3,000)");
        }
        if index.is_some_and(|ix| ix.failed) {
            line.push_str(" — couldn't read further back");
        }
        line
    };
    panel = panel.push(
        container(
            row![if reading { icon(icons::clock(), 12, FAINT) } else { icon(icons::shield_check(), 12, GREEN) }, text(status).size(11.5).color(FAINT)].spacing(6).align_y(Alignment::Center),
        )
        .padding([8, 16]),
    );
    panel = panel.push(hline());

    // Suggestions for the word being typed, then the results.
    let mut body: Column<'a, Message> = column![].spacing(2);
    if !suggestions.is_empty() {
        let typing_value = s.query.rsplit(' ').next().is_some_and(|w| w.contains(':'));
        body = body.push(caption(if typing_value { "SUGGESTIONS" } else { "SEARCH OPTIONS" }));
        for (i, sug) in suggestions.iter().enumerate() {
            body = body.push(suggestion(sug, i == s.selected.min(suggestions.len() - 1)));
        }
        body = body.push(space().height(6));
    }
    if s.key_missing {
        body = body.push(note("This chat's encryption key isn't on this computer yet, so there's nothing to search."));
    } else if !parsed.searching() {
        body = body.push(note("Type at least 2 characters, or add a filter like from:, has:link or during:."));
    } else {
        let found = index.map(|ix| chat_search::results(ix, &parsed)).unwrap_or_default();
        if found.is_empty() {
            body = body.push(note(if reading { "Searching…" } else { "No messages match." }));
        } else {
            let n = found.len();
            body = body.push(caption(&if n >= MAX_RESULTS { format!("{MAX_RESULTS}+ RESULTS") } else { format!("{n} RESULT{}", if n == 1 { "" } else { "S" }) }));
            for item in found {
                body = body.push(result(item, &parsed.text));
            }
        }
    }
    panel = panel.push(scrollable(container(body).padding([6, 8]).width(Fill)).height(Fill).style(thin_scroll));

    container(panel.height(Fill))
        .width(400)
        .height(Fill)
        .style(|_| container::Style { background: Some(hex(0x0f1015).into()), border: Border { color: BORDER, width: 1.0, radius: 0.0.into() }, ..Default::default() })
        .into()
}

fn caption<'a>(t: &str) -> El<'a> {
    container(text(t.to_string()).size(10.5).font(BOLD).color(FAINT)).padding(Padding { top: 6.0, right: 10.0, bottom: 2.0, left: 10.0 }).into()
}

fn note<'a>(t: &'a str) -> El<'a> {
    container(text(t).size(13).color(FAINT).line_height(1.5)).padding([14, 10]).into()
}

fn suggestion<'a>(s: &chat_search::Suggestion, selected: bool) -> El<'a> {
    let handle = match s.hint {
        Hint::Key(Key::From) | Hint::User => icons::user(),
        Hint::Key(Key::Mentions) => icons::at_sign(),
        Hint::Key(Key::Has) | Hint::Has => icons::paperclip(),
        Hint::Key(Key::Pinned) | Hint::Pinned => icons::pin(),
        Hint::Key(Key::Before | Key::After | Key::During) | Hint::Date => icons::calendar(),
    };
    let mut line = row![icon(handle, 14, MUTED), text(s.label.clone()).size(13).font(if matches!(s.hint, Hint::Key(_)) { BOLD } else { INTER }).color(hex(0xe2e8f0))].spacing(10).align_y(Alignment::Center);
    line = line.push(space().width(Fill));
    if let Some(d) = &s.detail {
        line = line.push(text(d.clone()).size(11.5).color(FAINT));
    }
    button(line)
        .width(Fill)
        .padding([7, 10])
        .style(move |t, st| if selected { selectable(true, rgba(0x6366f1, 0.18), 7.0)(t, st) } else { ghost(7.0, rgba(0xffffff, 0.04))(t, st) })
        .on_press(msg(SearchMsg::Pick(s.insert.clone())))
        .into()
}

fn result<'a>(i: &'a Item, q: &str) -> El<'a> {
    let mut who = row![text(i.sender.clone()).size(12).font(BOLD).color(hex(0xe2e8f0))].spacing(6).align_y(Alignment::Center);
    if i.pinned {
        who = who.push(icon(icons::pin(), 11, AMBER));
    }
    let mut when = row![].spacing(4).align_y(Alignment::Center);
    if i.thread_root_id.is_some() {
        when = when.push(icon(icons::message_square_text(), 11, FAINT));
    }
    when = when.push(text(date(&i.created_at)).size(12).color(FAINT));
    let body: El = if i.text.is_empty() {
        text(chat_search::type_label(&i.kind).unwrap_or("Attachment")).size(13).color(MUTED).into()
    } else {
        snippet(&i.text, q)
    };
    button(column![row![who, space().width(Fill), when].align_y(Alignment::Center), body].spacing(3))
        .width(Fill)
        .padding([10, 10])
        .style(ghost(10.0, rgba(0xffffff, 0.04)))
        .on_press(msg(SearchMsg::Open(i.id)))
        .into()
}

/// "Nov 3", or "Nov 3, 2025" from another year.
fn date(iso: &str) -> String {
    let Some(t) = display::local(iso) else { return String::new() };
    if t.format("%Y").to_string() == chrono::Local::now().format("%Y").to_string() {
        t.format("%b %-d").to_string()
    } else {
        t.format("%b %-d, %Y").to_string()
    }
}

/// The message on one paragraph, the first match marked, with ~40 characters before it.
fn snippet<'a>(text: &str, q: &str) -> El<'a> {
    const SIZE: f32 = 13.0;
    let flat = text.replace('\n', " ");
    let (before, hit, after) = cut(&flat, q);
    let mut spans = Vec::new();
    timestamp::spans_of(&before, SIZE, LABEL, &mut spans);
    if !hit.is_empty() {
        let mut marked = Vec::new();
        timestamp::spans_of(&hit, SIZE, Color::WHITE, &mut marked);
        spans.extend(marked.into_iter().map(|s| s.background(rgba(0xfacc15, 0.35)).border(Border { radius: 3.0.into(), ..Default::default() })));
    }
    timestamp::spans_of(&after, SIZE, LABEL, &mut spans);
    iced::widget::rich_text(spans).line_height(1.45).width(Fill).into()
}

/// (before, the match, after): the match where `q` (lowercase) is first found; no query or no
/// match — the start of the text.
fn cut(text: &str, q: &str) -> (String, String, String) {
    let head = |t: &str, n: usize| match t.char_indices().nth(n) {
        Some((at, _)) => format!("{}…", &t[..at]),
        None => t.to_string(),
    };
    let lower = text.to_lowercase();
    // Lowercasing kept every position (true for Latin and Cyrillic): the match maps back.
    let found = (!q.is_empty() && lower.len() == text.len()).then(|| lower.find(q)).flatten().filter(|at| text.is_char_boundary(*at) && text.is_char_boundary(at + q.len()));
    let Some(at) = found else { return (head(text, 200), String::new(), String::new()) };
    let end = at + q.len();
    let start = text[..at].char_indices().rev().nth(39).map_or(0, |(i, _)| i);
    let prefix = if start > 0 { "…" } else { "" };
    (format!("{prefix}{}", &text[start..at]), text[at..end].to_string(), head(&text[end..], 160))
}

#[cfg(test)]
mod tests {
    use super::cut;

    #[test]
    fn the_match_stays_in_view() {
        assert_eq!(cut("Hello World", "world"), ("Hello ".into(), "World".into(), String::new()));
        assert_eq!(cut("Привет Мир", "мир"), ("Привет ".into(), "Мир".into(), String::new()));
        let long = format!("{}needle tail", "x".repeat(100));
        let (before, hit, _) = cut(&long, "needle");
        assert!(before.starts_with('…') && before.chars().count() == 41 && hit == "needle");
        assert_eq!(cut("short", "").0, "short");
    }
}
