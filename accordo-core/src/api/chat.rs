// SPDX-License-Identifier: GPL-3.0-or-later
//! Chat (src/api/chat.ts): rooms, messages, members, invites, end-to-end key envelopes, unread
//! counters — and polls, threads, scheduled messages, reports and the public-room catalog in the
//! submodules below.
//!
//! Most chat endpoints may answer inside `{data: …}`; they're read the way the web reads them
//! (`res.data || res`). Message bodies in private rooms are encrypted: `content` and
//! `attachment_data` arrive as the server stores them (ciphertext), to be opened by the E2EE layer.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::auth::PublicWallet;
use super::client::{enc, query, Api, ApiResult};

/// One wrapped copy of a room key, for one member.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyEnvelope {
    pub envelope: String,
    pub wrapped_by: i64,
    /// The wrapper's public key at wrap time: unwrapping uses this rather than their current
    /// profile key, so envelopes survive the wrapper changing keys later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_public_key: Option<String>,
}

pub type KeyEnvelopes = HashMap<String, KeyEnvelope>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MemberAuthEntry {
    pub tag: String,
    /// "owner" | "invite"
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct E2eeMeta {
    pub invite_secrets: Option<Vec<String>>,
    /// Every previous room key, encrypted under the current one (see rotate_room_key).
    pub archived_keys: Option<String>,
    pub key_epoch: Option<i64>,
    /// Set by the server when a member left / was removed; the next member to open the room rotates.
    pub rotation_needed: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ThreadSummary {
    pub count: i64,
    pub last_reply_at: Option<String>,
    pub participant_ids: Vec<i64>,
}

/// text | cheque | invoice | transfer | campaign | drop | gif | sticker | image | file | audio |
/// video | nft_mint | poll | random_draw | article | tip | deleted — kept as text so a new kind
/// from the server still reads.
pub type ChatMessageType = String;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ChatMessageItem {
    pub id: i64,
    pub room_id: String,
    pub sender_id: Option<i64>,
    pub sender_username: String,
    pub sender_display_name: Option<String>,
    pub sender_avatar: Option<String>,
    pub recipient_id: Option<i64>,
    pub recipient_username: Option<String>,
    pub content: String,
    pub message_type: ChatMessageType,
    pub attachment_data: Option<Value>,
    /// Set on thread replies (they never appear in the main feed).
    pub thread_root_id: Option<i64>,
    /// Set on main-feed messages that have a thread.
    pub thread_summary: Option<ThreadSummary>,
    /// Disappearing messages: when the server deletes it (UTC ISO).
    pub expires_at: Option<String>,
    /// emoji → ids of the users who reacted.
    pub reactions: Option<HashMap<String, Vec<i64>>>,
    pub reply_to_id: Option<i64>,
    pub reply_to_data: Option<Value>,
    pub is_pinned: Option<bool>,
    pub is_edited: Option<bool>,
    pub edited_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
    pub discord_avatar: Option<String>,
    pub discord_id: Option<String>,
    pub discord_user_id: Option<String>,
    pub discord_username: Option<String>,
    pub role: Option<String>,
    pub verified: Option<bool>,
    /// Organizations they're in (profile only), verified first.
    pub orgs: Vec<crate::api::orgs::OrgBrief>,
    pub wallets: Option<Vec<PublicWallet>>,
    pub e2ee_public_key: Option<String>,
    pub is_online: Option<bool>,
    pub last_seen_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SharedRoomSummary {
    pub room_key: String,
    pub title: Option<String>,
    pub chat_type: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RecentTransactionSummary {
    /// "cheque" | "invoice"
    #[serde(rename = "type")]
    pub kind: String,
    pub amount: Option<f64>,
    pub token: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserRelationshipStats {
    pub shared_room_count: i64,
    pub shared_rooms: Vec<SharedRoomSummary>,
    pub cheques_exchanged: i64,
    pub invoices_exchanged: i64,
    pub most_recent_transaction: Option<RecentTransactionSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomRole {
    pub id: String,
    pub name: String,
    pub color: String,
}

/// Someone waiting to get into a group, as its managers see them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct JoinRequestItem {
    pub user: UserInfo,
    pub at: Option<String>,
    /// "invite" | "view"
    pub via: String,
    /// Their note for the admins — plaintext to the server (they have no room key yet).
    pub message: Option<String>,
    pub account_created_at: Option<String>,
    pub shared_groups: i64,
}

/// What a group you're not in shows you through its view rules.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RoomTeaser {
    pub members: i64,
    pub online: i64,
    pub last_activity: Option<String>,
    pub requested: bool,
    /// A real waiting request (not a quiet "no") — the asker may take it back.
    pub can_withdraw: bool,
    /// "frozen" | "full"
    pub closed: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ChatRoomItem {
    pub id: i64,
    pub room_key: String,
    /// "public" | "dm" | "group" | "private"
    pub chat_type: String,
    pub title: String,
    pub custom_title: Option<String>,
    pub topic: Option<String>,
    pub avatar: Option<String>,
    pub settings: Option<HashMap<String, Value>>,
    /// Visibility rules — reserved, set by the server only.
    pub view: Option<HashMap<String, Value>>,
    /// False for a public room you can read but haven't joined: Join to write.
    pub is_member: Option<bool>,
    /// Groups: people waiting to get in (only counted for those who may decide).
    pub join_requests_count: Option<i64>,
    /// A group shown through its view rules that you're not in: no messages, no members.
    pub is_teaser: Option<bool>,
    pub teaser: Option<RoomTeaser>,
    pub key_envelopes: Option<KeyEnvelopes>,
    pub member_auth: Option<HashMap<String, MemberAuthEntry>>,
    pub e2ee_meta: Option<E2eeMeta>,
    pub is_pending_request: Option<bool>,
    pub allowed_user_ids: Vec<i64>,
    pub allowed_users: Vec<UserInfo>,
    /// Real member count; big public rooms carry only a few members in allowed_users.
    pub member_count: Option<i64>,
    pub members_truncated: Option<bool>,
    pub last_message: Option<String>,
    pub last_timestamp: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: Option<String>,
    /// joinByInvite on an approval-only link: "pending" until a manager approves.
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RoomUnreadInfo {
    pub unread_count: i64,
    pub mention_count: i64,
    pub last_read_id: i64,
}

/// The user's own sidebar arrangement for a room.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RoomPrefs {
    pub pinned_at: Option<String>,
    pub archived: bool,
    pub muted: Option<bool>,
    /// setRoomPrefs answers with the room too.
    pub room_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UnreadsResponse {
    pub unreads: HashMap<String, RoomUnreadInfo>,
    /// room → user id → last message id they read.
    pub partner_reads: HashMap<String, HashMap<String, i64>>,
    pub prefs: Option<HashMap<String, RoomPrefs>>,
}

/// Room disappearing-message timer: off | 30s | 5m | 1h | 24h | 7d.
pub type AutoBurn = String;

#[derive(Debug, Clone, Default, Serialize)]
pub struct InviteRules {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_hours: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InviteStatus {
    pub room_key: Option<String>,
    /// createInvite only.
    pub invite_code: Option<String>,
    pub expires_at: Option<String>,
    pub max_uses: Option<i64>,
    pub uses: i64,
    pub requires_approval: bool,
    /// getInvite only.
    pub active: Option<bool>,
    pub join_requests: Option<Vec<JoinRequestBrief>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct JoinRequestBrief {
    pub user: UserInfo,
    pub at: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateRoom {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, Value>>,
    // DM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    // group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_usernames: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_user_ids: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_envelopes: Option<KeyEnvelopes>,
}

/// updateRoom: send only what changes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RoomUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_mode_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_gifs_stickers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<RoomRole>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_roles: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_burn: Option<AutoBurn>,
    /// Owner only: a member's own permission list, or None to follow the room default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_permissions: Option<HashMap<String, Option<Vec<String>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_permissions: Option<Vec<String>>,
    /// 2 = the lists name send_messages explicitly (older clients' lists imply it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions_version: Option<i64>,
}

/// getMessages filters.
#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    pub room_id: Option<String>,
    pub recipient_id: Option<i64>,
    pub recipient_username: Option<String>,
    pub limit: Option<i64>,
    pub before_id: Option<i64>,
    pub include_thread_replies: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PollSettings {
    /// "poll" | "quiz"
    pub kind: String,
    pub anonymous: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrawSettings {
    pub count: i64,
    pub include_me: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<ChatMessageType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<PollSettings>,
    /// random_draw: the server picks `count` room members and fills attachment_data with the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw: Option<DrawSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_root_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Presence {
    pub online_user_ids: Vec<i64>,
    pub presence: HashMap<String, PresenceItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PresenceItem {
    pub user_id: i64,
    pub username: String,
    pub is_online: bool,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MembersPage {
    pub total: i64,
    pub offset: i64,
    pub members: Vec<UserInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct JoinRequests {
    pub room_key: String,
    pub requests: Vec<JoinRequestItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct JoinDecision {
    pub status: String,
    pub pending: i64,
    pub room: Option<ChatRoomItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EveryonePermissions {
    pub default_permissions: Vec<String>,
    pub members_updated: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MemberAuthUpdate {
    pub room_key: String,
    pub member_auth: Option<HashMap<String, MemberAuthEntry>>,
    pub e2ee_meta: Option<E2eeMeta>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemberAuthPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<HashMap<String, MemberAuthEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_invite_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_invite_secrets: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EnvelopesUpdate {
    pub room_key: String,
    pub key_envelopes: KeyEnvelopes,
    pub e2ee_meta: Option<E2eeMeta>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct KeyRotation {
    pub envelopes: KeyEnvelopes,
    pub archived_keys: String,
    pub expected_epoch: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_secrets: Option<Vec<String>>,
}

/// Small answers: `{status, room_key, room_id, …}`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StatusReply {
    pub status: String,
    pub room_key: Option<String>,
    pub room_id: Option<String>,
    pub message_id: Option<i64>,
    pub rooms_invalidated: Option<i64>,
    pub last_read_message_id: Option<i64>,
    pub is_pinned: Option<bool>,
    pub reactions: Option<HashMap<String, Vec<i64>>>,
    pub view: Option<HashMap<String, Value>>,
    pub id: Option<i64>,
}

fn room(room_key: &str) -> String {
    format!("/api/chat/rooms/{room_key}")
}

// ── rooms ──

pub async fn get_rooms(api: Api) -> ApiResult<Vec<ChatRoomItem>> {
    api.get_list("/api/chat/rooms").await
}

/// Same list as get_rooms (the web keeps both names).
pub async fn get_conversations(api: Api) -> ApiResult<Vec<ChatRoomItem>> {
    api.get_data("/api/chat/rooms").await
}

pub async fn create_public_room(api: Api, room: CreateRoom) -> ApiResult<ChatRoomItem> {
    api.post_data("/api/chat/rooms/public", Some(&room)).await
}

pub async fn create_dm_room(api: Api, room: CreateRoom) -> ApiResult<ChatRoomItem> {
    api.post_data("/api/chat/rooms/dm", Some(&room)).await
}

pub async fn create_group_room(api: Api, room: CreateRoom) -> ApiResult<ChatRoomItem> {
    api.post_data("/api/chat/rooms/group", Some(&room)).await
}

pub async fn add_allowed_users(api: Api, room_key: String, usernames: Option<Vec<String>>, user_ids: Option<Vec<i64>>) -> ApiResult<ChatRoomItem> {
    let mut body = json!({});
    if let Some(u) = usernames {
        body["usernames"] = json!(u);
    }
    if let Some(ids) = user_ids {
        body["user_ids"] = json!(ids);
    }
    api.post_data(&format!("{}/allowed-users", room(&room_key)), Some(&body)).await
}

pub async fn update_room(api: Api, room_key: String, update: RoomUpdate) -> ApiResult<ChatRoomItem> {
    api.patch_data(&room(&room_key), &update).await
}

pub async fn kick_user(api: Api, room_key: String, user_id: i64) -> ApiResult<ChatRoomItem> {
    api.delete_data(&format!("{}/members/{user_id}", room(&room_key))).await
}

/// One page of a room's members (online first); for big public rooms.
pub async fn list_room_members(api: Api, room_key: String, search: Option<String>, offset: i64, limit: i64) -> ApiResult<MembersPage> {
    let q = query(&[
        ("q", search.filter(|s| !s.is_empty())),
        ("offset", Some(offset.to_string())),
        ("limit", Some((if limit > 0 { limit } else { 50 }).to_string())),
    ]);
    api.get(&format!("/api/chat/rooms/{}/members{q}", enc(&room_key))).await
}

// ── invites, join requests ──

pub async fn create_invite(api: Api, room_key: String, rules: InviteRules) -> ApiResult<InviteStatus> {
    api.post_data(&format!("{}/invite", room(&room_key)), Some(&rules)).await
}

/// The current link's rules / usage and people waiting for approval (manage_members only).
pub async fn get_invite(api: Api, room_key: String) -> ApiResult<InviteStatus> {
    api.get_data(&format!("{}/invite", room(&room_key))).await
}

pub async fn revoke_invite(api: Api, room_key: String) -> ApiResult<Value> {
    api.delete(&format!("{}/invite", room(&room_key))).await
}

/// Give / take permissions from everyone at once (room default + every member's own list).
pub async fn change_everyone_permissions(api: Api, room_key: String, add: Vec<String>, remove: Vec<String>) -> ApiResult<EveryonePermissions> {
    api.post_data(&format!("{}/permissions/everyone", room(&room_key)), Some(&json!({ "add": add, "remove": remove }))).await
}

/// "pending" or "member".
pub async fn request_to_join_shown(api: Api, room_key: String, message: Option<String>) -> ApiResult<StatusReply> {
    api.post_data(&format!("{}/request-join", room(&room_key)), Some(&json!({ "message": message.filter(|m| !m.is_empty()) }))).await
}

/// Adds the note to a request sent without one (approval-only invite links). Set once.
pub async fn add_join_request_note(api: Api, room_key: String, message: String) -> ApiResult<Value> {
    api.put(&format!("{}/join-request", room(&room_key)), &json!({ "message": message })).await
}

pub async fn withdraw_join_request(api: Api, room_key: String) -> ApiResult<Value> {
    api.delete(&format!("{}/join-request", room(&room_key))).await
}

pub async fn get_join_requests(api: Api, room_key: String) -> ApiResult<JoinRequests> {
    api.get_data(&format!("{}/join-requests", room(&room_key))).await
}

pub async fn decide_join_request(api: Api, room_key: String, user_id: i64, approve: bool) -> ApiResult<JoinDecision> {
    api.post_data(&format!("{}/join-requests/{user_id}", room(&room_key)), Some(&json!({ "approve": approve }))).await
}

/// Owner / site admin: who sees the room without being in it ({everyone: true} → Suggested).
pub async fn set_room_view(api: Api, room_key: String, view: Option<HashMap<String, Value>>) -> ApiResult<StatusReply> {
    api.put_data(&format!("{}/view", room(&room_key)), &json!({ "view": view })).await
}

/// Joins — or, for an approval-only link, answers `status: "pending"` until a manager approves.
pub async fn join_by_invite(api: Api, invite_code: String, auth_tag: Option<String>) -> ApiResult<ChatRoomItem> {
    let path = format!("/api/chat/rooms/join/{invite_code}");
    match auth_tag {
        Some(tag) => api.post_data(&path, Some(&json!({ "auth_tag": tag }))).await,
        None => api.post_data::<(), _>(&path, None).await,
    }
}

pub async fn accept_dm_request(api: Api, room_key: String) -> ApiResult<StatusReply> {
    api.post_data::<(), _>(&format!("{}/accept", room(&room_key)), None).await
}

pub async fn decline_dm_request(api: Api, room_key: String) -> ApiResult<StatusReply> {
    api.post_data::<(), _>(&format!("{}/decline", room(&room_key)), None).await
}

// ── end-to-end keys ──

pub async fn update_member_auth(api: Api, room_key: String, patch: MemberAuthPatch) -> ApiResult<MemberAuthUpdate> {
    api.post_data(&format!("{}/member-auth", room(&room_key)), Some(&patch)).await
}

pub async fn add_key_envelopes(api: Api, room_key: String, envelopes: KeyEnvelopes) -> ApiResult<EnvelopesUpdate> {
    api.post_data(&format!("{}/key-envelopes", room(&room_key)), Some(&json!({ "envelopes": envelopes }))).await
}

/// Rotates the room key: new envelopes + the archive of previous keys, atomically. The server
/// refuses it (409) if someone else rotated since `expected_epoch`.
pub async fn rotate_room_key(api: Api, room_key: String, rotation: KeyRotation) -> ApiResult<EnvelopesUpdate> {
    api.put_data(&format!("{}/key-envelopes", room(&room_key)), &rotation).await
}

pub async fn invalidate_my_key_envelopes(api: Api) -> ApiResult<StatusReply> {
    api.post_data::<(), _>("/api/chat/e2ee/invalidate-my-envelopes", None).await
}

// ── messages ──

pub async fn get_messages(api: Api, q: MessageQuery) -> ApiResult<Vec<ChatMessageItem>> {
    let qs = query(&[
        ("room_id", q.room_id),
        ("recipient_id", q.recipient_id.filter(|v| *v != 0).map(|v| v.to_string())),
        ("recipient_username", q.recipient_username),
        ("limit", q.limit.filter(|v| *v != 0).map(|v| v.to_string())),
        ("before_id", q.before_id.filter(|v| *v != 0).map(|v| v.to_string())),
        ("include_thread_replies", q.include_thread_replies.then(|| "true".into())),
    ]);
    // Always "?…" even when empty, as the web sends it.
    let qs = if qs.is_empty() { "?".to_string() } else { qs };
    api.get_list(&format!("/api/chat/messages{qs}")).await
}

pub async fn send_message(api: Api, message: SendMessage) -> ApiResult<ChatMessageItem> {
    api.post_data("/api/chat/messages", Some(&message)).await
}

pub async fn edit_message(api: Api, message_id: i64, content: String) -> ApiResult<ChatMessageItem> {
    api.patch_data(&format!("/api/chat/messages/{message_id}"), &json!({ "content": content })).await
}

pub async fn delete_message(api: Api, message_id: i64) -> ApiResult<StatusReply> {
    api.delete_data(&format!("/api/chat/messages/{message_id}")).await
}

pub async fn toggle_reaction(api: Api, message_id: i64, emoji: String) -> ApiResult<StatusReply> {
    api.post_data(&format!("/api/chat/messages/{message_id}/reactions"), Some(&json!({ "emoji": emoji }))).await
}

pub async fn toggle_pin(api: Api, message_id: i64) -> ApiResult<StatusReply> {
    api.post_data::<(), _>(&format!("/api/chat/messages/{message_id}/pin"), None).await
}

// ── users, presence ──

/// The public profile; falls back to the chat's own user endpoint like the web does.
pub async fn get_user_profile(api: Api, user_ref: String) -> ApiResult<UserInfo> {
    match api.get_data(&format!("/api/users/{user_ref}")).await {
        Ok(user) => Ok(user),
        Err(_) => api.get_data(&format!("/api/chat/users/{user_ref}")).await,
    }
}

pub async fn get_user_relationship(api: Api, user_ref: String) -> ApiResult<UserRelationshipStats> {
    api.get_data(&format!("/api/users/{user_ref}/relationship")).await
}

/// Who's online; `user_ids` empty = everyone the server tracks for you.
pub async fn get_presence(api: Api, user_ids: Vec<i64>) -> ApiResult<Presence> {
    let q = if user_ids.is_empty() { String::new() } else { format!("?user_ids={}", user_ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")) };
    api.get_data(&format!("/api/users/presence{q}")).await
}

// ── read state, sidebar prefs ──

pub async fn mark_room_read(api: Api, room_key: String, message_id: Option<i64>) -> ApiResult<StatusReply> {
    let body = match message_id {
        Some(id) => json!({ "message_id": id }),
        None => json!({}),
    };
    api.post_data(&format!("{}/read", room(&room_key)), Some(&body)).await
}

pub async fn set_room_prefs(api: Api, room_key: String, pinned: Option<bool>, archived: Option<bool>, muted: Option<bool>) -> ApiResult<RoomPrefs> {
    let mut body = json!({});
    if let Some(v) = pinned {
        body["pinned"] = json!(v);
    }
    if let Some(v) = archived {
        body["archived"] = json!(v);
    }
    if let Some(v) = muted {
        body["muted"] = json!(v);
    }
    api.put_data(&format!("{}/prefs", room(&room_key)), &body).await
}

pub async fn get_unreads(api: Api) -> ApiResult<UnreadsResponse> {
    api.get_data("/api/chat/unreads").await
}

/// Polls and quizzes (pollsApi).
pub mod polls {
    use serde::Deserialize;
    use serde_json::json;

    use crate::api::client::{Api, ApiResult};

    /// Poll / quiz content — carried in the message's attachment_data (encrypted in private rooms).
    #[derive(Debug, Clone, Default, serde::Serialize, Deserialize)]
    #[serde(default)]
    pub struct PollContent {
        /// "poll"
        #[serde(rename = "type")]
        pub kind_tag: String,
        /// "poll" | "quiz"
        pub kind: String,
        pub question: String,
        pub options: Vec<PollOption>,
        pub multiple: Option<bool>,
        pub anonymous: Option<bool>,
        /// Quiz only: id of the right option.
        pub correct: Option<String>,
        /// Quiz only: shown after answering.
        pub explanation: Option<String>,
    }

    #[derive(Debug, Clone, Default, serde::Serialize, Deserialize)]
    #[serde(default)]
    pub struct PollOption {
        pub id: String,
        pub text: String,
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct PollState {
        pub message_id: i64,
        pub kind: String,
        pub anonymous: bool,
        pub allow_change: bool,
        pub creator_id: Option<i64>,
        pub closes_at: Option<String>,
        pub closed: bool,
        pub total_voters: i64,
        pub votes: Vec<Vote>,
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct Vote {
        pub user_id: Option<i64>,
        pub choice: String,
        pub mine: bool,
    }

    pub async fn get(api: Api, message_id: i64) -> ApiResult<PollState> {
        api.get(&format!("/api/chat/polls/{message_id}")).await
    }

    pub async fn vote(api: Api, message_id: i64, choice: String) -> ApiResult<PollState> {
        api.post(&format!("/api/chat/polls/{message_id}/vote"), &json!({ "choice": choice })).await
    }

    pub async fn close(api: Api, message_id: i64) -> ApiResult<PollState> {
        api.post_empty(&format!("/api/chat/polls/{message_id}/close")).await
    }
}

/// Thread replies (threadsApi).
pub mod threads {
    use serde::Deserialize;

    use super::ChatMessageItem;
    use crate::api::client::{Api, ApiResult};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct Thread {
        pub root: ChatMessageItem,
        pub replies: Vec<ChatMessageItem>,
    }

    pub async fn get(api: Api, root_id: i64) -> ApiResult<Thread> {
        api.get(&format!("/api/chat/messages/{root_id}/thread")).await
    }
}

/// Messages queued to send later (scheduledApi).
pub mod scheduled {
    use serde::Deserialize;
    use serde_json::Value;

    use super::StatusReply;
    use crate::api::client::{enc, Api, ApiResult};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct ScheduledMessage {
        pub id: i64,
        pub room_id: String,
        pub content: String,
        pub message_type: String,
        pub attachment_data: Option<Value>,
        pub send_at: String,
    }

    pub async fn list(api: Api, room_id: String) -> ApiResult<Vec<ScheduledMessage>> {
        api.get(&format!("/api/chat/scheduled?room_id={}", enc(&room_id))).await
    }

    pub async fn cancel(api: Api, id: i64) -> ApiResult<StatusReply> {
        api.delete(&format!("/api/chat/scheduled/{id}")).await
    }
}

/// Reporting a message or a room to moderators (reportsApi).
pub mod reports {
    use serde_json::json;

    use super::StatusReply;
    use crate::api::client::{enc, Api, ApiResult};

    /// spam | scam | harassment | illegal | other
    pub type ReportReason = String;

    pub async fn report(api: Api, message_id: i64, reason: ReportReason, comment: Option<String>, content_snapshot: Option<String>) -> ApiResult<StatusReply> {
        let mut body = json!({ "reason": reason });
        if let Some(c) = comment {
            body["comment"] = json!(c);
        }
        if let Some(s) = content_snapshot {
            body["content_snapshot"] = json!(s);
        }
        api.post(&format!("/api/chat/messages/{message_id}/report"), &body).await
    }

    pub async fn report_room(api: Api, room_key: String, reason: ReportReason, comment: Option<String>) -> ApiResult<StatusReply> {
        let mut body = json!({ "reason": reason });
        if let Some(c) = comment {
            body["comment"] = json!(c);
        }
        api.post(&format!("/api/chat/rooms/{}/report", enc(&room_key)), &body).await
    }
}

/// The public-room catalog: rooms show in your sidebar only after you join (or write in) them.
pub mod public_rooms {
    use serde::Deserialize;

    use super::{ChatRoomItem, StatusReply};
    use crate::api::client::{enc, Api, ApiResult};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct PublicRoomListing {
        pub room_key: String,
        pub title: String,
        pub topic: Option<String>,
        pub avatar: Option<String>,
        pub member_count: i64,
        pub owner_username: Option<String>,
        pub is_member: bool,
        pub is_owner: bool,
        pub created_at: Option<String>,
        pub last_activity: Option<String>,
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct Catalog {
        pub total: i64,
        pub rooms: Vec<PublicRoomListing>,
    }

    pub async fn catalog(api: Api, search: String, offset: i64, limit: i64) -> ApiResult<Catalog> {
        let search = search.trim();
        let s = if search.is_empty() { String::new() } else { format!("&search={}", enc(search)) };
        api.get(&format!("/api/chat/rooms/public/catalog?limit={limit}&offset={offset}{s}")).await
    }

    pub async fn join(api: Api, room_key: String) -> ApiResult<ChatRoomItem> {
        api.post_empty(&format!("/api/chat/rooms/{}/join", enc(&room_key))).await
    }

    pub async fn leave(api: Api, room_key: String) -> ApiResult<StatusReply> {
        api.post_empty(&format!("/api/chat/rooms/{}/leave", enc(&room_key))).await
    }

    /// The owner deletes their public / group room; admins can delete any.
    pub async fn remove(api: Api, room_key: String) -> ApiResult<StatusReply> {
        api.delete(&format!("/api/chat/rooms/{}", enc(&room_key))).await
    }
}

/// Audio / video attachments as chat messages carry them (attachment_data).
pub mod attachments {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    #[serde(default)]
    pub struct AudioAttachmentData {
        pub file_data: String,
        pub file_name: String,
        pub file_size: i64,
        pub file_type: String,
        pub duration: f64,
        pub waveform: Option<Vec<f64>>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    #[serde(default)]
    pub struct VideoAttachmentData {
        pub file_data: String,
        pub file_name: String,
        pub file_size: i64,
        pub file_type: String,
        pub duration: f64,
        pub is_round: Option<bool>,
        pub thumbnail: Option<String>,
    }
}
