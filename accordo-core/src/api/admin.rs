// SPDX-License-Identifier: GPL-3.0-or-later
//! Admin panel (src/api/admin.ts): users, access codes and the waitlist, moderation of messages /
//! rooms / campaigns, announcements, registration limits. The server checks the admin flag.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{enc, Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AdminUser {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub discord_username: Option<String>,
    pub current_bot_count: i64,
    pub allowed_bots_limit: i64,
    pub created_at: String,
    pub invoice_count: i64,
    pub cheque_count: i64,
    pub is_active: bool,
    pub is_admin: bool,
    pub api_login: bool,
    /// Public rooms this account may own (0 = can't create any). The four limits: None = no limit
    /// (admins, unless the server's default limits file sets one for them).
    pub public_rooms_limit: Option<i64>,
    pub public_rooms_owned: i64,
    /// May post articles in chats.
    pub allowed_write_article: bool,
    /// May use the apps' developer tools (granted here; admins always may).
    pub is_developer: bool,
    /// The profile's label, set in Control (no rights of its own).
    pub role: Option<String>,
    /// Private groups this account may own (server default until an admin sets it).
    pub private_groups_limit: Option<i64>,
    pub private_groups_owned: i64,
    /// Own rooms this account may show in everyone's Suggested (0 = none).
    pub suggested_rooms_limit: Option<i64>,
    /// Mini-apps this account may register (server default until an admin sets it).
    pub mini_apps_limit: Option<i64>,
    pub mini_apps_owned: i64,
    /// Subscriptions paid until (None = never / ended).
    pub basic_until: Option<String>,
    pub lite_until: Option<String>,
    pub pro_until: Option<String>,
    pub enterprise_until: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Frozen {
    pub at: String,
    pub by: i64,
    pub reason: Option<String>,
}

/// A group or public room a user owns, with its member cap.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AdminOwnedRoom {
    pub room_key: String,
    pub title: Option<String>,
    /// "group" | "public"
    pub chat_type: String,
    pub members: i64,
    pub cap: i64,
    pub admin_cap: Option<i64>,
    pub owner_cap: Option<i64>,
    pub default_cap: i64,
    pub max_admin_cap: i64,
    /// Set when an admin froze the room: nobody new gets in, settings can't change.
    pub frozen: Option<Frozen>,
    /// Visibility rules: {everyone: true} shows a public room in every account's sidebar.
    pub view: Option<Value>,
    pub created_at: String,
}

/// Per-account permissions (the profile's Control section); send only what changes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserControlPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_rooms_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_write_article: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_developer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_groups_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_rooms_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mini_apps_limit: Option<i64>,
    /// Subscription ends: ISO date-time, or "" to take it away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lite_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pro_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise_until: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserRef {
    pub id: i64,
    pub username: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MessageReportItem {
    pub id: i64,
    pub message_id: i64,
    pub room_id: String,
    /// spam | scam | harassment | illegal | other
    pub reason: String,
    pub comment: Option<String>,
    pub content_snapshot: Option<String>,
    pub message_type: Option<String>,
    /// open | resolved | dismissed
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub reporter: Option<UserRef>,
    pub reported_user: Option<UserRef>,
    pub report_count: i64,
    pub message_exists: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CampaignReportItem {
    pub id: i64,
    pub campaign_id: String,
    pub title: Option<String>,
    pub creator_username: Option<String>,
    pub chain_name: Option<String>,
    pub raised: Option<f64>,
    pub token_symbol: Option<String>,
    pub contributors: Option<i64>,
    pub hidden: Option<bool>,
    pub reason: String,
    pub comment: Option<String>,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub reporter: UserRef,
    pub report_count: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RoomReportItem {
    pub id: i64,
    pub room_key: String,
    pub room_title: Option<String>,
    pub room_topic: Option<String>,
    pub member_count: i64,
    pub room_exists: bool,
    pub reason: String,
    pub comment: Option<String>,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub reporter: Option<UserRef>,
    pub owner: Option<UserRef>,
    pub report_count: i64,
}

/// info | success | warning | critical
pub type AnnouncementLevel = String;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SentAnnouncement {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub level: AnnouncementLevel,
    pub link: Option<String>,
    /// "all" | "users"
    pub target_type: String,
    pub recipients_count: i64,
    pub created_at: String,
    pub created_by: Option<String>,
    pub retracted: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SendAnnouncementPayload {
    pub title: String,
    pub body: String,
    pub level: AnnouncementLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Option<String>>,
    /// "all" | "users"
    pub target_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ResolvedRecipient {
    pub id: i64,
    pub username: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ResolvedRecipients {
    pub resolved: Vec<ResolvedRecipient>,
    pub unresolved: Vec<String>,
}

/// The defaults every account (by its role) and room gets without its own value: the server's
/// data/default_limits.json, in its shape — {"user": {…}, "developer": {…}, "admin": {…},
/// "groups": {…}, "file": bool} (`file` false = the server's .env values, no file yet).
pub type DefaultLimits = Value;

/// The account limits: (key in the file, label).
pub const ACCOUNT_LIMITS: [(&str, &str); 4] = [
    ("max_groups_per_user", "Groups"),
    ("max_public_per_user", "Public rooms"),
    ("max_suggested_per_user", "In Suggested"),
    ("max_mini_apps_per_user", "Mini-apps"),
];
/// The roles' sections: (key, label).
pub const LIMIT_ROLES: [(&str, &str); 3] = [("user", "User"), ("developer", "Developer"), ("admin", "Admin")];
/// The room member caps: (key in the file, label).
pub const ROOM_LIMITS: [(&str, &str); 4] = [
    ("max_users_per_group", "Members per group"),
    ("max_users_per_public_group", "Members per public room"),
    ("admin_max_users_per_group", "Admin may raise a group to"),
    ("admin_max_users_per_public_group", "…a public room to"),
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RegistrationLimits {
    pub enabled: bool,
    pub max_users: i64,
    pub current_users: i64,
    pub remaining_spots: Option<i64>,
    pub rate_limit_per_ip: i64,
    pub rate_limit_window_seconds: i64,
    pub invite_code_required: bool,
    pub invite_code: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateRegistrationLimitsPayload {
    pub enabled: bool,
    pub max_users: i64,
    pub rate_limit_per_ip: i64,
    pub rate_limit_window_seconds: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_code: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AccessCodeItem {
    pub code: String,
    /// Who signed up with it; None = still valid.
    pub used_by: Option<String>,
    /// Generated.
    pub created_at: Option<String>,
    /// Activated.
    pub used_at: Option<String>,
    /// Whose invite this is; None = issued by an admin.
    pub invited_by: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AccessCodes {
    pub unused: i64,
    pub total: i64,
    pub codes: Vec<AccessCodeItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WaitlistItem {
    pub id: i64,
    pub email: String,
    pub created_at: Option<String>,
    pub invited_at: Option<String>,
    pub access_code: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Waitlist {
    pub pending: i64,
    pub total: i64,
    pub entries: Vec<WaitlistItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WaitlistInvite {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WaitlistInvited {
    pub emailed: bool,
    pub invited: Vec<WaitlistInvite>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ModerationResult {
    pub resolved_reports: i64,
    pub done: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFilter {
    Open,
    Resolved,
    Dismissed,
    All,
}

impl ReportFilter {
    fn as_str(self) -> &'static str {
        match self {
            ReportFilter::Open => "open",
            ReportFilter::Resolved => "resolved",
            ReportFilter::Dismissed => "dismissed",
            ReportFilter::All => "all",
        }
    }
}

// ── access codes, waitlist ──

/// `status`: "all" | "unused" | "used".
pub async fn list_access_codes(api: Api, status: &'static str) -> ApiResult<AccessCodes> {
    api.get(&format!("/api/admin/access-codes?status={status}")).await
}

/// `for_username`: give the codes to that user to hand out (their Settings → Invite friends).
pub async fn create_access_codes(api: Api, count: i64, for_username: Option<String>) -> ApiResult<Value> {
    let for_username = for_username.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    api.post("/api/admin/access-codes", &json!({ "count": count, "for_username": for_username })).await
}

pub async fn delete_access_code(api: Api, code: String) -> ApiResult<Value> {
    api.delete(&format!("/api/admin/access-codes/{}", enc(&code))).await
}

/// `status`: "all" | "pending" | "invited".
pub async fn list_waitlist(api: Api, status: &'static str) -> ApiResult<Waitlist> {
    api.get(&format!("/api/admin/waitlist?status={status}")).await
}

pub async fn invite_from_waitlist(api: Api, ids: Vec<i64>) -> ApiResult<WaitlistInvited> {
    api.post("/api/admin/waitlist/invite", &json!({ "ids": ids })).await
}

pub async fn delete_waitlist_entry(api: Api, id: i64) -> ApiResult<Value> {
    api.delete(&format!("/api/admin/waitlist/{id}")).await
}

// ── users ──

pub async fn list_users(api: Api) -> ApiResult<Vec<AdminUser>> {
    api.get("/api/admin/users").await
}

pub async fn list_invoices(api: Api) -> ApiResult<Vec<Value>> {
    api.get("/api/admin/invoices").await
}

pub async fn list_cheques(api: Api) -> ApiResult<Vec<Value>> {
    api.get("/api/admin/cheques").await
}

pub async fn update_user_limit(api: Api, user_id: i64, new_limit: i64) -> ApiResult<Value> {
    api.put(&format!("/api/admin/users/{user_id}/limit"), &json!({ "new_limit": new_limit })).await
}

pub async fn set_user_active(api: Api, user_id: i64, active: bool) -> ApiResult<Value> {
    api.put(&format!("/api/admin/users/{user_id}/active"), &json!({ "active": active })).await
}

pub async fn set_user_api_login(api: Api, user_id: i64, enabled: bool) -> ApiResult<Value> {
    api.put(&format!("/api/admin/users/{user_id}/api-login"), &json!({ "enabled": enabled })).await
}

pub async fn update_user_control(api: Api, user_id: i64, patch: UserControlPatch) -> ApiResult<Value> {
    api.patch(&format!("/api/admin/users/{user_id}/control"), &patch).await
}

pub async fn get_online_users(api: Api) -> ApiResult<Value> {
    api.get("/api/admin/online").await
}

pub async fn get_user_rooms(api: Api, user_id: i64) -> ApiResult<Vec<AdminOwnedRoom>> {
    api.get(&format!("/api/admin/users/{user_id}/rooms")).await
}

// ── rooms ──

pub async fn set_room_view(api: Api, room_key: String, view: Option<Value>) -> ApiResult<Value> {
    api.put(&format!("/api/admin/rooms/{room_key}/view"), &json!({ "view": view })).await
}

pub async fn set_room_frozen(api: Api, room_key: String, frozen: bool, reason: Option<String>) -> ApiResult<Value> {
    let mut body = json!({ "frozen": frozen });
    if let Some(r) = reason {
        body["reason"] = json!(r);
    }
    api.put(&format!("/api/admin/rooms/{room_key}/freeze"), &body).await
}

/// None = back to the default cap for that kind of room.
pub async fn set_room_member_cap(api: Api, room_key: String, cap: Option<i64>) -> ApiResult<Value> {
    api.put(&format!("/api/admin/rooms/{room_key}/member-cap"), &json!({ "cap": cap })).await
}

// ── registration ──

pub async fn get_registration_limits(api: Api) -> ApiResult<RegistrationLimits> {
    api.get("/api/admin/registration-limits").await
}

pub async fn update_registration_limits(api: Api, payload: UpdateRegistrationLimitsPayload) -> ApiResult<RegistrationLimits> {
    api.put("/api/admin/registration-limits", &payload).await
}

pub async fn get_default_limits(api: Api) -> ApiResult<DefaultLimits> {
    api.get("/api/admin/default-limits").await
}

pub async fn update_default_limits(api: Api, values: Value) -> ApiResult<DefaultLimits> {
    api.put("/api/admin/default-limits", &values).await
}

// ── announcements ──

pub async fn resolve_recipients(api: Api, identifiers: Vec<String>) -> ApiResult<ResolvedRecipients> {
    api.post("/api/admin/notifications/resolve", &json!({ "identifiers": identifiers })).await
}

pub async fn send_notification(api: Api, payload: SendAnnouncementPayload) -> ApiResult<SentAnnouncement> {
    api.post("/api/admin/notifications", &payload).await
}

pub async fn list_notifications(api: Api) -> ApiResult<Vec<SentAnnouncement>> {
    api.get("/api/admin/notifications").await
}

pub async fn retract_notification(api: Api, id: i64) -> ApiResult<SentAnnouncement> {
    api.post_empty(&format!("/api/admin/notifications/{id}/retract")).await
}

// ── moderation ──

pub async fn list_reports(api: Api, filter: ReportFilter) -> ApiResult<Vec<MessageReportItem>> {
    api.get(&format!("/api/admin/reports?status={}", filter.as_str())).await
}

/// `action`: delete_message | suspend_sender | delete_and_suspend | dismiss.
pub async fn act_on_report(api: Api, id: i64, action: &'static str) -> ApiResult<ModerationResult> {
    api.post(&format!("/api/admin/reports/{id}/action"), &json!({ "action": action })).await
}

pub async fn list_room_reports(api: Api, filter: ReportFilter) -> ApiResult<Vec<RoomReportItem>> {
    api.get(&format!("/api/admin/room-reports?status={}", filter.as_str())).await
}

/// `action`: delete_room | suspend_owner | delete_and_suspend | dismiss.
pub async fn act_on_room_report(api: Api, id: i64, action: &'static str) -> ApiResult<ModerationResult> {
    api.post(&format!("/api/admin/room-reports/{id}/action"), &json!({ "action": action })).await
}

pub async fn list_campaign_reports(api: Api, filter: ReportFilter) -> ApiResult<Vec<CampaignReportItem>> {
    api.get(&format!("/api/admin/campaign-reports?status={}", filter.as_str())).await
}

/// `action`: hide | hide_and_suspend | dismiss.
pub async fn act_on_campaign_report(api: Api, id: i64, action: &'static str, reason: Option<String>) -> ApiResult<ModerationResult> {
    let mut body = json!({ "action": action });
    if let Some(r) = reason {
        body["reason"] = json!(r);
    }
    api.post(&format!("/api/admin/campaign-reports/{id}/action"), &body).await
}

pub async fn set_campaign_hidden(api: Api, campaign_id: String, hidden: bool, reason: Option<String>) -> ApiResult<Value> {
    let mut body = json!({ "hidden": hidden });
    if let Some(r) = reason {
        body["reason"] = json!(r);
    }
    api.post(&format!("/api/admin/campaigns/{campaign_id}/visibility"), &body).await
}
