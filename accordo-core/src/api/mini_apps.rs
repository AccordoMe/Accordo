// SPDX-License-Identifier: GPL-3.0-or-later
//! Mini-apps (src/api/miniApps.ts): the permission list, registration by admins (`admin`), apps
//! in chats (`room`), a developer's own apps and slots (`dev`), Settings → Connected apps
//! (`connected`) and the Apps folder of the chat list (`messages`).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Permissions in groups (docs/MINIAPP_API_RU.md §8; backend mini_app/permissions.py), in the
/// order the server keeps them and every screen shows them.
pub const PERMISSION_GROUPS: [(&str, &str, &[&str]); 5] = [
    ("profile", "Profile", &["profile.nick", "profile.avatar", "profile.email", "profile.wallet"]),
    ("payments", "Payments", &["payments.pay", "payments.invoice"]),
    ("history", "History", &["history.invoices", "history.cheques"]),
    ("notify", "Notify", &["notify.push", "notify.messages"]),
    ("chat", "Chat", &["chat.send", "chat.members"]),
];

/// Every permission, in order.
pub fn all_permissions() -> impl Iterator<Item = &'static str> {
    PERMISSION_GROUPS.iter().flat_map(|(_, _, perms)| perms.iter().copied())
}

/// Held in one chat only (backend PER_ROOM); chat.send is the chat owner's answer for everyone.
pub const PER_ROOM_PERMISSIONS: [&str; 2] = ["chat.send", "chat.members"];

/// The first, coarse names still mean these (a manifest or an older app may use them). Signing and
/// sending with the user's wallet are gone — Accordo isn't a wallet provider — so they mean nothing.
pub fn legacy_permission(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "profile" => &["profile.nick", "profile.avatar"],
        "email" => &["profile.email"],
        "wallet" | "wallet.address" => &["profile.wallet"],
        "wallet.sign" | "wallet.transact" => &[],
        "payments" => &["payments.pay", "payments.invoice"],
        "history.read" => &["history.invoices", "history.cheques"],
        "notify" => &["notify.push"],
        _ => return None,
    })
}

/// Old names → new ones, in the fixed order, no duplicates (unknown names dropped).
pub fn expand_permissions<S: AsRef<str>>(perms: &[S]) -> Vec<&'static str> {
    let mut want: HashSet<String> = HashSet::new();
    for p in perms {
        match legacy_permission(p.as_ref()) {
            Some(mapped) => want.extend(mapped.iter().map(|s| s.to_string())),
            None => {
                want.insert(p.as_ref().to_string());
            }
        }
    }
    all_permissions().filter(|p| want.contains(*p)).collect()
}

/// Short names, for the columns.
pub fn permission_label(permission: &str) -> &'static str {
    match permission {
        "profile.nick" => "Nickname",
        "profile.avatar" => "Avatar",
        "profile.email" => "Email",
        "profile.wallet" => "Wallet & network",
        "payments.pay" => "Ask to pay",
        "payments.invoice" => "Issue invoices",
        "history.invoices" => "Invoices",
        "history.cheques" => "Cheques",
        "notify.push" => "Notifications",
        "notify.messages" => "Messages to the user",
        "chat.send" => "Post in a chat",
        "chat.members" => "See chat members",
        _ => "",
    }
}

/// What each permission lets an app do — for developers and admins.
pub fn permission_summary(permission: &str) -> &'static str {
    match permission {
        "profile.nick" => "See the user's nickname",
        "profile.avatar" => "See the user's avatar",
        "profile.email" => "See the account's email and whether it's confirmed",
        "profile.wallet" => "See the address of the user's current wallet and its network (never signs or sends)",
        "payments.pay" => "Ask the user to pay someone (each payment confirmed)",
        "payments.invoice" => "Ask the user to issue an invoice (each one confirmed)",
        "history.invoices" => "See the user's own invoices (no addresses)",
        "history.cheques" => "See the user's own cheques (no addresses, no links)",
        "notify.push" => "Send notifications from its server (3 a day per user)",
        "notify.messages" => "Write to the user in its own chat in Accordo's Apps folder",
        "chat.send" => "Post in a chat — only the chat's owner can allow it",
        "chat.members" => "See who is in a chat (nicknames only)",
        _ => "",
    }
}

/// draft: in no slot, not sent for review · pending: waiting for review (draft and pending: only its
/// developer can open it) · active: everywhere · rejected: review_note says why · disabled: off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    #[default]
    Draft,
    Pending,
    Active,
    Rejected,
    Disabled,
    #[serde(other)]
    Unknown,
}

/// A paid place for one app, a year at a time.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MiniAppSlot {
    pub id: i64,
    /// The app in it (None = free).
    pub app_id: Option<i64>,
    /// Paid up to here; None = no end (given by Accordo).
    pub expires_at: Option<String>,
    pub active: bool,
    pub granted: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MiniApp {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub url: String,
    pub origin: String,
    pub permissions: Vec<String>,
    pub status: AppStatus,
    pub review_note: Option<String>,
    pub reviewed_at: Option<String>,
    /// The slot the app is in (None = a draft, rejected, or apps are free).
    pub slot: Option<MiniAppSlot>,
    pub developer: Option<String>,
    /// Last characters of the app's server key (None = none made yet).
    pub server_key_hint: Option<String>,
    pub manifest_checked_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// The registration form (admins), also what a manifest is read into.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MiniAppForm {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub url: String,
    pub permissions: Vec<String>,
    pub developer: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ManifestRead {
    pub manifest: HashMap<String, Value>,
    pub form: MiniAppForm,
}

/// What a chat member sees of an app.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RoomMiniApp {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub url: String,
    pub origin: String,
    pub permissions: Vec<String>,
}

/// One permission's answer: whether the user gave it, plus what was picked (wallet: {evm, solana}).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Grant {
    pub granted: bool,
    pub decided: Option<bool>,
    pub data: Option<HashMap<String, String>>,
}

/// permission → its answer.
pub type MiniAppGrants = HashMap<String, Grant>;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MiniAppLaunch {
    pub app: RoomMiniApp,
    pub grants: MiniAppGrants,
    /// Pseudo-ids for this app only (docs/MINIAPP_API_RU.md §7.2–7.3).
    pub user_ref: String,
    pub room_ref: Option<String>,
    /// The user's role in the room it was opened from: owner | admin | member | guest.
    pub room_role: Option<String>,
    pub start_param: Option<String>,
}

/// `history.read`: the user's own invoices and cheques, no addresses.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppHistoryItem {
    /// "invoice" | "cheque"
    pub kind: String,
    /// issued | billed | sent | claimed
    pub role: String,
    pub status: String,
    pub amount: Option<String>,
    pub token: Option<String>,
    pub chain: Option<String>,
    pub note: Option<String>,
    pub counterparty: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppBrief {
    pub id: Option<i64>,
    pub slug: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub status: Option<AppStatus>,
}

/// A notification an app's server sent.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppNotificationItem {
    pub id: i64,
    pub app: AppBrief,
    pub title: String,
    pub body: String,
    pub start_param: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DevAppForm {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub url: String,
    pub permissions: Vec<String>,
}

/// Buying a slot-year: an Accordo invoice with these exact terms, created and paid by the wallet.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SlotOrder {
    pub id: i64,
    pub slot_id: Option<i64>,
    pub renew: bool,
    /// "pending" | "paid"
    pub status: String,
    pub chain_id: String,
    pub invoice_id: String,
    pub contract: String,
    pub merchant: String,
    pub token: String,
    pub symbol: String,
    pub decimals: u8,
    pub amount_units: String,
    pub usd: f64,
    pub days: i64,
    pub fee_bps: i64,
    pub expires_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SlotsState {
    pub slots: Vec<MiniAppSlot>,
    /// Started, not counted yet.
    pub pending: Vec<SlotOrder>,
    /// None = no limit (admin).
    pub limit: Option<i64>,
    pub can_buy: bool,
    /// check(): the order that was counted.
    pub order: Option<SlotOrder>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FeeToken {
    pub symbol: String,
    pub address: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FeeNetwork {
    pub chain_id_dec: String,
    pub name: String,
    pub treasury: String,
    /// AccordoInvoice there.
    pub contract: String,
    pub testnet: bool,
    pub tokens: Vec<FeeToken>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppFeeInfo {
    /// Per slot per year, in USD stablecoins; 0 = free (no slots needed).
    pub usd: f64,
    pub days: i64,
    pub networks: Vec<FeeNetwork>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RoomRef {
    pub key: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConnectedGrant {
    pub permission: String,
    /// chat.* hold in one chat only: shown once per chat answered.
    pub room: Option<RoomRef>,
    pub allowed: bool,
    pub granted_at: Option<String>,
}

/// Settings → Connected apps.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConnectedApp {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub origin: String,
    pub status: AppStatus,
    /// Every permission its registration lists, on or off.
    pub grants: Vec<ConnectedGrant>,
    pub connected_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppMessageButton {
    pub label: String,
    pub start_param: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppMessage {
    pub id: i64,
    pub text: String,
    /// Opens the app with this start param.
    pub button: Option<AppMessageButton>,
    pub created_at: Option<String>,
    pub read: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppThread {
    pub app: AppBrief,
    pub last: AppMessage,
    pub unread: i64,
    pub can_write: bool,
}

/// Registration (admin-only for now; backend routers/miniApps.py).
pub mod admin {
    use serde_json::{json, Value};

    use super::{ManifestRead, MiniApp, MiniAppForm};
    use crate::api::client::{Api, ApiResult};

    pub async fn list(api: Api) -> ApiResult<Vec<MiniApp>> {
        api.get("/api/admin/apps").await
    }

    /// Reads <origin>/accordo-app.json and shapes it like the form. Saves nothing.
    pub async fn read_manifest(api: Api, url: String) -> ApiResult<ManifestRead> {
        api.post("/api/admin/apps/manifest", &json!({ "url": url })).await
    }

    pub async fn create(api: Api, form: MiniAppForm, manifest: Option<Value>) -> ApiResult<MiniApp> {
        let mut body = serde_json::to_value(&form).unwrap_or_default();
        body["manifest"] = manifest.unwrap_or(Value::Null);
        api.post("/api/admin/apps", &body).await
    }

    /// Everything but the slug.
    pub async fn update(api: Api, id: i64, form: MiniAppForm) -> ApiResult<MiniApp> {
        let mut body = serde_json::to_value(&form).unwrap_or_default();
        if let Some(map) = body.as_object_mut() {
            map.remove("slug");
        }
        api.patch(&format!("/api/admin/apps/{id}"), &body).await
    }

    /// Approve ("active"), reject with a note the developer sees, or switch off ("disabled").
    pub async fn set_status(api: Api, id: i64, status: &'static str, note: Option<String>) -> ApiResult<MiniApp> {
        api.put(&format!("/api/admin/apps/{id}/status"), &json!({ "status": status, "note": note })).await
    }

    /// A year of the app's slot on the house (e.g. paid outside Accordo); a draft goes to review.
    pub async fn grant_year(api: Api, id: i64) -> ApiResult<MiniApp> {
        api.post_empty(&format!("/api/admin/apps/{id}/grant-year")).await
    }

    pub async fn remove(api: Api, id: i64) -> ApiResult<Value> {
        api.delete(&format!("/api/admin/apps/{id}")).await
    }
}

/// Mini-apps in chats: the room's owner picks them, members open them.
pub mod room {
    use std::collections::HashMap;

    use serde::Deserialize;
    use serde_json::json;

    use super::{AppHistoryItem, AppNotificationItem, MiniAppGrants, MiniAppLaunch, RoomMiniApp};
    use crate::api::client::{enc, Api, ApiResult};

    pub async fn catalog(api: Api) -> ApiResult<Vec<RoomMiniApp>> {
        api.get("/api/apps/catalog").await
    }

    pub async fn one(api: Api, slug: String) -> ApiResult<RoomMiniApp> {
        api.get(&format!("/api/apps/{}", enc(&slug))).await
    }

    pub async fn list(api: Api, room_key: String) -> ApiResult<Vec<RoomMiniApp>> {
        api.get(&format!("/api/apps/rooms/{}", enc(&room_key))).await
    }

    /// Owner only. Replaces the room's apps, in this order.
    pub async fn set(api: Api, room_key: String, slugs: Vec<String>) -> ApiResult<Vec<RoomMiniApp>> {
        api.put(&format!("/api/apps/rooms/{}", enc(&room_key)), &json!({ "slugs": slugs })).await
    }

    /// Records what the user just allowed (after they confirmed it in the Accordo window).
    pub async fn grant(api: Api, slug: String, room_key: Option<String>, permissions: Vec<String>, data: Option<HashMap<String, String>>) -> ApiResult<MiniAppGrants> {
        api.post(&format!("/api/apps/{}/grants", enc(&slug)), &json!({ "room_key": room_key, "permissions": permissions, "data": data })).await
    }

    /// The consent screen's answers (permission → allowed); declined ones are remembered too.
    pub async fn consent(api: Api, slug: String, room_key: Option<String>, decisions: HashMap<String, bool>) -> ApiResult<MiniAppGrants> {
        api.post(&format!("/api/apps/{}/consent", enc(&slug)), &json!({ "room_key": room_key, "decisions": decisions })).await
    }

    pub async fn revoke(api: Api, slug: String, room_key: Option<String>, permissions: Option<Vec<String>>) -> ApiResult<MiniAppGrants> {
        api.post(&format!("/api/apps/{}/grants/revoke", enc(&slug)), &json!({ "room_key": room_key, "permissions": permissions })).await
    }

    /// `room_key`: opened from that chat; None: on its own (catalog / link).
    pub async fn launch(api: Api, slug: String, room_key: Option<String>, start_param: Option<String>) -> ApiResult<MiniAppLaunch> {
        api.post(&format!("/api/apps/{}/launch", enc(&slug)), &json!({ "room_key": room_key, "start_param": start_param })).await
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct History {
        pub items: Vec<AppHistoryItem>,
    }

    /// Needs `history.read`.
    pub async fn history(api: Api, slug: String, limit: i64) -> ApiResult<History> {
        api.get(&format!("/api/apps/{}/history?limit={limit}", enc(&slug))).await
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct Notifications {
        pub notifications: Vec<AppNotificationItem>,
    }

    /// Notifications from apps in the last week (the bell's catch-up).
    pub async fn notifications(api: Api) -> ApiResult<Notifications> {
        api.get("/api/notifications/apps").await
    }
}

/// A developer's own apps: submit for review, edit, re-check the manifest, buy slots.
pub mod dev {
    use serde::Deserialize;
    use serde_json::{json, Value};

    use super::{AppFeeInfo, DevAppForm, ManifestRead, MiniApp, SlotOrder, SlotsState};
    use crate::api::client::{Api, ApiResult};

    pub async fn list(api: Api) -> ApiResult<Vec<MiniApp>> {
        api.get("/api/dev/apps").await
    }

    pub async fn read_manifest(api: Api, url: String) -> ApiResult<ManifestRead> {
        api.post("/api/dev/apps/manifest", &json!({ "url": url })).await
    }

    /// `slot_id`: the free slot it goes in (409 if that one isn't free); None = any free one.
    pub async fn submit(api: Api, form: DevAppForm, slot_id: Option<i64>) -> ApiResult<MiniApp> {
        let mut body = serde_json::to_value(&form).unwrap_or_default();
        body["slot_id"] = json!(slot_id);
        api.post("/api/dev/apps", &body).await
    }

    /// Everything but the slug.
    pub async fn edit(api: Api, id: i64, form: DevAppForm) -> ApiResult<MiniApp> {
        let mut body = serde_json::to_value(&form).unwrap_or_default();
        if let Some(map) = body.as_object_mut() {
            map.remove("slug");
        }
        api.patch(&format!("/api/dev/apps/{id}"), &body).await
    }

    pub async fn verify(api: Api, id: i64) -> ApiResult<MiniApp> {
        api.post_empty(&format!("/api/dev/apps/{id}/verify")).await
    }

    pub async fn remove(api: Api, id: i64) -> ApiResult<Value> {
        api.delete(&format!("/api/dev/apps/{id}")).await
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct ServerKey {
        /// Shown once; the old key stops working at once.
        pub key: String,
        pub app: MiniApp,
    }

    /// A new key for the app's own server.
    pub async fn server_key(api: Api, id: i64) -> ApiResult<ServerKey> {
        api.post_empty(&format!("/api/dev/apps/{id}/server-key")).await
    }

    /// Puts a draft / rejected app in a free slot (the one ending soonest, or `slot_id`) → review.
    pub async fn review(api: Api, id: i64, slot_id: Option<i64>) -> ApiResult<MiniApp> {
        api.post(&format!("/api/dev/apps/{id}/review"), &json!({ "slot_id": slot_id })).await
    }

    pub async fn fee(api: Api) -> ApiResult<AppFeeInfo> {
        api.get("/api/dev/apps/fee").await
    }

    pub async fn slots(api: Api) -> ApiResult<SlotsState> {
        api.get("/api/dev/apps/slots").await
    }

    /// The invoice to create and pay for a new slot, or a year more of `slot_id`.
    pub async fn order(api: Api, chain_id: String, token: String, slot_id: Option<i64>) -> ApiResult<SlotOrder> {
        api.post("/api/dev/apps/slots/orders", &json!({ "chain_id": chain_id, "token": token, "slot_id": slot_id })).await
    }

    /// Counts the order once its invoice is paid on-chain (409 while it isn't).
    pub async fn check(api: Api, order_id: i64) -> ApiResult<SlotsState> {
        api.post_empty(&format!("/api/dev/apps/slots/orders/{order_id}/check")).await
    }

    /// Drops an order nothing was paid on.
    pub async fn discard(api: Api, order_id: i64) -> ApiResult<SlotsState> {
        api.delete(&format!("/api/dev/apps/slots/orders/{order_id}")).await
    }
}

/// Settings → Connected apps (backend routers/connectedApps.py).
pub mod connected {
    use serde::Deserialize;
    use serde_json::json;

    use super::ConnectedApp;
    use crate::api::client::{Api, ApiResult};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct ConnectedApps {
        pub apps: Vec<ConnectedApp>,
    }

    pub async fn list(api: Api) -> ApiResult<ConnectedApps> {
        api.get("/api/connected-apps").await
    }

    /// `room_key`: "" outside a chat (chat.* hold per chat).
    pub async fn revoke(api: Api, app_id: i64, permission: String, room_key: String) -> ApiResult<ConnectedApps> {
        api.post(&format!("/api/connected-apps/{app_id}/revoke"), &json!({ "permission": permission, "room_key": room_key })).await
    }

    /// The switch: off keeps a "no" (the app has to ask again), on gives it back.
    pub async fn set(api: Api, app_id: i64, permission: String, allowed: bool, room_key: String) -> ApiResult<ConnectedApps> {
        api.post(&format!("/api/connected-apps/{app_id}/set"), &json!({ "permission": permission, "room_key": room_key, "allowed": allowed })).await
    }

    pub async fn disconnect(api: Api, app_id: i64) -> ApiResult<ConnectedApps> {
        api.delete(&format!("/api/connected-apps/{app_id}")).await
    }
}

/// The Apps folder of the chat list: what mini-apps wrote to you (backend routers/appMessages.py).
pub mod messages {
    use serde::Deserialize;
    use serde_json::Value;

    use super::{AppBrief, AppMessage, AppThread};
    use crate::api::client::{enc, Api, ApiResult};

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct Threads {
        pub threads: Vec<AppThread>,
    }

    #[derive(Debug, Clone, Default, Deserialize)]
    #[serde(default)]
    pub struct ThreadPage {
        pub app: AppBrief,
        pub messages: Vec<AppMessage>,
        pub has_more: bool,
        pub can_write: bool,
    }

    pub async fn threads(api: Api) -> ApiResult<Threads> {
        api.get("/api/app-messages").await
    }

    /// `before`: older than this message id (paging back).
    pub async fn thread(api: Api, slug: String, before: Option<i64>) -> ApiResult<ThreadPage> {
        let q = before.filter(|b| *b != 0).map(|b| format!("?before={b}")).unwrap_or_default();
        api.get(&format!("/api/app-messages/{}{q}", enc(&slug))).await
    }

    pub async fn read(api: Api, slug: String) -> ApiResult<Value> {
        api.post_empty(&format!("/api/app-messages/{}/read", enc(&slug))).await
    }

    pub async fn clear(api: Api, slug: String) -> ApiResult<Value> {
        api.delete(&format!("/api/app-messages/{}", enc(&slug))).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_expand_in_order() {
        assert_eq!(expand_permissions(&["notify", "profile", "wallet.sign", "nope"]), vec!["profile.nick", "profile.avatar", "notify.push"]);
        assert_eq!(expand_permissions(&["chat.send", "history.read"]), vec!["history.invoices", "history.cheques", "chat.send"]);
    }
}
