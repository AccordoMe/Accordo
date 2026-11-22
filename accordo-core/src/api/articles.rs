// SPDX-License-Identifier: GPL-3.0-or-later
//! Supporter content: the hidden part of an article (src/api/articles.ts,
//! backend/app/routers/articles.py).

use serde::Deserialize;
use serde_json::{json, Value};

use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SupporterCondition {
    pub chain: String,
    pub token: Option<String>,
    pub symbol: String,
    pub min_amount: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SupporterContent {
    pub doc: Value,
    /// For its author only.
    pub condition: Option<SupporterCondition>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Unlock {
    pub unlocked: Option<bool>,
    /// "pending" while the payment isn't confirmed yet.
    pub status: Option<String>,
    pub doc: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SetContentReply {
    pub condition: SupporterCondition,
}

pub async fn set_supporter_content(api: Api, message_id: i64, doc: Value, chain: String, token: Option<String>, min_amount: String) -> ApiResult<SetContentReply> {
    api.post(&format!("/api/articles/{message_id}/supporter-content"), &json!({ "doc": doc, "chain": chain, "token": token, "min_amount": min_amount })).await
}

/// The hidden part for its author (with the condition) or someone who unlocked it; None otherwise.
pub async fn get_supporter_content(api: Api, message_id: i64) -> Option<SupporterContent> {
    api.get_data(&format!("/api/articles/{message_id}/supporter-content")).await.ok()
}

/// Unlocks with the payment txs the reader sent.
pub async fn unlock(api: Api, message_id: i64, chain: String, txs: Vec<String>) -> ApiResult<Unlock> {
    api.post_data(&format!("/api/articles/{message_id}/unlock"), Some(&json!({ "chain": chain, "txs": txs }))).await
}
