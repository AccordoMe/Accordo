// SPDX-License-Identifier: GPL-3.0-or-later
//! Tasks (src/api/tasks.ts): posted work paid from an escrowed cheque, with review and disputes.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::client::{enc, Api, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Open,
    Taken,
    Funded,
    Submitted,
    Disputed,
    Paid,
    RefundAwarded,
    Refunded,
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PosterStats {
    pub paid: i64,
    pub refunded: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HistoryEntry {
    pub at: i64,
    pub by: String,
    pub action: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub chain: String,
    pub token_address: Option<String>,
    pub token_symbol: String,
    pub token_decimals: u8,
    pub reward: String,
    pub reward_raw: String,
    /// Accordo fee on the payout, chosen when posting; None on older tasks (contract default).
    pub fee_bps: Option<i64>,
    pub deadline_at: i64,
    /// "public" | "direct"
    pub visibility: String,
    pub assignee_username: Option<String>,
    pub status: TaskStatus,
    pub poster: Option<String>,
    pub poster_stats: PosterStats,
    pub executor: Option<String>,
    pub executor_address: Option<String>,
    pub cheque_id: Option<String>,
    pub funder_address: Option<String>,
    pub expires_at: Option<i64>,
    pub fund_tx: Option<String>,
    pub paid_tx: Option<String>,
    pub refund_tx: Option<String>,
    pub submission: Option<String>,
    pub submitted_at: Option<i64>,
    /// The poster's review window: no answer by then = paid automatically.
    pub review_until: Option<i64>,
    pub change_requests: i64,
    pub max_change_requests: i64,
    pub review_comment: Option<String>,
    /// "poster" | "executor"
    pub dispute_by: Option<String>,
    pub dispute_reason: Option<String>,
    pub dispute_opened_at: Option<i64>,
    /// "executor" | "poster"
    pub resolution: Option<String>,
    pub resolution_note: Option<String>,
    pub history: Vec<HistoryEntry>,
    /// "poster" | "executor"
    pub role: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    Board,
    Posted,
    Working,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NewTask {
    pub title: String,
    pub description: String,
    pub chain: String,
    pub token_address: Option<String>,
    pub reward: String,
    pub deadline_at: i64,
    /// "public" | "direct"
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_username: Option<Option<String>>,
    pub fee_bps: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Arbiter {
    pub address: String,
    pub review_seconds: i64,
    pub buffer_seconds: i64,
}

fn task(id: i64, action: &str) -> String {
    format!("/api/tasks/{id}/{action}")
}

pub async fn list(api: Api, scope: Scope) -> ApiResult<Vec<Task>> {
    let scope = match scope {
        Scope::Board => "board",
        Scope::Posted => "posted",
        Scope::Working => "working",
    };
    api.get(&format!("/api/tasks?scope={scope}")).await
}

pub async fn get(api: Api, id: i64) -> ApiResult<Task> {
    api.get(&format!("/api/tasks/{id}")).await
}

pub async fn create(api: Api, body: NewTask) -> ApiResult<Task> {
    api.post("/api/tasks", &body).await
}

pub async fn take(api: Api, id: i64, address: String) -> ApiResult<Task> {
    api.post(&task(id, "take"), &json!({ "address": address })).await
}

pub async fn drop_task(api: Api, id: i64) -> ApiResult<Task> {
    api.post_empty(&task(id, "drop")).await
}

pub async fn cancel(api: Api, id: i64) -> ApiResult<Task> {
    api.post_empty(&task(id, "cancel")).await
}

pub async fn fund(api: Api, id: i64, tx_hash: String, cheque_id: String) -> ApiResult<Task> {
    api.post(&task(id, "fund"), &json!({ "tx_hash": tx_hash, "cheque_id": cheque_id })).await
}

pub async fn submit(api: Api, id: i64, text: String) -> ApiResult<Task> {
    api.post(&task(id, "submit"), &json!({ "text": text })).await
}

pub async fn reject(api: Api, id: i64, comment: String) -> ApiResult<Task> {
    api.post(&task(id, "reject"), &json!({ "comment": comment })).await
}

pub async fn approve(api: Api, id: i64) -> ApiResult<Task> {
    api.post_empty(&task(id, "approve")).await
}

pub async fn release(api: Api, id: i64) -> ApiResult<Task> {
    api.post_empty(&task(id, "release")).await
}

pub async fn dispute(api: Api, id: i64, reason: String) -> ApiResult<Task> {
    api.post(&task(id, "dispute"), &json!({ "reason": reason })).await
}

pub async fn arbiter(api: Api, chain: String) -> ApiResult<Arbiter> {
    api.get(&format!("/api/tasks/arbiter?chain={}", enc(&chain))).await
}

/// Admin: disputes to decide ("open") or all of them.
pub async fn disputes(api: Api, open_only: bool) -> ApiResult<Vec<Task>> {
    api.get(&format!("/api/tasks/admin/disputes?state={}", if open_only { "open" } else { "all" })).await
}

/// Admin: who wins a dispute — "executor" or "poster".
pub async fn resolve(api: Api, id: i64, winner: String, note: String) -> ApiResult<Task> {
    api.post(&task(id, "resolve"), &json!({ "winner": winner, "note": note })).await
}

pub async fn refund(api: Api, id: i64, tx_hash: String) -> ApiResult<Task> {
    api.post(&task(id, "refund"), &json!({ "tx_hash": tx_hash })).await
}
