// SPDX-License-Identifier: GPL-3.0-or-later
//! Searching a chat on this computer (SearchPanel.tsx). With end-to-end encryption the server can't
//! search, so the room's history is fetched page by page, opened with the room key here and matched
//! here — Discord-style filters included (`from:` `mentions:` `has:` `before:` `after:` `during:`
//! `pinned:`). Memory only: the opened text is never written anywhere, and a new chat state (sign
//! out, another account) forgets it. The view is `components/chat/search_panel.rs`.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, Local, NaiveDate, TimeZone};
use iced::Task;

use super::chat_page::{room_keys, ChatMsg, State};
use crate::api::chat::{self, ChatMessageItem, MessageQuery};
use crate::api::client::{Api, ApiError};
use crate::app::Message;
use crate::components::chat::{display, timestamp};
use crate::e2ee::crypto::{decrypt_symmetric, Opened};
use crate::e2ee::rooms;

/// Messages per request, and how far back the index reads.
const PAGE: i64 = 100;
pub const MAX_MESSAGES: usize = 3000;
pub const MAX_RESULTS: usize = 100;
/// The search box (focused when the panel opens).
pub const SEARCH_INPUT: &str = "chat-search";

#[derive(Debug, Clone)]
pub enum SearchMsg {
    /// The header's magnifier / the shortcut: open for this chat, or close.
    Toggle,
    Close,
    Query(String),
    /// Enter: take the highlighted suggestion.
    Submit,
    /// Arrows over the suggestions.
    Move(i32),
    /// A suggestion: replaces the word being typed.
    Pick(String),
    RemoveFilter(String),
    /// A page of the room's history (room key, the page).
    Page(String, Result<Vec<ChatMessageItem>, ApiError>),
    /// A result: to it in the chat, or its thread.
    Open(i64),
}

/// The search panel, open for one chat.
#[derive(Debug, Clone)]
pub struct Search {
    pub room_key: String,
    pub query: String,
    /// The highlighted suggestion.
    pub selected: usize,
    /// A private chat whose key isn't on this computer: nothing can be read to search.
    pub key_missing: bool,
}

/// A message as the search sees it.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    /// The opened text (time tokens as words).
    pub text: String,
    lower: String,
    pub sender: String,
    pub sender_username: String,
    pub kind: String,
    /// Lowercase usernames.
    mentions: Vec<String>,
    has_link: bool,
    pub pinned: bool,
    pub created_at: String,
    at_ms: i64,
    pub thread_root_id: Option<i64>,
}

/// A room's history as read so far: newest first.
#[derive(Debug, Default)]
pub struct RoomIndex {
    pub items: Vec<Item>,
    /// The main-feed messages read (opened), so a result older than the chat's own feed can be
    /// shown there.
    feed: Vec<ChatMessageItem>,
    oldest_id: Option<i64>,
    pub done: bool,
    fetching: bool,
    /// Reading stopped on an error.
    pub failed: bool,
}

impl RoomIndex {
    /// Still reading history (and allowed to read more).
    pub fn reading(&self) -> bool {
        !self.done && self.items.len() < MAX_MESSAGES
    }

    /// A message as the server sent it: in, or in place of its earlier copy (gone if there's
    /// nothing in it to find any more).
    fn put(&mut self, m: &ChatMessageItem, keys: Option<&[String]>) {
        let found = item(m, keys);
        match (self.items.iter().position(|i| i.id == m.id), found) {
            (Some(at), Some(it)) => self.items[at] = it,
            (Some(at), None) => {
                self.items.remove(at);
            }
            (None, Some(it)) => {
                let at = self.items.partition_point(|i| i.id > it.id);
                self.items.insert(at, it);
            }
            (None, None) => {}
        }
        if m.thread_root_id.is_none() {
            let opened = rooms::open_message(m.clone(), keys);
            match self.feed.iter().position(|x| x.id == m.id) {
                Some(at) => self.feed[at] = opened,
                None => self.feed.push(opened),
            }
        }
    }

    fn remove(&mut self, ids: &HashSet<i64>) {
        self.items.retain(|i| !ids.contains(&i.id));
        self.feed.retain(|m| !ids.contains(&m.id));
    }
}

/// What a message of this kind reads as when it has no text (so `has:image` etc. find it).
pub fn type_label(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "image" => "Photo",
        "file" => "File",
        "video" => "Video",
        "audio" => "Voice message",
        "gif" => "GIF",
        "sticker" => "Sticker",
        "cheque" => "Cheque",
        "invoice" => "Invoice",
        "poll" => "Poll",
        "campaign" => "Campaign",
        "drop" => "Drop",
        "nft_mint" => "NFT drop",
        "random_draw" => "Random draw",
        _ => return None,
    })
}

/// The searchable copy of a message as the server sent it (`keys`: the room's). None: nothing to
/// find in it — deleted, empty, or sealed with a key this computer hasn't got.
fn item(m: &ChatMessageItem, keys: Option<&[String]>) -> Option<Item> {
    if m.message_type == "deleted" || (m.content.is_empty() && type_label(&m.message_type).is_none()) {
        return None;
    }
    let raw = match keys.filter(|_| !m.content.is_empty()) {
        Some(keys) => match decrypt_symmetric(&m.content, keys) {
            Opened::Text(t) | Opened::Plain(t) => t,
            Opened::Locked => return None,
        },
        None => m.content.clone(),
    };
    let text = timestamp::to_plain(&raw);
    Some(Item {
        id: m.id,
        lower: text.to_lowercase(),
        mentions: mentions(&text),
        has_link: text.split_whitespace().any(|w| ["http://", "https://"].iter().any(|p| w.find(p).is_some_and(|at| w.len() > at + p.len()))),
        text,
        sender: m.sender_display_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| m.sender_username.clone()),
        sender_username: m.sender_username.clone(),
        kind: m.message_type.clone(),
        pinned: m.is_pinned == Some(true),
        at_ms: display::local(&m.created_at).map_or(0, |t| t.timestamp_millis()),
        created_at: m.created_at.clone(),
        thread_root_id: m.thread_root_id,
    })
}

/// "@alice" (not in "bob@alice" or "@@alice"): lowercase usernames, 2–32 characters.
fn mentions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let name_char = |c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '-';
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let after_word = i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '@');
        if chars[i] == '@' && !after_word {
            let name: String = chars[i + 1..].iter().take_while(|c| name_char(c)).take(32).collect();
            if name.len() >= 2 {
                out.push(name.to_lowercase());
            }
            i += 1 + name.len();
        } else {
            i += 1;
        }
    }
    out
}

// ── The query: plain words plus key:value filters ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    From,
    Mentions,
    Has,
    Before,
    After,
    During,
    Pinned,
}

/// Every filter: the key, its name, what it takes.
pub const KEYS: [(Key, &str, &str); 7] = [
    (Key::From, "from", "user"),
    (Key::Mentions, "mentions", "user"),
    (Key::Has, "has", "link, image, file, cheque…"),
    (Key::Before, "before", "date"),
    (Key::After, "after", "date"),
    (Key::During, "during", "date"),
    (Key::Pinned, "pinned", "true"),
];

/// `has:` values and what they read as.
pub const HAS: [(&str, &str); 11] = [
    ("link", "Link"),
    ("image", "Image"),
    ("video", "Video"),
    ("file", "File"),
    ("voice", "Voice message"),
    ("gif", "GIF"),
    ("sticker", "Sticker"),
    ("cheque", "Cheque"),
    ("invoice", "Invoice"),
    ("poll", "Poll"),
    ("thread", "Thread reply"),
];

fn has(i: &Item, value: &str) -> bool {
    match value {
        "link" => i.has_link,
        "voice" => i.kind == "audio",
        "thread" => i.thread_root_id.is_some(),
        kind => i.kind == kind,
    }
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub key: Key,
    pub name: &'static str,
    pub value: String,
    /// The word as typed (removing the chip removes it).
    pub raw: String,
    pub error: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    /// The plain words, lowercase.
    pub text: String,
    pub filters: Vec<Filter>,
}

impl Parsed {
    /// Something to look for: two letters, or a filter that reads.
    pub fn searching(&self) -> bool {
        self.text.chars().count() >= 2 || self.filters.iter().any(|f| f.error.is_none())
    }
}

/// "from:alice" → ("from", "alice"); a key is word characters, the value may be empty.
fn split_token(word: &str) -> Option<(&str, &str)> {
    let colon = word.find(':')?;
    let key = &word[..colon];
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| (key, &word[colon + 1..]))
}

fn key_named(name: &str) -> Option<(Key, &'static str)> {
    KEYS.iter().find(|(_, n, _)| n.eq_ignore_ascii_case(name)).map(|(k, n, _)| (*k, *n))
}

/// The query: filters that read, the rest as words. `me`: the signed-in username (`from:me`).
pub fn parse(query: &str, me: &str) -> Parsed {
    let mut words = Vec::new();
    let mut filters = Vec::new();
    for raw in query.split_whitespace() {
        let known = split_token(raw).and_then(|(k, v)| key_named(k).map(|key| (key, v))).filter(|(_, v)| !v.is_empty());
        let Some(((key, name), value)) = known else {
            words.push(raw);
            continue;
        };
        let mut value = value.strip_prefix('@').unwrap_or(value).to_string();
        if matches!(key, Key::From | Key::Mentions) && value.eq_ignore_ascii_case("me") && !me.is_empty() {
            value = me.to_string();
        }
        let error = match key {
            Key::Before | Key::After | Key::During if day(&value).is_none() => Some("Use YYYY-MM-DD"),
            Key::Has if !HAS.iter().any(|(v, _)| v.eq_ignore_ascii_case(&value)) => Some("Unknown type"),
            _ => None,
        };
        filters.push(Filter { key, name, value, raw: raw.to_string(), error });
    }
    Parsed { text: words.join(" ").to_lowercase(), filters }
}

/// A YYYY-MM-DD day on this computer's clock: [start, end) in ms.
fn day(value: &str) -> Option<(i64, i64)> {
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    let start = |d: NaiveDate| Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest().map(|t| t.timestamp_millis());
    Some((start(d)?, start(d + Duration::days(1))?))
}

pub fn matches(i: &Item, p: &Parsed) -> bool {
    if !p.text.is_empty() && !i.lower.contains(&p.text) {
        return false;
    }
    p.filters.iter().filter(|f| f.error.is_none()).all(|f| {
        let v = f.value.to_lowercase();
        match f.key {
            Key::From => i.sender_username.to_lowercase() == v,
            Key::Mentions => i.mentions.contains(&v),
            Key::Has => has(i, &v),
            Key::Before => day(&v).is_some_and(|(start, _)| i.at_ms < start),
            Key::After => day(&v).is_some_and(|(_, end)| i.at_ms >= end),
            Key::During => day(&v).is_some_and(|(start, end)| i.at_ms >= start && i.at_ms < end),
            Key::Pinned => i.pinned == (v != "false"),
        }
    })
}

/// The newest matches, at most MAX_RESULTS.
pub fn results<'a>(index: &'a RoomIndex, p: &Parsed) -> Vec<&'a Item> {
    if !p.searching() {
        return Vec::new();
    }
    index.items.iter().filter(|i| matches(i, p)).take(MAX_RESULTS).collect()
}

// ── Suggestions under the box ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Key(Key),
    User,
    Has,
    Pinned,
    Date,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub hint: Hint,
    pub label: String,
    pub detail: Option<String>,
    /// Replaces the word being typed.
    pub insert: String,
}

/// The word being typed: after the last space.
fn typing(query: &str) -> &str {
    &query[query.rfind(' ').map_or(0, |at| at + 1)..]
}

/// What to offer for the word being typed: filter keys on an empty box (or a key's first
/// letters), people for `from:` / `mentions:`, kinds for `has:`, days for the date filters.
pub fn suggestions(query: &str, index: Option<&RoomIndex>, me: &str) -> Vec<Suggestion> {
    let word = typing(query);
    if let Some((key, value)) = split_token(word) {
        let value = value.strip_prefix('@').unwrap_or(value).to_lowercase();
        let Some((key, name)) = key_named(key) else { return Vec::new() };
        return match key {
            Key::From | Key::Mentions => {
                let mut list = people(index).into_iter().filter(|(u, n)| u.starts_with(&value) || n.to_lowercase().starts_with(&value)).collect::<Vec<_>>();
                if !me.is_empty() && "me".starts_with(&value) && !list.iter().any(|(u, _)| u.eq_ignore_ascii_case(me)) {
                    list.insert(0, (me.to_lowercase(), format!("{me} (you)")));
                }
                list.into_iter()
                    .take(8)
                    .map(|(u, n)| Suggestion { hint: Hint::User, label: n, detail: Some(format!("@{u}")), insert: format!("{name}:{u} ") })
                    .collect()
            }
            Key::Has => HAS
                .iter()
                .filter(|(v, _)| v.starts_with(&value))
                .map(|(v, label)| Suggestion { hint: Hint::Has, label: (*label).into(), detail: Some(format!("has:{v}")), insert: format!("has:{v} ") })
                .collect(),
            Key::Pinned => vec![Suggestion { hint: Hint::Pinned, label: "Pinned messages".into(), detail: Some("pinned:true".into()), insert: "pinned:true ".into() }],
            Key::Before | Key::After | Key::During => {
                let today = Local::now().date_naive();
                [("Today", 0), ("Yesterday", 1), ("A week ago", 7), ("A month ago", 30)]
                    .into_iter()
                    .map(|(label, back)| (label, (today - Duration::days(back)).format("%Y-%m-%d").to_string()))
                    .filter(|(_, d)| d.starts_with(&value))
                    .map(|(label, d)| Suggestion { hint: Hint::Date, label: label.into(), detail: Some(d.clone()), insert: format!("{name}:{d} ") })
                    .collect()
            }
        };
    }
    // Filter keys: on an empty box, or while the word is the start of one.
    let start = word.to_lowercase();
    if start.is_empty() && !query.trim().is_empty() {
        return Vec::new();
    }
    KEYS.iter()
        .filter(|(_, name, _)| name.starts_with(&start))
        .map(|(key, name, what)| Suggestion { hint: Hint::Key(*key), label: format!("{name}:"), detail: Some((*what).into()), insert: format!("{name}:") })
        .collect()
}

/// People seen in the room's history: senders, then the mentioned (username lowercase, name).
fn people(index: Option<&RoomIndex>) -> Vec<(String, String)> {
    let mut seen: Vec<(String, String)> = Vec::new();
    let Some(ix) = index else { return seen };
    for i in &ix.items {
        let u = i.sender_username.to_lowercase();
        if !u.is_empty() && !seen.iter().any(|(x, _)| *x == u) {
            seen.push((u, i.sender.clone()));
        }
    }
    for i in &ix.items {
        for m in &i.mentions {
            if !seen.iter().any(|(x, _)| x == m) {
                seen.push((m.clone(), m.clone()));
            }
        }
    }
    seen
}

// ── Messages ────────────────────────────────────────────────────────────────────────────────

fn msg(m: SearchMsg) -> Message {
    Message::Chat(ChatMsg::Search(m))
}

pub fn update(state: &mut State, m: SearchMsg, api: &Api) -> Task<Message> {
    match m {
        SearchMsg::Toggle => {
            let Some(room_key) = state.selected.clone() else { return Task::none() };
            if state.search.as_ref().is_some_and(|s| s.room_key == room_key) {
                state.search = None;
                return Task::none();
            }
            if state.rooms.get(&room_key).is_some_and(display::is_teaser) {
                return Task::none();
            }
            // One side panel at a time (the members list steps aside by itself).
            state.thread = None;
            state.pinned_open = false;
            state.search = Some(Search { room_key: room_key.clone(), query: String::new(), selected: 0, key_missing: false });
            Task::batch([read_more(state, api, &room_key), iced::widget::operation::focus(SEARCH_INPUT)])
        }
        SearchMsg::Close => {
            state.search = None;
            Task::none()
        }
        SearchMsg::Query(q) => {
            let Some(s) = state.search.as_mut() else { return Task::none() };
            s.query = q;
            s.selected = 0;
            // The key may have come since: try reading again.
            let room_key = s.room_key.clone();
            read_more(state, api, &room_key)
        }
        SearchMsg::Move(by) => {
            let Some(s) = state.search.as_ref() else { return Task::none() };
            let n = suggestions(&s.query, state.search_index.get(&s.room_key), &state.me_name).len();
            if let Some(s) = state.search.as_mut().filter(|_| n > 0) {
                s.selected = (s.selected as i64 + i64::from(by)).rem_euclid(n as i64) as usize;
            }
            Task::none()
        }
        SearchMsg::Submit => {
            let Some(s) = state.search.as_ref() else { return Task::none() };
            let list = suggestions(&s.query, state.search_index.get(&s.room_key), &state.me_name);
            match list.get(s.selected).or(list.first()) {
                Some(pick) => update(state, SearchMsg::Pick(pick.insert.clone()), api),
                None => Task::none(),
            }
        }
        SearchMsg::Pick(insert) => {
            let Some(s) = state.search.as_mut() else { return Task::none() };
            let keep = s.query.len() - typing(&s.query).len();
            s.query = format!("{}{insert}", &s.query[..keep]);
            s.selected = 0;
            Task::batch([iced::widget::operation::focus(SEARCH_INPUT), iced::widget::operation::move_cursor_to_end(SEARCH_INPUT)])
        }
        SearchMsg::RemoveFilter(raw) => {
            let Some(s) = state.search.as_mut() else { return Task::none() };
            let rest: Vec<&str> = s.query.split_whitespace().filter(|w| *w != raw).collect();
            s.query = if rest.is_empty() { String::new() } else { format!("{} ", rest.join(" ")) };
            s.selected = 0;
            Task::batch([iced::widget::operation::focus(SEARCH_INPUT), iced::widget::operation::move_cursor_to_end(SEARCH_INPUT)])
        }
        SearchMsg::Page(room_key, result) => {
            let keys = room_keys(state, &room_key);
            let Some(ix) = state.search_index.get_mut(&room_key) else { return Task::none() };
            ix.fetching = false;
            match result {
                Ok(page) if page.is_empty() => ix.done = true,
                Ok(page) => {
                    ix.oldest_id = page.iter().map(|m| m.id).min();
                    for m in &page {
                        ix.put(m, keys.as_deref());
                    }
                    if (page.len() as i64) < PAGE {
                        ix.done = true;
                    }
                }
                Err(e) => {
                    tracing::warn!("chat search: couldn't read the history: {}", e.message);
                    ix.done = true;
                    ix.failed = true;
                }
            }
            // On while the panel is open on this chat.
            if state.search.as_ref().is_some_and(|s| s.room_key == room_key) {
                read_more(state, api, &room_key)
            } else {
                Task::none()
            }
        }
        SearchMsg::Open(id) => {
            let Some(room_key) = state.search.as_ref().map(|s| s.room_key.clone()) else { return Task::none() };
            let Some(found) = state.search_index.get(&room_key).and_then(|ix| ix.items.iter().find(|i| i.id == id)).cloned() else { return Task::none() };
            // A thread reply: its thread takes the side (one panel at a time).
            if let Some(root) = found.thread_root_id {
                return super::chat_actions::open_thread(state, api, root);
            }
            // The panel stays open, so the next result is one click away.
            bring_into_feed(state, &room_key, id);
            Task::done(Message::Chat(ChatMsg::JumpTo(id)))
        }
    }
}

/// The next page of history, while the panel reads it. A private chat without its key here reads
/// nothing (and keeps no empty index): the panel says so, and typing tries again.
fn read_more(state: &mut State, api: &Api, room_key: &str) -> Task<Message> {
    let encrypted = state.rooms.get(room_key).is_some_and(display::is_encrypted_room);
    let missing = encrypted && room_keys(state, room_key).is_none();
    if let Some(s) = state.search.as_mut().filter(|s| s.room_key == room_key) {
        s.key_missing = missing;
    }
    if missing {
        return Task::none();
    }
    let ix = state.search_index.entry(room_key.to_string()).or_default();
    if ix.fetching || !ix.reading() {
        return Task::none();
    }
    ix.fetching = true;
    let q = MessageQuery { room_id: Some(room_key.to_string()), limit: Some(PAGE), before_id: ix.oldest_id, include_thread_replies: true, ..Default::default() };
    let key = room_key.to_string();
    Task::perform(chat::get_messages(api.clone(), q), move |r| msg(SearchMsg::Page(key.clone(), r)))
}

/// A result older than what the chat has loaded: the history read for the search goes into the
/// feed in front of it, so the chat can scroll to it.
fn bring_into_feed(state: &mut State, room_key: &str, id: i64) {
    let Some(ix) = state.search_index.get(room_key) else { return };
    let feed = state.messages.entry(room_key.to_string()).or_default();
    if feed.iter().any(|m| m.id == id) {
        return;
    }
    let oldest = feed.first().map_or(i64::MAX, |m| m.id);
    let mut older: Vec<ChatMessageItem> = ix.feed.iter().filter(|m| m.id < oldest).cloned().collect();
    if !older.iter().any(|m| m.id == id) {
        return;
    }
    older.sort_by_key(|m| m.id);
    older.append(feed);
    *feed = older;
}

// ── Kept current by the socket (only an index that exists) ──────────────────────────────────

/// A new or edited message as the socket brought it (before it's opened for the feed).
pub fn observe(state: &mut State, m: &ChatMessageItem) {
    if !state.search_index.contains_key(&m.room_id) {
        return;
    }
    let encrypted = state.rooms.get(&m.room_id).is_some_and(display::is_encrypted_room);
    let keys = room_keys(state, &m.room_id);
    if encrypted && keys.is_none() {
        return;
    }
    if let Some(ix) = state.search_index.get_mut(&m.room_id) {
        // Only inside what's been read: newer than its oldest message, or all of it once complete.
        if ix.done || ix.oldest_id.is_some_and(|oldest| m.id >= oldest) {
            ix.put(m, keys.as_deref());
        }
    }
}

/// Messages deleted or expired (ids are unique across chats: an event without its room still works).
pub fn forget(state: &mut State, ids: &HashSet<i64>) {
    state.search_index.values_mut().for_each(|ix| ix.remove(ids));
}

/// A message pinned or unpinned.
pub fn set_pinned(state: &mut State, id: i64, pinned: bool) {
    if let Some(i) = state.search_index.values_mut().find_map(|ix| ix.items.iter_mut().find(|i| i.id == id)) {
        i.pinned = pinned;
    }
}

/// Indexes kept per chat (`State::search_index`).
pub type Indexes = HashMap<String, RoomIndex>;

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: i64, who: &str, text: &str, kind: &str) -> ChatMessageItem {
        ChatMessageItem { id, room_id: "r".into(), sender_username: who.into(), content: text.into(), message_type: kind.into(), created_at: "2026-11-20T12:00:00Z".into(), ..Default::default() }
    }

    fn index(list: &[ChatMessageItem]) -> RoomIndex {
        let mut ix = RoomIndex::default();
        list.iter().for_each(|m| ix.put(m, None));
        ix
    }

    #[test]
    fn words_and_filters() {
        let ix = index(&[
            msg(1, "alice", "Hello @Bob, see https://accordo.me", "text"),
            msg(2, "bob", "hi alice", "text"),
            msg(3, "alice", "", "image"),
            msg(4, "carol", "deleted", "deleted"),
        ]);
        assert_eq!(ix.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![3, 2, 1], "newest first, nothing deleted");
        let ids = |q: &str| results(&ix, &parse(q, "bob")).iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids("hello"), vec![1]);
        assert_eq!(ids("from:alice"), vec![3, 1]);
        assert_eq!(ids("from:me"), vec![2]);
        assert_eq!(ids("mentions:@bob"), vec![1]);
        assert_eq!(ids("has:link"), vec![1]);
        assert_eq!(ids("has:image"), vec![3]);
        assert_eq!(ids("during:2026-11-20"), vec![3, 2, 1]);
        assert_eq!(ids("before:2026-11-20"), Vec::<i64>::new());
        assert!(ids("h").is_empty(), "one letter isn't a search");
        let p = parse("has:spaceship before:yesterday", "");
        assert_eq!(p.filters.iter().map(|f| f.error).collect::<Vec<_>>(), vec![Some("Unknown type"), Some("Use YYYY-MM-DD")]);
        assert!(!p.searching());
    }

    #[test]
    fn suggestions_follow_the_word_being_typed() {
        let ix = index(&[msg(1, "alice", "hey @carol", "text")]);
        assert_eq!(suggestions("", Some(&ix), "me").len(), KEYS.len());
        assert!(suggestions("hello ", Some(&ix), "me").is_empty());
        assert_eq!(suggestions("fr", Some(&ix), "me")[0].insert, "from:");
        let people: Vec<String> = suggestions("x from:", Some(&ix), "dazay").into_iter().map(|s| s.insert).collect();
        assert_eq!(people, vec!["from:dazay ", "from:alice ", "from:carol "]);
        assert_eq!(suggestions("has:st", Some(&ix), "")[0].insert, "has:sticker ");
        assert_eq!(suggestions("during:", Some(&ix), "").len(), 4);
    }

    #[test]
    fn mentions_and_edits() {
        assert_eq!(mentions("@ab hi bob@cd @@ef @x @under_score-1"), vec!["ab", "under_score-1"]);
        let mut ix = index(&[msg(1, "a", "first", "text")]);
        ix.put(&msg(1, "a", "changed", "text"), None);
        assert_eq!(ix.items[0].text, "changed");
        ix.remove(&HashSet::from([1]));
        assert!(ix.items.is_empty() && ix.feed.is_empty());
    }
}
