// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Manage Room window does (the room handlers of ChatPage.tsx): renaming, settings,
//! members, invite links, join requests — and handing the room key on.
//!
//! The server never has a room key, so it can't give one to a new member: a member who holds it
//! wraps a copy for them (healRoomKeyEnvelopes) — only for people whose admission is provable
//! without trusting the server (DM participant, owner tag, invite tag). Approving someone (a join
//! request, or a member awaiting approval) issues an owner tag for their key and then shares.

use std::collections::{HashMap, HashSet};

use iced::Task;
use serde_json::{json, Value};

use super::chat_page::{room_keys, ChatMsg, State};
use crate::api::chat::{self, ChatRoomItem, EnvelopesUpdate, InviteRules, InviteStatus, JoinDecision, JoinRequests, KeyRotation, MemberAuthEntry, MemberAuthPatch, MemberAuthUpdate, RoomRole, RoomUpdate};
use crate::api::client::{normalize_base, Api, ApiError};
use crate::app::Message;
use crate::components::chat::manage_room::{self, Confirm, ManageMsg, Tab, ROLE_COLORS};
use crate::e2ee::crypto::{encrypt_symmetric, generate_room_key, seal_archived_keys};
use crate::e2ee::rooms;
use crate::store::room_permissions::{self as perms, Perm};

#[derive(Debug, Clone)]
pub enum AdminResult {
    Saved(String, Result<Box<ChatRoomItem>, ApiError>),
    /// The room's picture changed (true) or was removed (false).
    Avatar(String, bool, Result<Box<ChatRoomItem>, ApiError>),
    /// A room setting changed (already shown); the server's settings come back.
    Updated(String, Result<Box<ChatRoomItem>, ApiError>),
    Added(String, HashSet<i64>, Result<Box<ChatRoomItem>, ApiError>),
    Kicked(String, Result<(), ApiError>),
    Invite(String, Result<InviteStatus, ApiError>),
    InviteCreated(String, Result<String, String>),
    Revoked(String, Result<(), ApiError>),
    Everyone(String, Result<&'static str, String>),
    Requests(String, Result<JoinRequests, ApiError>),
    Decided(String, i64, bool, Result<Box<JoinDecision>, ApiError>),
    /// Owner tags issued; `true`: share the room key next.
    Authorized(String, Result<MemberAuthUpdate, ApiError>, bool),
    Shared(String, Result<EnvelopesUpdate, ApiError>),
    Bootstrapped(String, Result<EnvelopesUpdate, ApiError>),
}

fn done(r: AdminResult) -> Message {
    Message::Chat(ChatMsg::Admin(Box::new(r)))
}

fn set_setting(room: &mut ChatRoomItem, key: &str, value: Value) {
    room.settings.get_or_insert_with(HashMap::new).insert(key.to_string(), value);
}

fn room_of(state: &State) -> Option<ChatRoomItem> {
    let key = &state.manage.as_ref()?.room_key;
    state.rooms.get(key).cloned()
}

pub fn access(state: &State, room: &ChatRoomItem) -> manage_room::Access {
    let has_key = room.key_envelopes.as_ref().is_some_and(|e| e.contains_key(&state.me_id.to_string())) && state.keys.has_device_key();
    manage_room::Access::of(room, state.me_id, state.is_admin, has_key)
}

/// Opens the window on the selected room.
pub fn open(state: &mut State, tab: Tab, api: &Api) -> Task<Message> {
    let Some(room) = state.selected_room().cloned() else { return Task::none() };
    state.manage = Some(manage_room::State::new(&room, tab));
    let acc = access(state, &room);
    let mut tasks = vec![];
    if manage_room::is_group(&room) && acc.manage_members {
        tasks.push(load_invite(api, &room.room_key));
    }
    if manage_room::can_see_requests(&room, &acc) {
        tasks.push(load_requests(api, &room.room_key));
    }
    Task::batch(tasks)
}

fn load_invite(api: &Api, key: &str) -> Task<Message> {
    let k = key.to_string();
    Task::perform(chat::get_invite(api.clone(), k.clone()), move |r| done(AdminResult::Invite(k.clone(), r)))
}

fn load_requests(api: &Api, key: &str) -> Task<Message> {
    let k = key.to_string();
    Task::perform(chat::get_join_requests(api.clone(), k.clone()), move |r| done(AdminResult::Requests(k.clone(), r)))
}

/// A `room_join_request` arrived: refresh what the open window shows.
pub fn on_join_request(state: &State, api: &Api, room_key: &str) -> Task<Message> {
    match &state.manage {
        Some(m) if m.room_key == room_key => Task::batch([load_requests(api, room_key), load_invite(api, room_key)]),
        _ => Task::none(),
    }
}

/// Changes the room: shown at once (`local`), sent, and the server's settings taken back.
fn patch(state: &mut State, api: &Api, key: &str, update: RoomUpdate, local: impl FnOnce(&mut ChatRoomItem)) -> Task<Message> {
    state.rooms.update_room(key, local);
    let k = key.to_string();
    Task::perform(chat::update_room(api.clone(), k.clone(), update), move |r| done(AdminResult::Updated(k.clone(), r.map(Box::new))))
}

fn permissions_update(state: &mut State, api: &Api, key: &str, update: RoomUpdate) -> Task<Message> {
    patch(state, api, key, RoomUpdate { permissions_version: Some(2), ..update }, |_| {})
}

pub fn update(state: &mut State, msg: ManageMsg, api: &Api) -> (Task<Message>, Option<String>) {
    let Some(room) = room_of(state) else {
        state.manage = None;
        return (Task::none(), None);
    };
    let key = room.room_key.clone();
    let Some(form) = state.manage.as_mut() else { return (Task::none(), None) };
    let task = match msg {
        ManageMsg::Close => {
            state.manage = None;
            Task::none()
        }
        ManageMsg::Tab(tab) => {
            form.tab = tab;
            if tab == Tab::Requests { load_requests(api, &key) } else { Task::none() }
        }
        ManageMsg::Title(v) => {
            form.title = v;
            Task::none()
        }
        ManageMsg::Topic(v) => {
            form.topic = v.chars().take(500).collect();
            Task::none()
        }
        ManageMsg::SaveGeneral => {
            let title = form.title.trim().to_string();
            if title.is_empty() || form.saving {
                return (Task::none(), None);
            }
            form.saving = true;
            let update = RoomUpdate { title: Some(title), topic: Some(form.topic.trim().to_string()), ..Default::default() };
            let k = key.clone();
            Task::perform(chat::update_room(api.clone(), key, update), move |r| done(AdminResult::Saved(k.clone(), r.map(Box::new))))
        }
        ManageMsg::AvatarPick => {
            return (Task::done(Message::Picker(crate::components::file_picker::PickerMsg::Open(crate::components::file_picker::Purpose::RoomAvatar))), None);
        }
        ManageMsg::Avatar(pic) => {
            if form.avatar_saving {
                return (Task::none(), None);
            }
            form.avatar_saving = true;
            let set = pic.is_some();
            // "" clears it on the server (ManageRoomModal: onAvatarChange('')).
            let update = RoomUpdate { avatar: Some(pic.unwrap_or_default()), ..Default::default() };
            let k = key.clone();
            Task::perform(chat::update_room(api.clone(), key, update), move |r| done(AdminResult::Avatar(k.clone(), set, r.map(Box::new))))
        }
        ManageMsg::AddRow(i, v) => {
            if let Some(row) = form.add_rows.get_mut(i) {
                *row = v;
            }
            Task::none()
        }
        ManageMsg::AddRowMore => {
            form.add_rows.push(String::new());
            Task::none()
        }
        ManageMsg::AddRowRemove(i) => {
            if form.add_rows.len() > 1 && i < form.add_rows.len() {
                form.add_rows.remove(i);
            }
            Task::none()
        }
        ManageMsg::AddPeople => {
            let names = form.add_usernames();
            if names.is_empty() || form.adding {
                return (Task::none(), None);
            }
            form.adding = true;
            let before: HashSet<i64> = room.allowed_user_ids.iter().copied().collect();
            let k = key.clone();
            Task::perform(chat::add_allowed_users(api.clone(), key, Some(names), None), move |r| done(AdminResult::Added(k.clone(), before.clone(), r.map(Box::new))))
        }
        ManageMsg::Expand(id) => {
            form.expanded = if form.expanded == Some(id) { None } else { Some(id) };
            Task::none()
        }
        ManageMsg::MemberRole(id, role) => {
            let update = RoomUpdate { member_roles: Some(HashMap::from([(id.to_string(), role)])), ..Default::default() };
            patch(state, api, &key, update, |_| {})
        }
        ManageMsg::MemberCustom(id, custom) => {
            let list = custom.then(|| perms::ids(&perms::default_permissions(&room)));
            permissions_update(state, api, &key, RoomUpdate { member_permissions: Some(HashMap::from([(id.to_string(), list)])), ..Default::default() })
        }
        ManageMsg::MemberPerm(id, p) => {
            let mut own = perms::custom_permissions(&room, id).unwrap_or_else(|| perms::default_permissions(&room));
            if !own.remove(&p) {
                own.insert(p);
            }
            permissions_update(state, api, &key, RoomUpdate { member_permissions: Some(HashMap::from([(id.to_string(), Some(perms::ids(&own)))])), ..Default::default() })
        }
        ManageMsg::DefaultPerm(p) => {
            let mut defaults = perms::default_permissions(&room);
            if !defaults.remove(&p) {
                defaults.insert(p);
            }
            permissions_update(state, api, &key, RoomUpdate { default_permissions: Some(perms::ids(&defaults)), ..Default::default() })
        }
        ManageMsg::Remove(id, name) => {
            form.confirm = Some(Confirm::Kick(id, name));
            Task::none()
        }
        ManageMsg::Approve(id) => authorize(state, api, &key, vec![id], true),
        ManageMsg::InviteEdit(on) => {
            form.invite_editing = on;
            Task::none()
        }
        ManageMsg::Expiry(v) => {
            form.expiry = v;
            Task::none()
        }
        ManageMsg::MaxUses(v) => {
            form.max_uses = v;
            Task::none()
        }
        ManageMsg::Approval(v) => {
            form.approval = v;
            Task::none()
        }
        ManageMsg::CreateInvite => {
            if form.generating {
                return (Task::none(), None);
            }
            form.generating = true;
            form.invite_editing = false;
            let rules = InviteRules { expires_in_hours: Some(form.expiry), max_uses: Some(form.max_uses), requires_approval: Some(form.approval) };
            let current = room_keys(state, &key).and_then(|k| k.into_iter().next());
            let k = key.clone();
            Task::perform(create_invite(api.clone(), key, rules, current), move |r| done(AdminResult::InviteCreated(k.clone(), r)))
        }
        ManageMsg::CopyInvite => match form.invite_link.clone() {
            Some(link) => {
                state.notices.push("Invite link copied!".into());
                iced::clipboard::write(link)
            }
            None => Task::none(),
        },
        ManageMsg::RevokeInvite => {
            form.confirm = Some(Confirm::RevokeInvite);
            Task::none()
        }
        ManageMsg::EveryoneCycle(p) => {
            match form.everyone.get(&p).copied() {
                None => form.everyone.insert(p, true),
                Some(true) => form.everyone.insert(p, false),
                Some(false) => form.everyone.remove(&p),
            };
            Task::none()
        }
        ManageMsg::EveryoneReset => {
            form.everyone.clear();
            Task::none()
        }
        ManageMsg::EveryoneApply => {
            let add: Vec<Perm> = form.everyone.iter().filter(|(_, v)| **v).map(|(p, _)| *p).collect();
            let remove: Vec<Perm> = form.everyone.iter().filter(|(_, v)| !**v).map(|(p, _)| *p).collect();
            everyone(state, api, &key, add, remove, "Permissions updated for everyone")
        }
        ManageMsg::ToggleChannel => {
            if perms::default_permissions(&room).contains(&Perm::SendMessages) {
                form.confirm = Some(Confirm::MakeChannel);
                Task::none()
            } else {
                everyone(state, api, &key, vec![Perm::SendMessages], vec![], "Everyone can post again")
            }
        }
        ManageMsg::Decide(id, approve) => {
            form.deciding = Some(id);
            let k = key.clone();
            Task::perform(chat::decide_join_request(api.clone(), key, id, approve), move |r| done(AdminResult::Decided(k.clone(), id, approve, r.map(Box::new))))
        }
        ManageMsg::SlowMode(seconds) => patch(state, api, &key, RoomUpdate { slow_mode_seconds: Some(seconds), ..Default::default() }, |r| set_setting(r, "slow_mode_seconds", json!(seconds))),
        ManageMsg::AutoBurn(v) => {
            let value = v.clone();
            patch(state, api, &key, RoomUpdate { auto_burn: Some(v), ..Default::default() }, move |r| set_setting(r, "auto_burn", json!(value)))
        }
        ManageMsg::Gifs(on) => patch(state, api, &key, RoomUpdate { allow_gifs_stickers: Some(on), ..Default::default() }, |r| set_setting(r, "allow_gifs_stickers", json!(on))),
        ManageMsg::NewRole(v) => {
            form.new_role = v;
            Task::none()
        }
        ManageMsg::RoleColor(i) => {
            form.new_role_color = i.min(ROLE_COLORS.len() - 1);
            Task::none()
        }
        ManageMsg::AddRole => {
            let name = form.new_role.trim().to_string();
            if name.is_empty() {
                return (Task::none(), None);
            }
            form.new_role.clear();
            let role = RoomRole { id: format!("role_{}", chrono::Utc::now().timestamp_millis()), name, color: format!("#{:06x}", ROLE_COLORS[form.new_role_color]) };
            let mut roles = current_roles(&room);
            roles.push(role);
            set_roles(state, api, &key, roles)
        }
        ManageMsg::DeleteRole(id) => {
            let roles = current_roles(&room).into_iter().filter(|r| r.id != id).collect();
            set_roles(state, api, &key, roles)
        }
        ManageMsg::Cancel => {
            form.confirm = None;
            Task::none()
        }
        ManageMsg::Confirm => match form.confirm.take() {
            Some(Confirm::Kick(id, name)) => {
                Task::perform(chat::kick_user(api.clone(), key, id), move |r| done(AdminResult::Kicked(name.clone(), r.map(|_| ()))))
            }
            Some(Confirm::MakeChannel) => everyone(state, api, &key, vec![], vec![Perm::SendMessages], "This room is now a channel"),
            Some(Confirm::RevokeInvite) => {
                let k = key.clone();
                Task::perform(chat::revoke_invite(api.clone(), key), move |r| done(AdminResult::Revoked(k.clone(), r.map(|_| ()))))
            }
            None => Task::none(),
        },
    };
    (task, None)
}

fn current_roles(room: &ChatRoomItem) -> Vec<RoomRole> {
    room.settings.as_ref().and_then(|s| s.get("roles")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
}

fn set_roles(state: &mut State, api: &Api, key: &str, roles: Vec<RoomRole>) -> Task<Message> {
    let value = serde_json::to_value(&roles).unwrap_or_default();
    patch(state, api, key, RoomUpdate { roles: Some(roles), ..Default::default() }, move |r| set_setting(r, "roles", value))
}

fn everyone(state: &mut State, api: &Api, key: &str, add: Vec<Perm>, remove: Vec<Perm>, ok: &'static str) -> Task<Message> {
    if let Some(form) = state.manage.as_mut() {
        form.everyone_busy = true;
    }
    let ids = |v: Vec<Perm>| v.into_iter().map(|p| p.id().to_string()).collect();
    let k = key.to_string();
    Task::perform(chat::change_everyone_permissions(api.clone(), k.clone(), ids(add), ids(remove)), move |r| done(AdminResult::Everyone(k.clone(), r.map(|_| ok).map_err(|e| e.message))))
}

/// Makes the link; with the room key, also a #k secret (stored encrypted under the room key) so
/// whoever joins with it can prove they had the link — the server never sees the fragment.
async fn create_invite(api: Api, room_key: String, rules: InviteRules, current_key: Option<String>) -> Result<String, String> {
    let made = chat::create_invite(api.clone(), room_key.clone(), rules).await.map_err(|e| e.message)?;
    let code = made.invite_code.ok_or("The server didn't return an invite code")?;
    let mut link = format!("{}/chat?invite={code}", normalize_base(api.base()));
    if let Some(key) = current_key {
        let secret = rooms::generate_invite_secret();
        if let Some(sealed) = encrypt_symmetric(&secret, &key) {
            let patch = MemberAuthPatch { add_invite_secret: Some(sealed), ..Default::default() };
            match chat::update_member_auth(api, room_key, patch).await {
                Ok(_) => link.push_str(&format!("#k={secret}")),
                Err(e) => tracing::warn!("e2ee: couldn't attach the invite secret; joiners will need approval: {}", e.message),
            }
        }
    }
    Ok(link)
}

/// Owner tags for these members' current keys (authorizeGroupMembers), so every member's app may
/// hand them the room key; `then_share`: share it right after.
fn authorize(state: &mut State, api: &Api, key: &str, ids: Vec<i64>, then_share: bool) -> Task<Message> {
    let Some(room) = state.rooms.get(key).cloned() else { return Task::none() };
    if room.chat_type != "group" {
        return if then_share { heal(state, api, key) } else { Task::none() };
    }
    let Some(current) = room_keys(state, key).and_then(|k| k.into_iter().next()) else {
        state.notices.push("Approving needs this chat's key on this computer.".into());
        return Task::none();
    };
    let mut entries = HashMap::new();
    for m in room.allowed_users.iter().filter(|m| ids.contains(&m.id)) {
        let Some(public) = m.e2ee_public_key.as_deref().filter(|p| !p.is_empty()) else { continue };
        if !state.keys.trusts(m.id, public) {
            continue;
        }
        if let Some(tag) = rooms::owner_tag(&current, &room.room_key, m.id, public) {
            entries.insert(m.id.to_string(), MemberAuthEntry { tag, kind: "owner".into() });
        }
    }
    if entries.is_empty() {
        return if then_share { heal(state, api, key) } else { Task::none() };
    }
    let k = key.to_string();
    let patch = MemberAuthPatch { entries: Some(entries), ..Default::default() };
    Task::perform(chat::update_member_auth(api.clone(), k.clone(), patch), move |r| done(AdminResult::Authorized(k.clone(), r, then_share)))
}

/// Hands the room key to members who don't have it yet and whose admission is provable; the rest
/// show as awaiting approval. A room nobody holds a key for gets its first one (bootstrap).
pub fn heal(state: &mut State, api: &Api, key: &str) -> Task<Message> {
    if state.healing.contains(key) {
        return Task::none();
    }
    let Some(room) = state.rooms.get(key).cloned() else { return Task::none() };
    if room.chat_type == "public" || room.is_teaser == Some(true) || room.is_member == Some(false) {
        return Task::none();
    }
    if room.key_envelopes.as_ref().is_none_or(|e| e.is_empty()) {
        return bootstrap(state, api, &room);
    }
    let Some(keys) = room_keys(state, key) else { return Task::none() };
    let (admitted, pending) = rooms::missing_members(&room, state.me_id, &keys);
    state.pending_admissions.insert(key.to_string(), pending);
    let members: Vec<(i64, String)> = admitted.iter().filter_map(|u| u.e2ee_public_key.clone().map(|k| (u.id, k))).collect();
    if members.is_empty() {
        return Task::none();
    }
    let envelopes = state.keys.wrap_for(&keys[0], &members);
    if envelopes.is_empty() {
        return Task::none();
    }
    tracing::info!("e2ee: sharing the key of {key} with {} member(s)", envelopes.len());
    state.healing.insert(key.to_string());
    let k = key.to_string();
    Task::perform(chat::add_key_envelopes(api.clone(), k.clone(), envelopes), move |r| done(AdminResult::Shared(k.clone(), r)))
}

/// The first key of a private room nobody holds one for (bootstrapRoomKey) — by a member who may
/// manage keys. The epoch guard (409) stops two members starting different keys.
fn bootstrap(state: &mut State, api: &Api, room: &ChatRoomItem) -> Task<Message> {
    if !perms::effective(room, state.me_id, state.is_admin).contains(&Perm::ManageKeys) || !state.keys.has_device_key() {
        return Task::none();
    }
    let new_key = generate_room_key();
    let members: Vec<(i64, String)> = room
        .allowed_users
        .iter()
        .filter(|m| room.allowed_user_ids.contains(&m.id))
        .filter_map(|m| m.e2ee_public_key.clone().filter(|k| !k.is_empty()).map(|k| (m.id, k)))
        .collect();
    let envelopes = state.keys.wrap_for(&new_key, &members);
    if !envelopes.contains_key(&state.me_id.to_string()) {
        return Task::none(); // this computer's key isn't the account's: the chat says why
    }
    let Some(archived_keys) = seal_archived_keys(&[], &new_key) else { return Task::none() };
    let vouched: Vec<(i64, String)> = members.into_iter().filter(|(id, _)| envelopes.contains_key(&id.to_string())).collect();
    let rotation = KeyRotation { envelopes, archived_keys, expected_epoch: room.e2ee_meta.as_ref().and_then(|m| m.key_epoch).unwrap_or(0), invite_secrets: None };
    tracing::info!("e2ee: making the first key of {}", room.room_key);
    state.healing.insert(room.room_key.clone());
    let k = room.room_key.clone();
    let group = room.chat_type == "group";
    Task::perform(first_key(api.clone(), k.clone(), rotation, group, new_key, vouched), move |r| done(AdminResult::Bootstrapped(k.clone(), r)))
}

async fn first_key(api: Api, room_key: String, rotation: KeyRotation, group: bool, new_key: String, vouched: Vec<(i64, String)>) -> Result<EnvelopesUpdate, ApiError> {
    let res = chat::rotate_room_key(api.clone(), room_key.clone(), rotation).await?;
    if group {
        let entries: HashMap<String, MemberAuthEntry> = vouched
            .iter()
            .filter_map(|(id, public)| rooms::owner_tag(&new_key, &room_key, *id, public).map(|tag| (id.to_string(), MemberAuthEntry { tag, kind: "owner".into() })))
            .collect();
        if !entries.is_empty() {
            if let Err(e) = chat::update_member_auth(api, room_key, MemberAuthPatch { entries: Some(entries), ..Default::default() }).await {
                tracing::warn!("e2ee: couldn't issue admission tags with the first key: {}", e.message);
            }
        }
    }
    Ok(res)
}

pub fn on_result(state: &mut State, result: AdminResult, api: &Api) -> (Task<Message>, Option<String>) {
    let mut error = None;
    let task = match result {
        AdminResult::Saved(key, Ok(room)) => {
            state.rooms.update_room(&key, |r| {
                r.title = room.title.clone();
                r.topic = room.topic.clone();
            });
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.saving = false;
            }
            state.notices.push(format!("Chat room renamed to \"{}\"", room.title));
            Task::none()
        }
        AdminResult::Avatar(key, set, r) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.avatar_saving = false;
            }
            match r {
                Ok(room) => {
                    state.rooms.update_room(&key, |r| r.avatar = room.avatar.clone());
                    state.notices.push(if set { "Room avatar updated" } else { "Room avatar removed" }.into());
                }
                Err(e) => error = Some(format!("Couldn't change the room avatar: {}", e.message)),
            }
            Task::none()
        }
        AdminResult::Saved(key, Err(e)) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.saving = false;
            }
            error = Some(format!("Couldn't rename the room: {}", e.message));
            Task::none()
        }
        AdminResult::Updated(key, Ok(room)) => {
            if let Some(settings) = room.settings.clone() {
                state.rooms.update_room(&key, |r| r.settings = Some(settings));
            }
            Task::none()
        }
        AdminResult::Updated(_, Err(e)) => {
            error = Some(format!("Couldn't update the room: {}", e.message));
            super::chat_page::load(state, api) // undo what was shown
        }
        AdminResult::Added(key, before, Ok(room)) => {
            let added: Vec<i64> = room.allowed_user_ids.iter().copied().filter(|id| !before.contains(id)).collect();
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.adding = false;
                m.add_rows = vec![String::new()];
            }
            state.notices.push(if added.len() == 1 { "Added 1 person".into() } else { format!("Added {} people", added.len()) });
            merge_room(state, *room);
            Task::batch([authorize(state, api, &key, added, true), super::chat_page::load(state, api)])
        }
        AdminResult::Added(key, _, Err(e)) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.adding = false;
            }
            error = Some(format!("Couldn't add: {}", e.message));
            Task::none()
        }
        AdminResult::Kicked(name, Ok(())) => {
            state.notices.push(format!("@{name} has been removed from the room"));
            super::chat_page::load(state, api)
        }
        AdminResult::Kicked(_, Err(e)) => {
            error = Some(format!("Couldn't remove: {}", e.message));
            Task::none()
        }
        AdminResult::Invite(key, result) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.invite = result.ok();
            }
            Task::none()
        }
        AdminResult::InviteCreated(key, result) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.generating = false;
                match result {
                    Ok(link) => m.invite_link = Some(link),
                    Err(e) => error = Some(format!("Couldn't create the invite link: {e}")),
                }
            }
            load_invite(api, &key)
        }
        AdminResult::Revoked(key, result) => {
            match result {
                Ok(()) => {
                    state.notices.push("Invite link turned off".into());
                    if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                        m.invite_link = None;
                    }
                }
                Err(e) => error = Some(e.message),
            }
            load_invite(api, &key)
        }
        AdminResult::Everyone(key, result) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.everyone_busy = false;
                if result.is_ok() {
                    m.everyone.clear();
                }
            }
            match result {
                Ok(text) => {
                    state.notices.push(text.into());
                    super::chat_page::load(state, api)
                }
                Err(e) => {
                    error = Some(e);
                    Task::none()
                }
            }
        }
        AdminResult::Requests(key, result) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                match result {
                    Ok(r) => m.requests = Some(r.requests),
                    Err(e) => {
                        m.requests = Some(vec![]);
                        error = Some(e.message);
                    }
                }
            }
            Task::none()
        }
        AdminResult::Decided(key, id, approve, result) => {
            if let Some(m) = state.manage.as_mut().filter(|m| m.room_key == key) {
                m.deciding = None;
                if result.is_ok() {
                    if let Some(list) = m.requests.as_mut() {
                        list.retain(|r| r.user.id != id);
                    }
                }
            }
            match result {
                Ok(decision) => {
                    state.rooms.update_room(&key, |r| r.join_requests_count = Some(decision.pending));
                    match decision.room.filter(|_| approve) {
                        Some(room) => {
                            state.notices.push("Approved — sharing the room key with them".into());
                            merge_room(state, room);
                            authorize(state, api, &key, vec![id], true)
                        }
                        None => {
                            state.notices.push(if approve { "Approved — they're in".into() } else { "Request declined".into() });
                            if approve { super::chat_page::load(state, api) } else { Task::none() }
                        }
                    }
                }
                Err(e) => {
                    error = Some(e.message);
                    Task::none()
                }
            }
        }
        AdminResult::Authorized(key, result, then_share) => match result {
            Ok(update) => {
                state.rooms.update_room(&key, |r| {
                    if update.member_auth.is_some() {
                        r.member_auth = update.member_auth.clone();
                    }
                    if update.e2ee_meta.is_some() {
                        r.e2ee_meta = update.e2ee_meta.clone();
                    }
                });
                if then_share { heal(state, api, &key) } else { Task::none() }
            }
            Err(e) => {
                error = Some(format!("Couldn't approve: {}", e.message));
                Task::none()
            }
        },
        AdminResult::Shared(key, result) => {
            state.healing.remove(&key);
            match result {
                Ok(update) => {
                    let shared = update.key_envelopes;
                    state.rooms.update_room(&key, |r| r.key_envelopes = Some(shared));
                }
                Err(e) => tracing::warn!("e2ee: couldn't share the room key: {}", e.message),
            }
            Task::none()
        }
        AdminResult::Bootstrapped(key, result) => {
            state.healing.remove(&key);
            match result {
                Ok(update) => {
                    state.rooms.update_room(&key, |r| {
                        r.key_envelopes = Some(update.key_envelopes.clone());
                        if update.e2ee_meta.is_some() {
                            r.e2ee_meta = update.e2ee_meta.clone();
                        }
                    });
                    state.locked.remove(&key);
                    super::chat_page::reload_messages(state, api, &key)
                }
                Err(e) if e.status == Some(409) => super::chat_page::load(state, api), // another member got there first
                Err(e) => {
                    tracing::warn!("e2ee: couldn't make the first room key: {}", e.message);
                    Task::none()
                }
            }
        }
    };
    (task, error)
}

/// A room as the server answered (add members, approve): its member data, keeping ours where it
/// sent less.
fn merge_room(state: &mut State, room: ChatRoomItem) {
    let key = room.room_key.clone();
    state.rooms.update_room(&key, |r| {
        r.allowed_user_ids = room.allowed_user_ids.clone();
        if !room.allowed_users.is_empty() {
            r.allowed_users = room.allowed_users.clone();
        }
        if room.key_envelopes.is_some() {
            r.key_envelopes = room.key_envelopes.clone();
        }
        if room.member_auth.is_some() {
            r.member_auth = room.member_auth.clone();
        }
        if room.e2ee_meta.is_some() {
            r.e2ee_meta = room.e2ee_meta.clone();
        }
        if room.settings.is_some() {
            r.settings = room.settings.clone();
        }
    });
}
