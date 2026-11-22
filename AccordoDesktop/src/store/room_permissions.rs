// SPDX-License-Identifier: GPL-3.0-or-later
//! Who may do what in a room (utils/roomPermissions.ts, mirroring backend/app/room_permissions.py).
//! The server enforces these; the client only hides what you can't do. Roles are labels only.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::api::chat::ChatRoomItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Perm {
    SendMessages,
    ManageMembers,
    ManageKeys,
    ManageRoom,
    PinMessages,
    DeleteMessages,
    PostFinance,
}

pub struct PermInfo {
    pub perm: Perm,
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
}

pub const PERMISSIONS: [PermInfo; 7] = [
    PermInfo { perm: Perm::SendMessages, id: "send_messages", label: "Send messages", hint: "Post in the chat. Without it the room is a channel for them — they read and reply in threads" },
    PermInfo { perm: Perm::ManageMembers, id: "manage_members", label: "Manage members", hint: "Add, remove and invite people, assign roles" },
    PermInfo { perm: Perm::ManageKeys, id: "manage_keys", label: "Manage encryption", hint: "Rotate the room key, e.g. after someone leaves" },
    PermInfo { perm: Perm::ManageRoom, id: "manage_room", label: "Manage room", hint: "Name, avatar, slow mode, disappearing messages, role list" },
    PermInfo { perm: Perm::PinMessages, id: "pin_messages", label: "Pin messages", hint: "" },
    PermInfo { perm: Perm::DeleteMessages, id: "delete_messages", label: "Delete others' messages", hint: "" },
    PermInfo { perm: Perm::PostFinance, id: "post_finance", label: "Post payments", hint: "Cheques, invoices, campaigns and drops" },
];

pub type Perms = BTreeSet<Perm>;

impl Perm {
    pub fn id(self) -> &'static str {
        info(self).id
    }

    pub fn label(self) -> &'static str {
        info(self).label
    }

    pub fn from_id(id: &str) -> Option<Perm> {
        PERMISSIONS.iter().find(|p| p.id == id).map(|p| p.perm)
    }
}

pub fn info(p: Perm) -> &'static PermInfo {
    PERMISSIONS.iter().find(|i| i.perm == p).expect("every permission is listed")
}

pub fn all() -> Perms {
    PERMISSIONS.iter().map(|p| p.perm).collect()
}

pub const DEFAULT_MEMBER: [Perm; 3] = [Perm::SendMessages, Perm::PinMessages, Perm::PostFinance];
/// Lists saved before send_messages existed imply it.
const PERMISSIONS_VERSION: i64 = 2;

fn setting<'a>(room: &'a ChatRoomItem, key: &str) -> Option<&'a Value> {
    room.settings.as_ref().and_then(|s| s.get(key))
}

fn clean(v: &Value) -> Option<Perms> {
    v.as_array().map(|list| list.iter().filter_map(|p| p.as_str().and_then(Perm::from_id)).collect())
}

fn as_saved(room: &ChatRoomItem, mut perms: Perms) -> Perms {
    let version = setting(room, "permissions_version").and_then(Value::as_i64).unwrap_or(1);
    if version < PERMISSIONS_VERSION {
        perms.insert(Perm::SendMessages);
    }
    perms
}

pub fn owner_id(room: &ChatRoomItem) -> Option<i64> {
    room.created_by.or_else(|| room.allowed_user_ids.first().copied())
}

pub fn default_permissions(room: &ChatRoomItem) -> Perms {
    match setting(room, "default_permissions").and_then(clean) {
        Some(p) => as_saved(room, p),
        None => DEFAULT_MEMBER.into_iter().collect(),
    }
}

/// The member's own list, or None when they follow the room default.
pub fn custom_permissions(room: &ChatRoomItem, user_id: i64) -> Option<Perms> {
    setting(room, "member_permissions").and_then(|m| m.get(user_id.to_string())).and_then(clean).map(|p| as_saved(room, p))
}

pub fn effective(room: &ChatRoomItem, user_id: i64, is_admin: bool) -> Perms {
    if is_admin && room.chat_type != "dm" {
        return all();
    }
    if room.chat_type == "public" {
        if room.created_by == Some(user_id) {
            return all();
        }
        return custom_permissions(room, user_id).unwrap_or_else(|| default_permissions(room));
    }
    if !room.allowed_user_ids.contains(&user_id) {
        return Perms::new();
    }
    if room.chat_type == "dm" {
        return all().into_iter().filter(|p| !matches!(p, Perm::ManageMembers | Perm::DeleteMessages)).collect();
    }
    if Some(user_id) == owner_id(room) {
        return all();
    }
    custom_permissions(room, user_id).unwrap_or_else(|| default_permissions(room))
}

/// Only the room's owner or a site admin edits who may do what (groups and public rooms).
pub fn can_edit_permissions(room: &ChatRoomItem, user_id: i64, is_admin: bool) -> bool {
    matches!(room.chat_type.as_str(), "group" | "public") && (Some(user_id) == owner_id(room) || is_admin)
}

/// Owner-level control of the room (the backend's _is_room_owner): a site admin in public rooms,
/// either participant of a DM, the creator of a group.
pub fn is_room_owner(room: &ChatRoomItem, user_id: i64, is_admin: bool) -> bool {
    match room.chat_type.as_str() {
        "public" => is_admin,
        "dm" => room.allowed_user_ids.contains(&user_id),
        _ => room.created_by == Some(user_id),
    }
}

/// The owner removes anyone but themselves; a manage_members holder removes plain members only.
pub fn can_remove(room: &ChatRoomItem, me: i64, is_admin: bool, target: i64) -> bool {
    if room.chat_type != "group" || target == me || Some(target) == owner_id(room) || !room.allowed_user_ids.contains(&target) {
        return false;
    }
    if Some(me) == owner_id(room) || is_admin {
        return true;
    }
    effective(room, me, is_admin).contains(&Perm::ManageMembers) && !effective(room, target, false).contains(&Perm::ManageMembers)
}

pub fn ids(perms: &Perms) -> Vec<String> {
    perms.iter().map(|p| p.id().to_string()).collect()
}
