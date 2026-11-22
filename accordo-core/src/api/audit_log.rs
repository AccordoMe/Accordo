// SPDX-License-Identifier: GPL-3.0-or-later
//! The audit log (src/api/auditLog.ts).

use serde::Deserialize;

use super::client::{query, Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuditLogEntry {
    pub id: i64,
    pub bot_id: Option<i64>,
    pub guild_id: Option<String>,
    /// "user" | "discord_event"
    pub actor_type: String,
    pub actor_user_id: Option<i64>,
    pub actor_discord_id: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub detail_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuditLogPage {
    pub entries: Vec<AuditLogEntry>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AuditLogFilters {
    pub bot_id: Option<i64>,
    pub guild_id: Option<String>,
    pub action: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(api: Api, f: AuditLogFilters) -> ApiResult<AuditLogPage> {
    let q = query(&[
        ("bot_id", f.bot_id.map(|v| v.to_string())),
        ("guild_id", f.guild_id.filter(|s| !s.is_empty())),
        ("action", f.action.filter(|s| !s.is_empty())),
        ("limit", f.limit.map(|v| v.to_string())),
        ("offset", f.offset.map(|v| v.to_string())),
    ]);
    api.get(&format!("/api/audit-log{q}")).await
}
