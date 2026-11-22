// SPDX-License-Identifier: GPL-3.0-or-later
//! Crowdfunding (src/api/crowdfund.ts).
//!
//! ACTIVE → ENDED (deadline passed, the creator decides) → CLAIMED | REFUNDING. REFUNDING also
//! happens on its own if the creator doesn't decide within 30 days.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{query, Api, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CampaignStatus {
    #[default]
    Active,
    Ended,
    Claimed,
    Refunding,
    /// A status this client doesn't know yet.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CampaignRead {
    pub id: i64,
    pub campaign_id: String,
    pub creator_username: String,
    pub creator_address: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub category: String,
    pub cover_image: Option<String>,
    pub chain_name: String,
    pub chain_id: Option<i64>,
    /// The contract it was created on (None: from before Accordo kept it).
    #[serde(default)]
    pub contract_address: Option<String>,
    pub token_symbol: String,
    pub token_address: Option<String>,
    pub token_decimals: u8,
    pub target_amount: f64,
    pub raised_amount: f64,
    pub net_raised_amount: f64,
    pub claimed_amount: f64,
    pub refunded_amount: f64,
    pub contributors_count: i64,
    pub fee_bps: i64,
    /// Target missed at the deadline → refunds only, the creator can't withdraw.
    pub is_all_or_nothing: Option<bool>,
    /// Current contract: the fee is taken from withdrawals and refunds are whole. Older campaigns
    /// paid it from every contribution.
    pub fee_on_withdrawal: Option<bool>,
    /// Gross raised has reached the target (the creator may end the campaign early).
    pub goal_reached: Option<bool>,
    pub status: CampaignStatus,
    /// Hidden by moderators: out of Explore, and Accordo won't take contributions for it.
    pub hidden: Option<bool>,
    pub hidden_reason: Option<String>,
    pub tx_hash: Option<String>,
    pub deadline: Option<String>,
    /// After this the creator can no longer claim, and backers can refund.
    pub decision_deadline: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CampaignContributionRead {
    pub id: i64,
    pub campaign_id: String,
    pub contributor_username: Option<String>,
    pub contributor_address: String,
    pub gross_amount: f64,
    pub fee_paid: f64,
    pub net_amount: f64,
    pub message: Option<String>,
    pub tx_hash: Option<String>,
    pub created_at: String,
}

/// Only presentation comes from the client; money, token, deadline and fee are read from the tx.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CampaignCreatePayload {
    pub tx_hash: String,
    pub chain_name: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_symbol: Option<String>,
    /// Crowdfunding Terms version the creator agreed to (must be the current one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_terms: Option<String>,
}

/// scam | impersonation | prohibited | misleading | spam | other
pub type CampaignReportReason = String;

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub search: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub chain_name: Option<String>,
    pub sort_by: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MyCampaigns {
    pub created: Vec<CampaignRead>,
    pub backed: Vec<CampaignRead>,
    pub contributions: Vec<CampaignContributionRead>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CampaignDetail {
    pub campaign: CampaignRead,
    pub contributions: Vec<CampaignContributionRead>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SyncResult {
    pub campaign: CampaignRead,
    pub applied: Vec<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReportReply {
    pub status: String,
    pub id: i64,
}

pub async fn report(api: Api, campaign_id: String, reason: CampaignReportReason, comment: Option<String>) -> ApiResult<ReportReply> {
    let mut body = json!({ "reason": reason });
    if let Some(c) = comment {
        body["comment"] = json!(c);
    }
    api.post_data(&format!("/api/crowdfund/{campaign_id}/report"), Some(&body)).await
}

pub async fn list(api: Api, filter: ListFilter) -> ApiResult<Vec<CampaignRead>> {
    let non_empty = |v: Option<String>| v.filter(|s| !s.is_empty());
    let q = query(&[
        ("search", non_empty(filter.search)),
        ("category", non_empty(filter.category)),
        ("status", non_empty(filter.status)),
        ("chain_name", non_empty(filter.chain_name)),
        ("sort_by", non_empty(filter.sort_by)),
    ]);
    api.get_data(&format!("/api/crowdfund{q}")).await
}

/// Live campaigns (queries.ts activeCampaigns).
pub async fn active(api: Api) -> ApiResult<Vec<CampaignRead>> {
    list(api, ListFilter { status: Some("ACTIVE".into()), ..Default::default() }).await
}

pub async fn get_my(api: Api) -> ApiResult<MyCampaigns> {
    api.get_data("/api/crowdfund/my").await
}

pub async fn get(api: Api, campaign_id: String) -> ApiResult<CampaignDetail> {
    api.get_data(&format!("/api/crowdfund/{campaign_id}")).await
}

/// Registers a campaign from its creation tx; retried while the backend's node catches up.
pub async fn create(api: Api, payload: CampaignCreatePayload) -> ApiResult<CampaignRead> {
    api.record_on_chain_tx("/api/crowdfund", &payload).await
}

/// Indexes a contribution / claim / cancel / refund tx. The backend reads what happened from the chain.
pub async fn sync(api: Api, campaign_id: String, tx_hash: String) -> ApiResult<SyncResult> {
    api.record_on_chain_tx(&format!("/api/crowdfund/{campaign_id}/sync"), &json!({ "tx_hash": tx_hash })).await
}
