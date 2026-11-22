// SPDX-License-Identifier: GPL-3.0-or-later
//! Polls, quizzes and random draws: posting them (handleCreatePoll / handleRandomDraw), loading a
//! poll's votes and counting them, voting, ending one. Votes are sealed with the room's key in
//! private rooms (PollCard.tsx), so the server only ever stores ciphertext there.

use iced::Task;
use serde_json::json;

use super::chat_page::{room_keys, ChatMsg, State};
use crate::api::chat::polls::{self as api, PollContent};
use crate::api::chat::{self, DrawSettings, PollSettings, SendMessage};
use crate::api::client::Api;
use crate::app::Message;
use crate::components::chat::display;
use crate::components::chat::poll_card::{tally, DrawForm, PollForm, PollMsg};
use crate::e2ee::rooms;

fn done(id: i64) -> impl Fn(crate::api::client::ApiResult<api::PollState>) -> Message {
    move |r| Message::Chat(ChatMsg::Poll(PollMsg::Loaded(id, r.map(Box::new).map_err(|e| e.message))))
}

/// The poll's content in the message (opened), if this computer can read it.
fn content_of(state: &State, id: i64) -> Option<(PollContent, String)> {
    let m = super::chat_actions::find(state, id)?;
    let a = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none() && a.get("type").and_then(|t| t.as_str()) == Some("poll"))?;
    Some((serde_json::from_value(a.clone()).ok()?, m.room_id.clone()))
}

fn private(state: &State, room: &str) -> bool {
    state.rooms.get(room).is_some_and(display::is_encrypted_room)
}

/// Loads the polls on screen that aren't loaded yet.
pub fn want(state: &mut State, api: &Api) -> Task<Message> {
    let Some(feed) = state.selected.as_ref().and_then(|k| state.messages.get(k)) else { return Task::none() };
    let ids: Vec<i64> = feed.iter().rev().take(60).filter(|m| m.message_type == "poll" && m.id > 0).map(|m| m.id).filter(|id| !state.polls.contains_key(id)).collect();
    let mut jobs = Vec::new();
    for id in ids {
        state.polls.insert(id, Default::default());
        jobs.push(Task::perform(api::get(api.clone(), id), done(id)));
    }
    Task::batch(jobs)
}

/// poll_updated from the socket: someone voted, or it was closed.
pub fn on_update(state: &mut State, api: &Api, id: i64) -> Task<Message> {
    if state.polls.contains_key(&id) {
        Task::perform(api::get(api.clone(), id), done(id))
    } else {
        Task::none()
    }
}

fn vote(state: &mut State, api: &Api, id: i64, ids: Vec<String>) -> Result<Task<Message>, String> {
    let Some((_, room)) = content_of(state, id) else { return Ok(Task::none()) };
    let mut choice = json!({ "o": ids }).to_string();
    if private(state, &room) {
        let keys = room_keys(state, &room).ok_or("This room's key isn't on this computer yet.")?;
        choice = rooms::seal_text(&choice, &keys).ok_or("Couldn't encrypt the vote.")?;
    }
    if let Some(v) = state.polls.get_mut(&id) {
        v.busy = true;
    }
    Ok(Task::perform(api::vote(api.clone(), id, choice), done(id)))
}

/// Seals what goes in the message for a private room.
fn sealed(state: &mut State, room: &str, text: &str, attachment: Option<serde_json::Value>) -> Result<(String, Option<serde_json::Value>), String> {
    if !private(state, room) {
        return Ok((text.to_string(), attachment));
    }
    let keys = room_keys(state, room).ok_or("Can't send: this chat's key isn't on this computer.")?;
    let text = if text.is_empty() { String::new() } else { rooms::seal_text(text, &keys).ok_or("Couldn't encrypt it.")? };
    let attachment = match attachment {
        Some(a) => Some(rooms::seal_json(&a, &keys).ok_or("Couldn't encrypt it.")?),
        None => None,
    };
    Ok((text, attachment))
}

pub fn update(state: &mut State, m: PollMsg, api: &Api) -> (Task<Message>, Option<String>) {
    match m {
        PollMsg::New(quiz) => {
            state.panel = None;
            state.poll_form = Some(PollForm::new(quiz));
        }
        PollMsg::NewDraw => {
            state.panel = None;
            state.draw_form = Some(DrawForm::default());
        }
        PollMsg::CloseForm => {
            state.poll_form = None;
            state.draw_form = None;
        }
        PollMsg::Kind(quiz) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.quiz = quiz;
            }
        }
        PollMsg::Question(v) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.question = v.chars().take(300).collect();
            }
        }
        PollMsg::Answer(i, v) => {
            if let Some(a) = state.poll_form.as_mut().and_then(|f| f.answers.get_mut(i)) {
                a.1 = v.chars().take(120).collect();
            }
        }
        PollMsg::AddAnswer => {
            if let Some(f) = state.poll_form.as_mut() {
                f.add();
            }
        }
        PollMsg::RemoveAnswer(i) => {
            if let Some(f) = state.poll_form.as_mut().filter(|f| f.answers.len() > 2 && i < f.answers.len()) {
                let (id, _) = f.answers.remove(i);
                if f.correct.as_deref() == Some(id.as_str()) {
                    f.correct = None;
                }
            }
        }
        PollMsg::Correct(i) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.correct = f.answers.get(i).map(|a| a.0.clone());
            }
        }
        PollMsg::Explanation(v) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.explanation = v.chars().take(400).collect();
            }
        }
        PollMsg::Multiple(v) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.multiple = v;
            }
        }
        PollMsg::Anonymous(v) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.anonymous = v;
            }
        }
        PollMsg::Deadline(h) => {
            if let Some(f) = state.poll_form.as_mut() {
                f.deadline = h;
            }
        }
        PollMsg::Post => {
            let (Some(room), Some(f)) = (state.selected.clone(), state.poll_form.as_ref()) else { return (Task::none(), None) };
            if f.busy || f.blocker().is_some() {
                return (Task::none(), None);
            }
            let content = f.content();
            let kind = content.kind.clone();
            let settings = PollSettings {
                kind: kind.clone(),
                anonymous: f.anonymous,
                closes_at: Some(f.deadline.map(|h| (chrono::Utc::now() + chrono::Duration::hours(h)).to_rfc3339())),
            };
            let text = format!("{}: {}", if kind == "quiz" { "🧠 Quiz" } else { "📊 Poll" }, content.question);
            let (text, att) = match sealed(state, &room, &text, serde_json::to_value(&content).ok()) {
                Ok(v) => v,
                Err(e) => return (Task::none(), Some(e)),
            };
            if let Some(f) = state.poll_form.as_mut() {
                f.busy = true;
            }
            let body = SendMessage { room_id: Some(room), content: Some(text), message_type: Some("poll".into()), attachment_data: att, poll: Some(settings), ..Default::default() };
            return (Task::perform(chat::send_message(api.clone(), body), |r| Message::Chat(ChatMsg::Sent(r.map(Box::new)))), None);
        }
        PollMsg::DrawTitle(v) => {
            if let Some(f) = state.draw_form.as_mut() {
                f.title = v.chars().take(200).collect();
            }
        }
        PollMsg::DrawCount(n) => {
            if let Some(f) = state.draw_form.as_mut() {
                f.count = n.clamp(1, 100);
            }
        }
        PollMsg::DrawIncludeMe(v) => {
            if let Some(f) = state.draw_form.as_mut() {
                f.include_me = v;
            }
        }
        PollMsg::Draw => {
            let (Some(room), Some(f)) = (state.selected.clone(), state.draw_form.as_ref()) else { return (Task::none(), None) };
            if f.busy {
                return (Task::none(), None);
            }
            let (count, include_me, title) = (f.count, f.include_me, f.title.trim().to_string());
            // Only the title is ours (encrypted in private rooms); the server picks and writes the result.
            let (text, _) = match sealed(state, &room, &title, None) {
                Ok(v) => v,
                Err(e) => return (Task::none(), Some(e)),
            };
            if let Some(f) = state.draw_form.as_mut() {
                f.busy = true;
            }
            let body = SendMessage { room_id: Some(room), content: Some(text), message_type: Some("random_draw".into()), draw: Some(DrawSettings { count, include_me }), ..Default::default() };
            return (Task::perform(chat::send_message(api.clone(), body), |r| Message::Chat(ChatMsg::Sent(r.map(Box::new)))), None);
        }
        PollMsg::Loaded(id, r) => match r {
            Ok(s) => {
                let counted = content_of(state, id).map(|(c, room)| {
                    let private = private(state, &room);
                    let keys = room_keys(state, &room);
                    tally(&s, &c, private, keys.as_deref())
                });
                let v = state.polls.entry(id).or_default();
                v.busy = false;
                if let Some(t) = counted {
                    v.tally = t;
                }
                // What was ticked and is now counted as mine isn't pending any more.
                if v.tally.mine.is_some() {
                    v.pending.clear();
                }
                v.state = Some(*s);
            }
            Err(e) => {
                if let Some(v) = state.polls.get_mut(&id) {
                    v.busy = false;
                }
                return (Task::none(), Some(e));
            }
        },
        PollMsg::Pick(id, option) => {
            let Some((c, _)) = content_of(state, id) else { return (Task::none(), None) };
            let Some(v) = state.polls.get_mut(&id) else { return (Task::none(), None) };
            if c.multiple == Some(true) {
                match v.pending.iter().position(|x| *x == option) {
                    Some(i) => {
                        v.pending.remove(i);
                    }
                    None => v.pending.push(option),
                }
                return (Task::none(), None);
            }
            if v.tally.mine.as_ref().is_some_and(|m| m.first() == Some(&option)) {
                return (Task::none(), None);
            }
            return match vote(state, api, id, vec![option]) {
                Ok(t) => (t, None),
                Err(e) => (Task::none(), Some(e)),
            };
        }
        PollMsg::Vote(id) => {
            let picks = state.polls.get(&id).map(|v| v.pending.clone()).unwrap_or_default();
            if picks.is_empty() {
                return (Task::none(), None);
            }
            return match vote(state, api, id, picks) {
                Ok(t) => (t, None),
                Err(e) => (Task::none(), Some(e)),
            };
        }
        PollMsg::End(id) => {
            if let Some(v) = state.polls.get_mut(&id) {
                v.busy = true;
            }
            return (Task::perform(api::close(api.clone(), id), done(id)), None);
        }
    }
    (Task::none(), None)
}

/// A message was sent: the poll / draw window it came from goes.
pub fn on_sent(state: &mut State, ok: bool) {
    if ok {
        state.poll_form = state.poll_form.take().filter(|f| !f.busy);
        state.draw_form = state.draw_form.take().filter(|f| !f.busy);
    } else {
        if let Some(f) = state.poll_form.as_mut() {
            f.busy = false;
        }
        if let Some(f) = state.draw_form.as_mut() {
            f.busy = false;
        }
    }
}
