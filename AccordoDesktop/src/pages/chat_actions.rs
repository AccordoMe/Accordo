// SPDX-License-Identifier: GPL-3.0-or-later
//! What you do with a message (useChatMessages.ts, ChatPage.tsx, ThreadPanel.tsx): react, reply,
//! edit, delete, pin, copy, and threads. In encrypted rooms everything you write — a reply's quote,
//! an edit, a thread reply — is sealed with the room key first, never sent as plaintext.

use std::collections::HashMap;

use iced::Task;
use serde_json::{json, Value};

use super::chat_page::{room_keys, ChatMsg, State, COMPOSER};
use crate::api::chat::threads::{self, Thread as ThreadData};
use crate::api::chat::{self, ChatMessageItem, SendMessage, StatusReply, ThreadSummary};
use crate::api::client::{Api, ApiError};
use crate::app::Message;
use crate::components::chat::display;
use crate::components::chat::message_list::MenuKind;
use crate::components::chat::thread_panel::{Thread, THREAD_INPUT};
use crate::e2ee::rooms;

#[derive(Debug, Clone)]
pub enum ActionResult {
    Reacted(i64, Option<HashMap<String, Vec<i64>>>, Result<StatusReply, ApiError>),
    Edited(i64, String, Result<(), ApiError>),
    Deleted(i64, Result<(), ApiError>),
    Pinned(i64, Result<StatusReply, ApiError>),
    ThreadLoaded(i64, Result<Box<ThreadData>, ApiError>),
    ThreadSent(i64, String, Result<Box<ChatMessageItem>, ApiError>),
}

fn done(r: ActionResult) -> Message {
    Message::Chat(ChatMsg::Action(Box::new(r)))
}

/// A message on screen: in the open room's feed or the open thread.
pub fn find(state: &State, id: i64) -> Option<&ChatMessageItem> {
    let in_feed = state.selected.as_ref().and_then(|k| state.messages.get(k)).and_then(|f| f.iter().find(|m| m.id == id));
    in_feed.or_else(|| state.thread.as_ref().and_then(|t| t.find(id)))
}

/// Changes every copy of a message on screen (feeds and the thread).
pub fn change(state: &mut State, id: i64, mut f: impl FnMut(&mut ChatMessageItem)) {
    for feed in state.messages.values_mut() {
        if let Some(m) = feed.iter_mut().find(|m| m.id == id) {
            f(m);
        }
    }
    if let Some(m) = state.thread.as_mut().and_then(|t| t.find_mut(id)) {
        f(m);
    }
}

fn is_encrypted(state: &State, room_key: &str) -> bool {
    state.rooms.get(room_key).is_some_and(display::is_encrypted_room)
}

/// Seals text for the room (encrypted rooms), or passes it (public).
fn seal(state: &mut State, room_key: &str, text: &str) -> Result<String, String> {
    if !is_encrypted(state, room_key) {
        return Ok(text.to_string());
    }
    let keys = room_keys(state, room_key).ok_or("This chat's key isn't on this computer.")?;
    rooms::seal_text(text, &keys).ok_or_else(|| "Encrypting failed.".into())
}

pub fn react(state: &mut State, api: &Api, id: i64, emoji: String) -> Task<Message> {
    state.menu = None;
    let me = state.me_id;
    let before = find(state, id).and_then(|m| m.reactions.clone());
    change(state, id, |m| {
        let list = m.reactions.get_or_insert_with(HashMap::new);
        let users = list.entry(emoji.clone()).or_default();
        if let Some(i) = users.iter().position(|u| *u == me) {
            users.remove(i);
        } else {
            users.push(me);
        }
        list.retain(|_, u| !u.is_empty());
    });
    Task::perform(chat::toggle_reaction(api.clone(), id, emoji), move |r| done(ActionResult::Reacted(id, before.clone(), r)))
}

pub fn reply(state: &mut State, id: i64) -> Task<Message> {
    state.menu = None;
    if let Some(m) = find(state, id).cloned() {
        if state.editing.take().is_some() {
            state.draft = std::mem::take(&mut state.saved_draft);
        }
        state.reply_to = Some(m);
    }
    iced::widget::operation::focus(COMPOSER)
}

pub fn edit(state: &mut State, id: i64) -> Task<Message> {
    state.menu = None;
    let Some(m) = find(state, id).cloned() else { return Task::none() };
    if state.editing.is_none() {
        state.saved_draft = std::mem::take(&mut state.draft);
    }
    state.editing = Some(id);
    state.reply_to = None;
    state.draft = m.content;
    iced::widget::operation::focus(COMPOSER)
}

pub fn cancel_edit(state: &mut State) {
    if state.editing.take().is_some() {
        state.draft = std::mem::take(&mut state.saved_draft);
    }
}

/// Enter while editing: the new text, sealed like a send.
pub fn save_edit(state: &mut State, api: &Api, id: i64) -> Result<Task<Message>, String> {
    let text = state.draft.trim().to_string();
    let room_key = find(state, id).map(|m| m.room_id.clone()).ok_or("That message isn't here any more.")?;
    if text.is_empty() {
        return Ok(Task::none());
    }
    if find(state, id).is_some_and(|m| m.content == text) {
        cancel_edit(state);
        return Ok(Task::none());
    }
    let content = seal(state, &room_key, &text)?;
    state.sending = true;
    Ok(Task::perform(chat::edit_message(api.clone(), id, content), move |r| done(ActionResult::Edited(id, text.clone(), r.map(|_| ())))))
}

/// The quote a reply carries (reply_to_data), sealed in encrypted rooms.
pub fn reply_payload(state: &mut State, room_key: &str) -> Result<(Option<i64>, Option<Value>), String> {
    let Some(to) = state.reply_to.clone() else { return Ok((None, None)) };
    let data = json!({
        "id": to.id,
        "sender_username": to.sender_username,
        "sender_display_name": to.sender_display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| to.sender_username.clone()),
        "content": to.content,
        "message_type": to.message_type,
    });
    if !is_encrypted(state, room_key) {
        return Ok((Some(to.id), Some(data)));
    }
    let keys = room_keys(state, room_key).ok_or("This chat's key isn't on this computer.")?;
    let sealed = rooms::seal_json(&data, &keys).ok_or("Encrypting the reply failed.")?;
    Ok((Some(to.id), Some(sealed)))
}

pub fn delete(state: &mut State, api: &Api, id: i64) -> Task<Message> {
    state.menu = None;
    Task::perform(chat::delete_message(api.clone(), id), move |r| done(ActionResult::Deleted(id, r.map(|_| ()))))
}

pub fn toggle_pin(state: &mut State, api: &Api, id: i64) -> Task<Message> {
    state.menu = None;
    Task::perform(chat::toggle_pin(api.clone(), id), move |r| done(ActionResult::Pinned(id, r)))
}

pub fn copy(state: &mut State, id: i64, what_id: bool) -> Task<Message> {
    state.menu = None;
    let Some(m) = find(state, id) else { return Task::none() };
    let (value, note) = if what_id { (m.id.to_string(), "Message ID copied") } else { (display::message_text(m).0, "Message copied!") };
    state.notices.push(note.into());
    iced::clipboard::write(value)
}

pub fn open_thread(state: &mut State, api: &Api, root_id: i64) -> Task<Message> {
    state.menu = None;
    state.search = None; // one side panel at a time
    let Some(room_key) = state.selected.clone() else { return Task::none() };
    let root = find(state, root_id).cloned();
    state.thread = Some(Thread::new(root_id, room_key, root));
    Task::batch([
        Task::perform(threads::get(api.clone(), root_id), move |r| done(ActionResult::ThreadLoaded(root_id, r.map(Box::new)))),
        iced::widget::operation::focus(THREAD_INPUT),
    ])
}

pub fn send_in_thread(state: &mut State, api: &Api) -> Result<Task<Message>, String> {
    let Some(t) = state.thread.as_ref() else { return Ok(Task::none()) };
    let text = t.draft.trim().to_string();
    if text.is_empty() || t.sending {
        return Ok(Task::none());
    }
    let (room_key, root_id) = (t.room_key.clone(), t.root_id);
    let content = seal(state, &room_key, &text)?;
    if let Some(t) = state.thread.as_mut() {
        t.sending = true;
    }
    let body = SendMessage { room_id: Some(room_key), content: Some(content), message_type: Some("text".into()), thread_root_id: Some(root_id), ..Default::default() };
    Ok(Task::perform(chat::send_message(api.clone(), body), move |r| done(ActionResult::ThreadSent(root_id, text.clone(), r.map(Box::new)))))
}

/// The feed's "N replies" under a root, after a reply arrived (the server's summary if it sent one).
pub fn bump_summary(state: &mut State, root_id: i64, reply: &ChatMessageItem, summary: Option<ThreadSummary>) {
    change(state, root_id, |root| {
        let s = summary.clone().unwrap_or_else(|| {
            let mut s = root.thread_summary.clone().unwrap_or_default();
            s.count += 1;
            s.last_reply_at = Some(reply.created_at.clone());
            if let Some(id) = reply.sender_id.filter(|id| !s.participant_ids.contains(id)) {
                s.participant_ids.push(id);
            }
            s
        });
        root.thread_summary = Some(s);
    });
}

/// A reply in a thread arrived (socket): into the open thread, and the root's summary.
pub fn on_thread_reply(state: &mut State, m: ChatMessageItem, summary: Option<ThreadSummary>) {
    let Some(root_id) = m.thread_root_id else { return };
    let keys = room_keys(state, &m.room_id);
    let m = rooms::open_message(m, keys.as_deref());
    let already = state.thread.as_ref().is_some_and(|t| t.replies.iter().any(|x| x.id == m.id));
    if !already {
        bump_summary(state, root_id, &m, summary);
    }
    if let Some(t) = state.thread.as_mut().filter(|t| t.root_id == root_id) {
        if !already {
            t.replies.push(m);
        }
    }
}

pub fn on_result(state: &mut State, result: ActionResult, api: &Api) -> (Task<Message>, Option<String>) {
    let _ = api;
    let mut error = None;
    let task = match result {
        ActionResult::Reacted(id, before, result) => {
            match result {
                Ok(reply) => {
                    if let Some(r) = reply.reactions {
                        change(state, id, |m| m.reactions = Some(r.clone()));
                    }
                }
                Err(e) => {
                    change(state, id, |m| m.reactions = before.clone());
                    error = Some(format!("Couldn't react: {}", e.message));
                }
            }
            Task::none()
        }
        ActionResult::Edited(id, text, result) => {
            state.sending = false;
            match result {
                Ok(()) => {
                    change(state, id, |m| {
                        m.content = text.clone();
                        m.is_edited = Some(true);
                    });
                    cancel_edit(state);
                }
                Err(e) => error = Some(format!("Couldn't edit: {}", e.message)),
            }
            iced::widget::operation::focus(COMPOSER)
        }
        ActionResult::Deleted(id, result) => {
            match result {
                Ok(()) => {
                    for feed in state.messages.values_mut() {
                        feed.retain(|m| m.id != id);
                    }
                    if let Some(t) = state.thread.as_mut() {
                        t.replies.retain(|m| m.id != id);
                    }
                    if state.reply_to.as_ref().is_some_and(|m| m.id == id) {
                        state.reply_to = None;
                    }
                    state.notices.push("Message deleted".into());
                }
                Err(e) => error = Some(format!("Couldn't delete: {}", e.message)),
            }
            Task::none()
        }
        ActionResult::Pinned(id, result) => {
            match result {
                Ok(reply) => {
                    let pinned = reply.is_pinned.unwrap_or(false);
                    change(state, id, |m| m.is_pinned = Some(pinned));
                    state.notices.push(if pinned { "Message pinned".into() } else { "Message unpinned".into() });
                }
                Err(e) => error = Some(format!("Couldn't pin: {}", e.message)),
            }
            Task::none()
        }
        ActionResult::ThreadLoaded(root_id, result) => {
            let room_key = state.thread.as_ref().filter(|t| t.root_id == root_id).map(|t| t.room_key.clone());
            if let Some(room_key) = room_key {
                match result {
                    Ok(data) => {
                        let keys = room_keys(state, &room_key);
                        let ThreadData { root, replies } = *data;
                        let root = rooms::open_message(root, keys.as_deref());
                        let mut replies: Vec<ChatMessageItem> = replies.into_iter().map(|m| rooms::open_message(m, keys.as_deref())).collect();
                        replies.sort_by_key(|m| m.id);
                        if let Some(t) = state.thread.as_mut() {
                            t.root = Some(root);
                            t.replies = replies;
                            t.loading = false;
                        }
                    }
                    Err(e) => {
                        if let Some(t) = state.thread.as_mut() {
                            t.loading = false;
                        }
                        error = Some(format!("Couldn't load the thread: {}", e.message));
                    }
                }
            }
            Task::none()
        }
        ActionResult::ThreadSent(root_id, text, result) => {
            if let Some(t) = state.thread.as_mut() {
                t.sending = false;
            }
            match result {
                Ok(sent) => {
                    let mut m = *sent;
                    m.content = text;
                    m.thread_root_id = Some(root_id);
                    if let Some(t) = state.thread.as_mut().filter(|t| t.root_id == root_id) {
                        t.draft.clear();
                    }
                    on_thread_reply(state, m, None);
                }
                Err(e) => error = Some(format!("Couldn't send: {}", e.message)),
            }
            iced::widget::operation::focus(THREAD_INPUT)
        }
    };
    (task, error)
}

pub fn open_menu(state: &mut State, id: i64, kind: MenuKind) {
    state.menu = if state.menu == Some((id, kind)) { None } else { Some((id, kind)) };
}
