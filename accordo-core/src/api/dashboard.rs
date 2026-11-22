// SPDX-License-Identifier: GPL-3.0-or-later
//! The dashboard (src/api/dashboard.ts).

use serde::Deserialize;

use super::audit_log::AuditLogEntry;
use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ActionCount {
    pub action: String,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DashboardStats {
    pub cheques_total: Option<i64>,
    pub cheques_pending: Option<i64>,
    pub invoices_total: Option<i64>,
    pub invoices_paid: Option<i64>,
    pub chats_total: Option<i64>,
    pub bots_total: Option<i64>,
    pub bots_online: Option<i64>,
    pub guilds_total: Option<i64>,
    pub members_total: Option<i64>,
    pub actions_last_24h: i64,
    pub actions_last_7d: i64,
    pub top_actions_7d: Vec<ActionCount>,
    pub recent_events: Vec<AuditLogEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TokenAmount {
    pub token: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AttentionItem {
    pub kind: String,
    /// danger | warning | success | info
    pub severity: String,
    pub title: String,
    pub subtitle: String,
    pub link: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ActivityItem {
    /// "in" | "out"
    pub direction: String,
    pub kind: String,
    pub title: String,
    pub amount: f64,
    pub token: String,
    pub counterparty: Option<String>,
    pub at: String,
    pub link: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Received {
    pub by_token: Vec<TokenAmount>,
    pub prev_by_token: Vec<TokenAmount>,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Awaiting {
    pub by_token: Vec<TokenAmount>,
    pub count: i64,
    pub overdue: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Locked {
    pub by_token: Vec<TokenAmount>,
    pub count: i64,
    pub expiring_soon: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DashboardOverview {
    pub period_days: i64,
    pub received: Received,
    pub awaiting: Awaiting,
    pub locked: Locked,
    pub attention: Vec<AttentionItem>,
    pub activity: Vec<ActivityItem>,
    pub pay_link_username: Option<String>,
}

pub async fn stats(api: Api) -> ApiResult<DashboardStats> {
    api.get("/api/dashboard/stats").await
}

pub async fn overview(api: Api) -> ApiResult<DashboardOverview> {
    api.get("/api/dashboard/overview").await
}
