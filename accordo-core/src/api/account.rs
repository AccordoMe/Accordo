// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the account is signed in (components/settings/SessionsCard.tsx — the web calls these
//! from the component, not from src/api).

use serde::{Deserialize, Serialize};

use super::client::{Api, ApiResult};

/// One place the account is signed in. The server keeps the device name and hour-level
/// activity only — no IP addresses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceSession {
    pub id: String,
    pub device: String,
    #[serde(default)]
    pub via_api_key: bool,
    #[serde(default)]
    pub current: bool,
    pub signed_in_at: Option<String>,
    pub last_active_at: Option<String>,
}

#[derive(Deserialize)]
struct SessionsBody {
    sessions: Vec<DeviceSession>,
}

pub async fn sessions(api: Api) -> ApiResult<Vec<DeviceSession>> {
    let body: SessionsBody = api.get("/api/auth/sessions").await?;
    Ok(body.sessions)
}

/// Signs one session out; answers its id.
pub async fn end_session(api: Api, id: String) -> ApiResult<String> {
    let _: serde_json::Value = api.delete(&format!("/api/auth/sessions/{}", super::client::enc(&id))).await?;
    Ok(id)
}

#[derive(Deserialize)]
struct EndOthersBody {
    access_token: String,
}

/// Signs out every other session; this one gets a fresh token (the old one stops working).
pub async fn end_other_sessions(api: Api) -> ApiResult<String> {
    let body: EndOthersBody = api.post_empty("/api/auth/sessions/end-others").await?;
    Ok(body.access_token)
}

/// "2026-11-14T10:00:00+00:00" → local time.
pub fn parse_time(iso: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|t| t.with_timezone(&chrono::Local))
}

/// "within the last hour" / "5 h ago" / "yesterday" / "3 days ago" (SessionsCard.tsx).
pub fn ago(iso: &str) -> String {
    let Some(t) = parse_time(iso) else { return String::new() };
    let min = (chrono::Local::now() - t).num_minutes().max(0);
    match min {
        m if m < 60 => "within the last hour".into(),
        m if m < 60 * 24 => format!("{} h ago", m / 60),
        m if m / (60 * 24) == 1 => "yesterday".into(),
        m => format!("{} days ago", m / (60 * 24)),
    }
}
