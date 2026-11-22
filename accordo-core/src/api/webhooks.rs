// SPDX-License-Identifier: GPL-3.0-or-later
//! Webhooks add-on (src/api/webhooks.ts): when one of your invoices is paid, Accordo POSTs it,
//! signed, to your server. Paid add-on; the server says whether it's active.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WebhookStatus {
    pub active: bool,
    pub until: Option<String>,
    pub has_secret: bool,
    pub default_url: Option<String>,
    pub events: Vec<String>,
    /// rotate_secret only: the new signing secret, returned this once.
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WebhookDelivery {
    pub id: String,
    pub event: String,
    pub url: String,
    /// pending | delivered | failed | skipped
    pub status: String,
    pub attempts: i64,
    pub last_status_code: Option<i64>,
    pub last_error: Option<String>,
    pub next_attempt_at: Option<String>,
    pub created_at: Option<String>,
    pub delivered_at: Option<String>,
    pub payload: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Deliveries {
    pub deliveries: Vec<WebhookDelivery>,
}

pub async fn status(api: Api) -> ApiResult<WebhookStatus> {
    api.get_data("/api/webhooks").await
}

/// None clears the default endpoint.
pub async fn set_default_url(api: Api, url: Option<String>) -> ApiResult<WebhookStatus> {
    api.put_data("/api/webhooks/default-url", &json!({ "url": url })).await
}

/// A new signing secret — returned this once (in `secret`).
pub async fn rotate_secret(api: Api) -> ApiResult<WebhookStatus> {
    api.post_data::<(), _>("/api/webhooks/secret", None).await
}

pub async fn deliveries(api: Api, limit: i64) -> ApiResult<Deliveries> {
    api.get_data(&format!("/api/webhooks/deliveries?limit={limit}")).await
}

pub async fn retry(api: Api, id: String) -> ApiResult<WebhookDelivery> {
    api.post_data::<(), _>(&format!("/api/webhooks/deliveries/{id}/retry"), None).await
}

/// A test event to `url`, or to the default endpoint.
pub async fn test(api: Api, url: Option<String>) -> ApiResult<WebhookDelivery> {
    api.post_data("/api/webhooks/test", Some(&json!({ "url": url.filter(|u| !u.is_empty()) }))).await
}
