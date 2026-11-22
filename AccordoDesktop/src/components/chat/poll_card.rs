// SPDX-License-Identifier: GPL-3.0-or-later
//! Polls, quizzes and random draws in the chat (PollCard.tsx, CreatePollModal.tsx, RandomDraw.tsx).
//!
//! A poll's question and answers ride in the message (encrypted in private rooms, like any
//! message). Votes go to the server as opaque strings — room-key ciphertext in private rooms — and
//! are opened and counted here, so the server never learns an encrypted room's results. A draw is
//! picked by the server from the room's members and posted as its result; only its title is ours.

use std::collections::HashMap;

use iced::widget::{button, column, container, progress_bar, row, space, text, text_input, toggler, Column};
use iced::{Alignment, Color, Fill, Length};
use serde_json::Value;

use crate::api::chat::polls::{PollContent, PollState};
use crate::api::chat::UserInfo;
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

pub const MAX_OPTIONS: usize = 10;
/// "Ends after": (label, hours).
pub const DEADLINES: [(&str, Option<i64>); 5] = [("No limit", None), ("1 hour", Some(1)), ("24 hours", Some(24)), ("3 days", Some(72)), ("7 days", Some(168))];

#[derive(Debug, Clone)]
pub enum PollMsg {
    /// The + menu: a new poll (false) or quiz (true).
    New(bool),
    Kind(bool),
    Question(String),
    Answer(usize, String),
    AddAnswer,
    RemoveAnswer(usize),
    Correct(usize),
    Explanation(String),
    Multiple(bool),
    Anonymous(bool),
    Deadline(Option<i64>),
    Post,
    CloseForm,
    /// A poll's state from the server.
    Loaded(i64, Result<Box<PollState>, String>),
    /// An answer clicked (single choice: votes at once; multiple: toggles it).
    Pick(i64, String),
    /// Multiple choice: send the ticked answers.
    Vote(i64),
    /// The creator ends it.
    End(i64),
    // the random draw
    NewDraw,
    DrawTitle(String),
    DrawCount(i64),
    DrawIncludeMe(bool),
    Draw,
}

fn m(p: PollMsg) -> Message {
    Message::Chat(ChatMsg::Poll(p))
}

/// What's being written in the new-poll window.
#[derive(Debug, Clone, Default)]
pub struct PollForm {
    pub quiz: bool,
    pub question: String,
    /// (id, text)
    pub answers: Vec<(String, String)>,
    pub multiple: bool,
    pub anonymous: bool,
    /// Quiz: the right answer's id.
    pub correct: Option<String>,
    pub explanation: String,
    pub deadline: Option<i64>,
    pub busy: bool,
}

fn new_id() -> String {
    let mut b = [0u8; 6];
    let _ = getrandom::fill(&mut b);
    format!("o{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

impl PollForm {
    pub fn new(quiz: bool) -> Self {
        PollForm { quiz, answers: vec![(new_id(), String::new()), (new_id(), String::new())], ..Default::default() }
    }

    pub fn add(&mut self) {
        if self.answers.len() < MAX_OPTIONS {
            self.answers.push((new_id(), String::new()));
        }
    }

    /// Why it can't be posted yet (the web's button label), or None.
    pub fn blocker(&self) -> Option<&'static str> {
        let filled: Vec<&(String, String)> = self.answers.iter().filter(|(_, t)| !t.trim().is_empty()).collect();
        let distinct: std::collections::HashSet<String> = filled.iter().map(|(_, t)| t.trim().to_lowercase()).collect();
        if self.question.trim().is_empty() {
            Some("Write a question")
        } else if filled.len() < 2 {
            Some("Add at least 2 answers")
        } else if distinct.len() != filled.len() {
            Some("Answers must be different")
        } else if self.quiz && !self.correct.as_ref().is_some_and(|c| filled.iter().any(|(id, _)| id == c)) {
            Some("Mark the correct answer")
        } else {
            None
        }
    }

    /// The message's poll content (answers trimmed, empty ones dropped).
    pub fn content(&self) -> PollContent {
        use crate::api::chat::polls::PollOption;
        let options = self.answers.iter().filter(|(_, t)| !t.trim().is_empty()).map(|(id, t)| PollOption { id: id.clone(), text: t.trim().to_string() }).collect();
        PollContent {
            kind_tag: "poll".into(),
            kind: if self.quiz { "quiz" } else { "poll" }.into(),
            question: self.question.trim().to_string(),
            options,
            multiple: (!self.quiz).then_some(self.multiple),
            anonymous: Some(self.anonymous),
            correct: if self.quiz { self.correct.clone() } else { None },
            explanation: if self.quiz { Some(self.explanation.trim().to_string()).filter(|e| !e.is_empty()) } else { None },
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrawForm {
    pub title: String,
    pub count: i64,
    pub include_me: bool,
    pub busy: bool,
}

impl Default for DrawForm {
    fn default() -> Self {
        DrawForm { title: String::new(), count: 1, include_me: true, busy: false }
    }
}

/// The counted votes of one poll.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    pub counts: HashMap<String, i64>,
    /// option → who picked it (not in anonymous polls).
    pub voters: HashMap<String, Vec<i64>>,
    pub mine: Option<Vec<String>>,
    pub total: i64,
    /// Votes this computer couldn't open.
    pub unreadable: i64,
}

/// Counts the votes. Unknown answer ids are ignored and single-choice polls count only the first
/// pick — a client can't stuff a vote the server can't inspect. `keys`: the room's keys (private
/// rooms; None there = nothing can be opened).
pub fn tally(state: &PollState, content: &PollContent, private: bool, keys: Option<&[String]>) -> Tally {
    let valid: std::collections::HashSet<&str> = content.options.iter().map(|o| o.id.as_str()).collect();
    let multiple = content.multiple == Some(true);
    let mut t = Tally::default();
    for v in &state.votes {
        let raw = if private {
            match keys.map(|k| crate::e2ee::crypto::decrypt_symmetric(&v.choice, k)) {
                Some(crate::e2ee::crypto::Opened::Text(s)) => s,
                _ => {
                    t.unreadable += 1;
                    continue;
                }
            }
        } else {
            v.choice.clone()
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            t.unreadable += 1;
            continue;
        };
        let mut ids: Vec<String> = Vec::new();
        for id in parsed.get("o").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if valid.contains(id) && !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
        if !multiple {
            ids.truncate(1);
        }
        if ids.is_empty() {
            continue;
        }
        t.total += 1;
        for id in &ids {
            *t.counts.entry(id.clone()).or_default() += 1;
            if let Some(u) = v.user_id {
                t.voters.entry(id.clone()).or_default().push(u);
            }
        }
        if v.mine {
            t.mine = Some(ids);
        }
    }
    t
}

/// One poll's live state on this computer.
#[derive(Debug, Clone, Default)]
pub struct PollView {
    pub state: Option<PollState>,
    pub tally: Tally,
    /// Multiple choice: ticked, not sent yet.
    pub pending: Vec<String>,
    pub busy: bool,
}

pub fn closed(state: &PollState) -> bool {
    state.closed || state.closes_at.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).is_some_and(|t| t <= chrono::Utc::now())
}

fn time_left(iso: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(iso) else { return String::new() };
    let s = (t.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds();
    match s {
        ..=0 => "ended".into(),
        1..3600 => format!("{}m left", (s / 60).max(1)),
        3600..86400 => format!("{}h left", s / 3600),
        _ => format!("{}d left", s / 86400),
    }
}

fn chip<'a>(label: &'a str, handle: Option<iced::widget::svg::Handle>) -> El<'a> {
    let mut r = row![].spacing(4).align_y(Alignment::Center);
    if let Some(h) = handle {
        r = r.push(icon(h, 10, MUTED));
    }
    container(r.push(text(label).size(10.5).font(SEMIBOLD).color(MUTED))).padding([2, 7]).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 6.0)).into()
}

fn name_of(members: &[UserInfo], id: i64) -> String {
    members.iter().find(|u| u.id == id).map(|u| if u.display_name.is_empty() { u.username.clone() } else { u.display_name.clone() }).unwrap_or_else(|| format!("#{id}"))
}

/// A poll or quiz message.
pub fn poll_card<'a>(message_id: i64, c: PollContent, view: Option<&'a PollView>, me: i64, members: &'a [UserInfo], private: bool) -> El<'a> {
    let quiz = c.kind == "quiz";
    let multiple = c.multiple == Some(true);
    let state = view.and_then(|v| v.state.as_ref());
    let t = view.map(|v| &v.tally);
    let is_closed = state.is_some_and(closed);
    let answered = t.is_some_and(|t| t.mine.is_some());
    let show = is_closed || answered;
    let can_change = !quiz && !is_closed;
    let busy = view.is_some_and(|v| v.busy) || state.is_none();
    let total = t.map_or(0, |t| t.total);
    let accent_kind = if quiz { AMBER_TEXT } else { INDIGO_SOFT };

    let mut chips = row![].spacing(6);
    if c.anonymous == Some(true) {
        chips = chips.push(chip("Anonymous", Some(icons::eye_off())));
    }
    if multiple {
        chips = chips.push(chip("Multiple choice", None));
    }
    if private {
        chips = chips.push(chip("Encrypted", Some(icons::lock())));
    }
    let head = row![
        row![icon(if quiz { icons::graduation_cap() } else { icons::chart_bar() }, 13, accent_kind), text(if quiz { "QUIZ" } else { "POLL" }).size(11).font(BOLD).color(accent_kind)].spacing(6).align_y(Alignment::Center),
        space().width(Fill),
        chips,
    ]
    .align_y(Alignment::Center);

    let mut options: Column<'a, Message> = column![].spacing(6);
    for o in &c.options {
        let count = t.and_then(|t| t.counts.get(&o.id)).copied().unwrap_or(0);
        let pct = if total > 0 { (count as f32 / total as f32 * 100.0).round() } else { 0.0 };
        let mine = t.and_then(|t| t.mine.as_ref()).is_some_and(|ids| ids.contains(&o.id));
        let selected = view.is_some_and(|v| v.pending.contains(&o.id));
        let right = quiz && show && c.correct.as_deref() == Some(o.id.as_str());
        let wrong = quiz && show && mine && !right;
        let accent = if right { GREEN } else if wrong { DANGER_TEXT } else if mine || selected { INDIGO } else { hex(0x475569) };
        let clickable = !is_closed && !busy && !(quiz && answered) && !(answered && !can_change && !multiple);
        let mark: El = if selected || (mine && !wrong) || right {
            icon(icons::check(), 11, Color::WHITE).into()
        } else if wrong {
            icon(icons::x(), 11, Color::WHITE).into()
        } else {
            space().width(11).height(11).into()
        };
        let filled = selected || mine || right || wrong;
        let marker = container(mark).width(18).height(18).center(18).style(move |_| iced::widget::container::Style {
            background: filled.then(|| accent.into()),
            border: iced::Border { color: if filled { accent } else { rgba(0xffffff, 0.3) }, width: 1.5, radius: if multiple { 5.0.into() } else { 9.0.into() } },
            ..Default::default()
        });
        let mut line = row![marker, text(o.text.clone()).size(13.5).color(TEXT).width(Fill)].spacing(10).align_y(Alignment::Center);
        if show {
            line = line.push(text(format!("{pct:.0}%")).size(12.5).font(BOLD).color(hex(0xcbd5e1)));
        }
        let mut body = column![line].spacing(5);
        if show {
            body = body.push(progress_bar(0.0..=100.0, pct).girth(4).style(move |_| iced::widget::progress_bar::Style {
                background: rgba(0xffffff, 0.05).into(),
                bar: Color { a: 0.55, ..accent }.into(),
                border: iced::Border { radius: 2.0.into(), ..Default::default() },
            }));
            let voters = if c.anonymous == Some(true) { Vec::new() } else { t.and_then(|t| t.voters.get(&o.id)).cloned().unwrap_or_default() };
            if !voters.is_empty() {
                let mut names: Vec<String> = voters.iter().take(3).map(|id| name_of(members, *id)).collect();
                if voters.len() > 3 {
                    names.push(format!("+{}", voters.len() - 3));
                }
                body = body.push(text(names.join(", ")).size(11).color(TEXT_SECONDARY));
            }
        }
        let border = if filled { accent } else { rgba(0xffffff, 0.08) };
        options = options.push(
            button(body)
                .width(Fill)
                .padding([9, 12])
                .style(move |_, status| button::Style {
                    background: Some(if matches!(status, button::Status::Hovered) && clickable { rgba(0xffffff, 0.05) } else { rgba(0xffffff, 0.02) }.into()),
                    border: iced::Border { color: border, width: 1.0, radius: 10.0.into() },
                    text_color: TEXT,
                    ..Default::default()
                })
                .on_press_maybe(clickable.then(|| m(PollMsg::Pick(message_id, o.id.clone())))),
        );
    }

    let mut col = column![head, text(c.question.clone()).size(15).font(BOLD).color(Color::WHITE), options].spacing(12);
    if multiple && !is_closed && view.is_some_and(|v| !v.pending.is_empty()) {
        col = col.push(
            button(container(text(if answered { "Update vote" } else { "Vote" }).size(13).font(BOLD)).center_x(Fill))
                .width(Fill)
                .padding([8, 14])
                .style(primary)
                .on_press_maybe((!busy).then(|| m(PollMsg::Vote(message_id)))),
        );
    }
    if quiz && answered {
        if let Some(e) = c.explanation.as_deref().filter(|e| !e.is_empty()) {
            col = col.push(container(text(format!("💡 {e}")).size(12.5).color(hex(0xcbd5e1)).line_height(1.45)).width(Fill).padding([8, 10]).style(boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 8.0)));
        }
    }
    let mut foot = format!("{total} {}", if quiz { if total == 1 { "answer" } else { "answers" } } else if total == 1 { "vote" } else { "votes" });
    if quiz && show && total > 0 {
        let right = c.correct.as_ref().and_then(|id| t.and_then(|t| t.counts.get(id))).copied().unwrap_or(0);
        foot.push_str(&format!(" · {right} correct"));
    }
    if let Some(u) = t.map(|t| t.unreadable).filter(|u| *u > 0) {
        foot.push_str(&format!(" · {u} unreadable on this computer"));
    }
    if !show && !is_closed {
        foot.push_str(if quiz { " · answer to see results" } else { " · vote to see results" });
    }
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    if is_closed {
        right = right.push(text("Closed").size(11.5).font(BOLD).color(DANGER_TEXT));
    } else if let Some(at) = state.and_then(|s| s.closes_at.as_deref()) {
        right = right.push(row![icon(icons::clock(), 11, TEXT_SECONDARY), text(time_left(at)).size(11.5).color(TEXT_SECONDARY)].spacing(4).align_y(Alignment::Center));
    }
    if !is_closed && state.and_then(|s| s.creator_id) == Some(me) {
        right = right.push(button(text("Close").size(11.5).font(BOLD).color(INDIGO_SOFT)).padding(0).style(link_button).on_press(m(PollMsg::End(message_id))));
    }
    col = col.push(row![text(foot).size(11.5).color(TEXT_SECONDARY).width(Fill), right].spacing(8).align_y(Alignment::Center));
    container(col).max_width(440).padding(14).style(boxed(hex(0x12131c), rgba(0x6366f1, 0.22), 14.0)).into()
}

/// The posted result of a draw: attachment_data (written by the server) and the title.
pub fn draw_card<'a>(data: &'a Value, title: &'a str, me: i64, members: &'a [UserInfo]) -> El<'a> {
    let pink = hex(0xf9a8d4);
    let num = |k: &str| data.get(k).and_then(Value::as_i64).unwrap_or(0);
    let mut list: Column<'a, Message> = column![].spacing(6);
    for (i, w) in data.get("winners").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let id = w.get("id").and_then(Value::as_i64).unwrap_or(0);
        let username = w.get("username").and_then(Value::as_str).unwrap_or("");
        let shown = w.get("display_name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(username);
        let you = id == me;
        let mut line = row![
            container(text((i + 1).to_string()).size(11).font(BOLD).color(pink)).width(22).height(22).center(22).style(boxed(rgba(0xf472b6, 0.2), Color::TRANSPARENT, 11.0)),
            text(shown.to_string()).size(13).font(SEMIBOLD).color(Color::WHITE),
            text(format!("@{username}")).size(12).color(TEXT_SECONDARY),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if you {
            line = line.push(space().width(Fill)).push(text("you").size(11).font(BOLD).color(GREEN));
        }
        list = list.push(container(line).width(Fill).padding([6, 10]).style(boxed(if you { rgba(0x34d399, 0.12) } else { rgba(0xffffff, 0.04) }, if you { rgba(0x34d399, 0.4) } else { rgba(0xffffff, 0.06) }, 8.0)));
    }
    let by = name_of(members, num("drawn_by"));
    let excl = if data.get("include_me").and_then(Value::as_bool) == Some(false) { " (excluding themselves)" } else { "" };
    let foot = format!("{} of {} members · drawn by {by}{excl} · picked by Accordo", num("count"), num("pool_size"));
    container(
        column![
            row![icon(icons::dices(), 16, pink), text(if title.trim().is_empty() { "Random draw".to_string() } else { title.to_string() }).size(14).font(BOLD).color(pink)].spacing(8).align_y(Alignment::Center),
            list,
            text(foot).size(11).color(TEXT_SECONDARY),
        ]
        .spacing(10),
    )
    .max_width(380)
    .padding(14)
    .style(boxed(rgba(0xf472b6, 0.06), rgba(0xf472b6, 0.3), 12.0))
    .into()
}

fn toggle_line<'a>(on: bool, title: &'a str, hint: &'a str, msg: fn(bool) -> PollMsg) -> El<'a> {
    row![toggler(on).size(18).style(switch).on_toggle(move |v| m(msg(v))), column![text(title).size(13).font(SEMIBOLD).color(TEXT), text(hint).size(11.5).color(TEXT_SECONDARY)].spacing(1)]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn close_x<'a>(on: PollMsg) -> El<'a> {
    button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(on)).into()
}

/// The new poll / quiz window.
pub fn poll_modal<'a>(f: &'a PollForm) -> El<'a> {
    let kind_btn = |quiz: bool, label: &'a str, hint: &'a str| -> El<'a> {
        let on = f.quiz == quiz;
        button(
            container(
                row![icon(if quiz { icons::graduation_cap() } else { icons::chart_bar() }, 15, if on { hex(0xc7d2fe) } else { MUTED }), text(label).size(13).font(SEMIBOLD), text(format!("· {hint}")).size(11.5)]
                    .spacing(7)
                    .align_y(Alignment::Center),
            )
            .center_x(Fill),
        )
        .width(Fill)
        .padding([9, 10])
        .style(move |_, _| button::Style {
            background: on.then(|| rgba(0x6366f1, 0.25).into()),
            text_color: if on { hex(0xc7d2fe) } else { MUTED },
            border: iced::Border { radius: 8.0.into(), ..Default::default() },
            ..Default::default()
        })
        .on_press(m(PollMsg::Kind(quiz)))
        .into()
    };
    let label = |t: &'a str| text(t).size(12).font(SEMIBOLD).color(MUTED);
    let mut answers: Column<'a, Message> = column![].spacing(6);
    for (i, (id, t)) in f.answers.iter().enumerate() {
        let mut line = row![].spacing(6).align_y(Alignment::Center);
        if f.quiz {
            let on = f.correct.as_deref() == Some(id.as_str());
            line = line.push(
                button(container(if on { El::from(icon(icons::check(), 13, Color::WHITE)) } else { space().width(13).height(13).into() }).width(22).height(22).center(22))
                    .padding(0)
                    .style(move |_, _| button::Style {
                        background: on.then(|| GREEN.into()),
                        border: iced::Border { color: if on { GREEN } else { rgba(0xffffff, 0.2) }, width: 1.5, radius: 11.0.into() },
                        ..Default::default()
                    })
                    .on_press(m(PollMsg::Correct(i))),
            );
        }
        line = line.push(text_input(&format!("Answer {}", i + 1), t).on_input(move |v| m(PollMsg::Answer(i, v))).size(13.5).padding([8, 11]).style(field).width(Fill));
        if f.answers.len() > 2 {
            line = line.push(button(icon(icons::trash_2(), 15, MUTED)).padding(6).style(ghost(6.0, rgba(0xef4444, 0.1))).on_press(m(PollMsg::RemoveAnswer(i))));
        }
        answers = answers.push(line);
    }
    if f.answers.len() < MAX_OPTIONS {
        answers = answers.push(button(row![icon(icons::plus(), 14, INDIGO_SOFT), text("Add answer").size(12.5).font(SEMIBOLD).color(INDIGO_SOFT)].spacing(6).align_y(Alignment::Center)).padding([6, 10]).style(ghost(8.0, rgba(0x6366f1, 0.12))).on_press(m(PollMsg::AddAnswer)));
    }
    let mut col = column![
        row![text(if f.quiz { "New quiz" } else { "New poll" }).size(16).font(BOLD).color(Color::WHITE), space().width(Fill), close_x(PollMsg::CloseForm)].align_y(Alignment::Center),
        container(row![kind_btn(false, "Poll", "Ask for opinions"), kind_btn(true, "Quiz", "One right answer")].spacing(6)).padding(4).style(boxed(rgba(0x000000, 0.3), rgba(0xffffff, 0.06), 10.0)),
        column![label("Question"), text_input(if f.quiz { "Which chain has the lowest fees?" } else { "Where should we meet?" }, &f.question).on_input(|v| m(PollMsg::Question(v))).size(13.5).padding([9, 11]).style(field)].spacing(6),
        column![label(if f.quiz { "Answers — tick the correct one" } else { "Answers" }), answers].spacing(6),
    ]
    .spacing(16);
    if f.quiz {
        col = col.push(column![label("Explanation (optional)"), text_input("Shown after someone answers", &f.explanation).on_input(|v| m(PollMsg::Explanation(v))).size(13).padding([9, 11]).style(field)].spacing(6));
    }
    let mut opts = column![].spacing(10);
    if !f.quiz {
        opts = opts.push(toggle_line(f.multiple, "Multiple answers", "People can pick more than one option", PollMsg::Multiple));
    }
    opts = opts.push(toggle_line(f.anonymous, "Anonymous", "Nobody sees who voted for what — only totals", PollMsg::Anonymous));
    let mut ends = row![].spacing(4);
    for (label, hours) in DEADLINES {
        ends = ends.push(button(text(label).size(11.5).font(SEMIBOLD)).padding([5, 9]).style(pill(f.deadline == hours)).on_press(m(PollMsg::Deadline(hours))));
    }
    opts = opts.push(row![text("Ends after").size(13).font(SEMIBOLD).color(TEXT), space().width(Fill), ends].align_y(Alignment::Center));
    col = col.push(container(space().width(Fill).height(1)).style(crate::theme::rule)).push(opts);
    let blocker = f.blocker();
    let label = if f.busy { "Posting…".to_string() } else { blocker.map(str::to_string).unwrap_or_else(|| format!("Post {}", if f.quiz { "quiz" } else { "poll" })) };
    col = col.push(button(container(text(label).size(14).font(BOLD)).center_x(Fill)).width(Fill).padding([11, 16]).style(primary).on_press_maybe((blocker.is_none() && !f.busy).then(|| m(PollMsg::Post))));
    overlay::modal(container(iced::widget::scrollable(col).style(thin_scroll)).width(480).max_height(680).padding(20).style(modal), m(PollMsg::CloseForm))
}

/// The random draw window. `members`: the room's member count.
pub fn draw_modal<'a>(f: &'a DrawForm, members: i64) -> El<'a> {
    let pool = if f.include_me { members } else { (members - 1).max(0) };
    let valid = f.count >= 1 && f.count <= pool.min(100);
    let step = |label: &'a str, to: i64| -> El<'a> { button(container(text(label).size(15)).center_x(Fill)).width(40).padding([6, 0]).style(tone_button(Tone::Dark)).on_press(m(PollMsg::DrawCount(to))).into() };
    let col = column![
        row![icon(icons::dices(), 18, hex(0xf472b6)), text("Random draw").size(17).font(BOLD).color(Color::WHITE), space().width(Fill), close_x(PollMsg::CloseForm)].spacing(8).align_y(Alignment::Center),
        column![text("What for (optional)").size(12).color(MUTED), text_input("e.g. Who buys pizza", &f.title).on_input(|v| m(PollMsg::DrawTitle(v))).size(14).padding([10, 12]).style(field)].spacing(6),
        column![
            text("How many people").size(12).color(MUTED),
            row![step("−", (f.count - 1).max(1)), container(text(f.count.to_string()).size(15).font(BOLD).color(Color::WHITE)).width(70).center_x(70), step("+", (f.count + 1).min(pool.max(1))), text(format!("of {pool}")).size(12).color(MUTED)].spacing(8).align_y(Alignment::Center),
        ]
        .spacing(6),
        row![iced::widget::checkbox(f.include_me).on_toggle(|v| m(PollMsg::DrawIncludeMe(v))).size(16).style(tick), text("I can be picked too").size(13).color(TEXT)].spacing(8).align_y(Alignment::Center),
        text("Accordo picks on the server, from the room's members, and posts the result — nobody, including you, can choose or edit it.").size(12).color(MUTED).line_height(1.5),
        button(container(text(if f.busy { "Drawing…".to_string() } else if pool == 0 { "Nobody to draw from".to_string() } else { format!("Draw {}", f.count) }).size(14).font(BOLD)).center_x(Fill))
            .width(Fill)
            .padding(12)
            .style(primary)
            .on_press_maybe((valid && !f.busy).then(|| m(PollMsg::Draw))),
    ]
    .spacing(14);
    overlay::modal(container(col).width(Length::Fixed(420.0)).padding(18).style(modal), m(PollMsg::CloseForm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chat::polls::{PollOption, Vote};

    fn content(multiple: bool) -> PollContent {
        PollContent { kind: "poll".into(), options: vec![PollOption { id: "a".into(), text: "A".into() }, PollOption { id: "b".into(), text: "B".into() }], multiple: Some(multiple), ..Default::default() }
    }

    #[test]
    fn counts_only_what_a_vote_may_say() {
        let v = |u: i64, c: &str, mine: bool| Vote { user_id: Some(u), choice: c.into(), mine };
        let state = PollState { votes: vec![v(1, r#"{"o":["a","b","zzz"]}"#, true), v(2, r#"{"o":["b"]}"#, false), v(3, "not json", false), v(4, r#"{"o":["nope"]}"#, false)], ..Default::default() };
        let single = tally(&state, &content(false), false, None);
        assert_eq!((single.total, single.counts.get("a"), single.counts.get("b"), single.unreadable), (2, Some(&1), Some(&1), 1));
        assert_eq!(single.mine, Some(vec!["a".to_string()])); // only the first pick counts
        let multi = tally(&state, &content(true), false, None);
        assert_eq!((multi.counts.get("b"), multi.voters.get("b").map(Vec::len)), (Some(&2), Some(2)));
        // A private room without the key: nothing readable.
        assert_eq!(tally(&state, &content(false), true, None).unreadable, 4);
    }

    #[test]
    fn form_says_what_is_missing() {
        let mut f = PollForm::new(true);
        assert_eq!(f.blocker(), Some("Write a question"));
        f.question = "Q?".into();
        f.answers[0].1 = "Yes".into();
        f.answers[1].1 = " yes ".into();
        assert_eq!(f.blocker(), Some("Answers must be different"));
        f.answers[1].1 = "No".into();
        assert_eq!(f.blocker(), Some("Mark the correct answer"));
        f.correct = Some(f.answers[1].0.clone());
        assert_eq!(f.blocker(), None);
        let c = f.content();
        assert_eq!((c.kind.as_str(), c.options.len(), c.multiple, c.correct.is_some()), ("quiz", 2, None, true));
    }
}
