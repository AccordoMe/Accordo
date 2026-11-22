// SPDX-License-Identifier: GPL-3.0-or-later
//! Drops at Accordo (the dropsApi of utils/drops.ts): registering a drop created on-chain (the
//! backend checks the list hashes to the contract's root and builds the proofs), the owner's
//! list, the public view, a wallet's slots and their proofs, and syncing claims from the chain.

#![allow(dead_code)] // the Drops page uses it once creating a drop is wired

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{enc, Api, ApiResult};

/// One wallet of a new drop (amounts in the token's smallest units, as a decimal string).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRecipient {
    pub address: String,
    pub amount_wei: String,
    pub start: u64,
    pub interval: u32,
    pub periods: u16,
    pub cliff: u32,
    pub upfront_bps: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct DropCreate {
    pub chain: String,
    pub drop_id: String,
    pub tx_hash: String,
    pub recipients: Vec<NewRecipient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The owner view (/my, create); the public view leaves out owner, counts, claimed and fees.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DropInfo {
    pub drop_id: String,
    pub chain: String,
    /// The contract it was created on (None: from before Accordo kept it).
    #[serde(default)]
    pub contract_address: Option<String>,
    pub owner: Option<String>,
    pub creator_address: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub token: String,
    pub token_address: Option<String>,
    pub token_decimals: u8,
    pub total: String,
    pub total_wei: String,
    pub claimed: Option<String>,
    pub claimed_wei: Option<String>,
    pub recipients_count: Option<u64>,
    pub claimed_count: Option<u64>,
    pub fee_bps: u16,
    pub creation_fee: Option<String>,
    pub expires_at: u64,
    pub vesting_end: u64,
    pub refunded: bool,
    pub refund_tx: Option<String>,
    /// active | completed | expired | refunded
    pub status: String,
    pub merkle_root: String,
    pub creation_tx: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SlotSchedule {
    pub start: u64,
    pub interval: u32,
    pub periods: u16,
    pub cliff: u32,
    pub upfront_bps: u16,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DropSlot {
    pub slot: u32,
    pub address: String,
    pub amount: String,
    pub amount_wei: String,
    pub net_amount: String,
    pub schedule: SlotSchedule,
    pub unlocked: String,
    pub claimed_amount: String,
    pub claimed_wei: String,
    pub claimable: String,
    pub claimable_wei: String,
    pub next_unlock_at: Option<u64>,
    pub vesting_end: u64,
    pub claimed: bool,
    pub claim_tx: Option<String>,
    pub claimed_at: Option<String>,
    /// The Merkle proof (lookup only).
    pub proof: Option<Vec<String>>,
    /// Owner view only.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SlotPage {
    pub total: u64,
    pub items: Vec<DropSlot>,
}

/// Registers a drop that is live on-chain (retries while the backend hasn't seen the tx yet).
pub async fn create(api: Api, body: DropCreate) -> ApiResult<DropInfo> {
    let v: Value = api.record_on_chain_tx("/api/drops", &body).await?;
    serde_json::from_value(v.get("drop").cloned().unwrap_or(v)).map_err(|e| super::client::ApiError { status: Some(200), message: format!("Unexpected answer from the server: {e}") })
}

pub async fn mine(api: Api) -> ApiResult<Vec<DropInfo>> {
    api.get_list("/api/drops/my").await
}

pub async fn get(api: Api, drop_id: String) -> ApiResult<DropInfo> {
    api.get_data(&format!("/api/drops/{}", enc(drop_id.trim()))).await
}

/// A chat card's data: the drop and — for these wallets — their shares, in one request (the
/// server takes up to 20 addresses with the drop; more, or a server from before it did, are looked
/// up after). No wallets: no shares (None).
pub async fn card(api: Api, drop_id: String, addresses: Vec<String>) -> ApiResult<(DropInfo, Option<Vec<DropSlot>>)> {
    let id = drop_id.trim().to_string();
    if addresses.is_empty() {
        return Ok((get(api, id).await?, None));
    }
    let bad = |e: serde_json::Error| super::client::ApiError { status: Some(200), message: format!("Unexpected answer from the server: {e}") };
    let first = &addresses[..addresses.len().min(20)];
    let qs: Vec<String> = first.iter().map(|a| format!("address={}", enc(a))).collect();
    let mut v: Value = api.get(&format!("/api/drops/{}?{}", enc(&id), qs.join("&"))).await?;
    let shares = v.as_object_mut().and_then(|o| o.remove("slots"));
    let info: DropInfo = serde_json::from_value(v).map_err(bad)?;
    let mut slots: Vec<DropSlot> = match shares {
        Some(s) => serde_json::from_value(s).map_err(bad)?,
        None => lookup(api.clone(), id.clone(), first.to_vec()).await?,
    };
    for chunk in addresses.chunks(20).skip(1) {
        slots.extend(lookup(api.clone(), id.clone(), chunk.to_vec()).await?);
    }
    Ok((info, Some(slots)))
}

/// The slots (with proofs) of these wallets — up to 20 at a time.
pub async fn lookup(api: Api, drop_id: String, addresses: Vec<String>) -> ApiResult<Vec<DropSlot>> {
    let qs: Vec<String> = addresses.iter().take(20).map(|a| format!("address={}", enc(a))).collect();
    let v: Value = api.get(&format!("/api/drops/{}/lookup?{}", enc(drop_id.trim()), qs.join("&"))).await?;
    serde_json::from_value(v.get("slots").cloned().unwrap_or(json!([]))).map_err(|e| super::client::ApiError { status: Some(200), message: format!("Unexpected answer from the server: {e}") })
}

/// A page of the owner's list (`q`: search, `claimed`: filter).
pub async fn slots(api: Api, drop_id: String, offset: u64, limit: u64, q: Option<String>, claimed: Option<bool>) -> ApiResult<SlotPage> {
    let mut qs = format!("offset={offset}&limit={limit}");
    if let Some(q) = q.filter(|q| !q.trim().is_empty()) {
        qs.push_str(&format!("&q={}", enc(q.trim())));
    }
    if let Some(c) = claimed {
        qs.push_str(&format!("&claimed={c}"));
    }
    api.get_data(&format!("/api/drops/{}/slots?{qs}", enc(drop_id.trim()))).await
}

/// Re-reads claims from the chain (after a claim / refund transaction, pass its hash).
pub async fn sync(api: Api, drop_id: String, tx_hash: Option<String>) -> ApiResult<Value> {
    let body = match tx_hash {
        Some(h) => json!({"tx_hash": h}),
        None => json!({}),
    };
    api.record_on_chain_tx(&format!("/api/drops/{}/sync", enc(drop_id.trim())), &body).await
}
