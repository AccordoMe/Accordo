// SPDX-License-Identifier: GPL-3.0-or-later
//! NFT collections and drops (src/api/nft.ts).
//!
//! UPCOMING → LIVE → ENDED | SOLD_OUT; PAUSED by the creator or Accordo moderation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{enc, query, Api, ApiResult, UploadFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DropStatus {
    #[default]
    Upcoming,
    Live,
    Ended,
    SoldOut,
    Paused,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NftCollectionRead {
    pub address: String,
    pub chain_name: String,
    pub chain_id: Option<i64>,
    pub creator_username: String,
    pub creator_address: String,
    pub name: String,
    pub symbol: String,
    pub description: Option<String>,
    pub image: Option<String>,
    /// 0 = unlimited.
    pub max_supply: i64,
    pub total_minted: i64,
    pub tx_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NftDropRead {
    pub drop_id: String,
    pub collection_address: String,
    pub collection: Option<NftCollectionRead>,
    pub chain_name: String,
    pub creator_username: String,
    pub creator_address: String,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub payout_address: String,
    /// None = the native coin.
    pub currency_address: Option<String>,
    pub currency_symbol: String,
    pub currency_decimals: u8,
    /// Exact, smallest unit.
    pub price_raw: String,
    pub is_free: bool,
    pub fee_bps: i64,
    pub start_time: String,
    pub end_time: Option<String>,
    pub max_mints: i64,
    pub per_wallet: i64,
    pub minted: i64,
    pub is_allowlist: bool,
    /// Hidden from Explore: reachable by link / chat card only (not access control).
    pub unlisted: Option<bool>,
    pub allowlist_size: i64,
    pub status: DropStatus,
    pub tx_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NftMintRead {
    pub drop_id: String,
    pub minter_address: String,
    pub minter_username: Option<String>,
    pub first_token_id: String,
    pub quantity: i64,
    pub paid_raw: String,
    pub fee_raw: String,
    pub tx_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Eligibility {
    pub address: String,
    pub allowed: bool,
    pub quota: i64,
    pub proof: Vec<String>,
    pub minted: i64,
    /// None = no per-wallet limit.
    pub remaining: Option<i64>,
    pub status: DropStatus,
}

/// One allowlisted wallet and how many it may mint (utils/nftMinter.ts).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub address: String,
    pub quota: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct IpfsConfig {
    pub enabled: bool,
    pub max_files: i64,
    pub max_file_mb: f64,
    pub max_total_mb: f64,
    pub cover_max_mb: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct IpfsUpload {
    pub base_uri: String,
    pub cover: String,
    pub count: i64,
    pub images_cid: String,
    pub metadata_cid: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CoverUpload {
    /// ipfs://…
    pub uri: String,
    /// "image" | "video"
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct DropFilter {
    pub search: Option<String>,
    pub status: Option<String>,
    pub chain_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DropDetail {
    #[serde(rename = "drop")]
    pub drop_info: NftDropRead,
    pub mints: Vec<NftMintRead>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CollectionDetail {
    pub collection: NftCollectionRead,
    pub drops: Vec<NftDropRead>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MyNft {
    pub collections: Vec<NftCollectionRead>,
    pub drops: Vec<NftDropRead>,
    pub mints: Vec<NftMintRead>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CollectionRegistration {
    pub tx_hash: String,
    pub chain_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DropRegistration {
    pub tx_hash: String,
    pub chain_name: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowlist: Option<Vec<AllowlistEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlisted: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DropSync {
    #[serde(rename = "drop")]
    pub drop_info: NftDropRead,
    pub applied: Vec<Value>,
}

pub async fn ipfs_config(api: Api) -> ApiResult<IpfsConfig> {
    api.get_data("/api/nft/ipfs/config").await
}

/// One cover (image or short video) for a collection or a drop → its ipfs:// link.
pub async fn upload_cover(api: Api, file: UploadFile) -> ApiResult<CoverUpload> {
    let form = reqwest::multipart::Form::new().part("file", file.part());
    api.post_form("/api/nft/ipfs/cover", form).await
}

/// Pins the images (in this order: the first is token #0) plus generated metadata to IPFS.
pub async fn upload_collection_assets(api: Api, files: Vec<UploadFile>, name: String, description: String) -> ApiResult<IpfsUpload> {
    let mut form = reqwest::multipart::Form::new();
    for f in &files {
        form = form.part("files", f.part());
    }
    form = form.text("name", name).text("description", description);
    api.post_form("/api/nft/ipfs/collection", form).await
}

pub async fn list_drops(api: Api, filter: DropFilter) -> ApiResult<Vec<NftDropRead>> {
    let non_empty = |v: Option<String>| v.filter(|s| !s.is_empty());
    let q = query(&[("search", non_empty(filter.search)), ("status", non_empty(filter.status)), ("chain_name", non_empty(filter.chain_name))]);
    api.get_data(&format!("/api/nft/drops{q}")).await
}

pub async fn get_drop(api: Api, drop_id: String) -> ApiResult<DropDetail> {
    api.get_data(&format!("/api/nft/drops/{drop_id}")).await
}

pub async fn get_collection(api: Api, address: String) -> ApiResult<CollectionDetail> {
    api.get_data(&format!("/api/nft/collections/{address}")).await
}

pub async fn my(api: Api) -> ApiResult<MyNft> {
    api.get_data("/api/nft/my").await
}

pub async fn eligibility(api: Api, drop_id: String, address: String) -> ApiResult<Eligibility> {
    api.get_data(&format!("/api/nft/drops/{drop_id}/eligibility?address={}", enc(&address))).await
}

/// Registration reads everything from the tx; retried while the backend's node catches up.
pub async fn register_collection(api: Api, body: CollectionRegistration) -> ApiResult<NftCollectionRead> {
    api.record_on_chain_tx("/api/nft/collections", &body).await
}

pub async fn register_drop(api: Api, body: DropRegistration) -> ApiResult<NftDropRead> {
    api.record_on_chain_tx("/api/nft/drops", &body).await
}

/// Creator only: show or hide the drop in Explore.
pub async fn set_unlisted(api: Api, drop_id: String, unlisted: bool) -> ApiResult<NftDropRead> {
    api.patch_data(&format!("/api/nft/drops/{drop_id}"), &json!({ "unlisted": unlisted })).await
}

pub async fn replace_allowlist(api: Api, drop_id: String, allowlist: Vec<AllowlistEntry>) -> ApiResult<NftDropRead> {
    api.put_data(&format!("/api/nft/drops/{drop_id}/allowlist"), &json!({ "allowlist": allowlist })).await
}

pub async fn sync(api: Api, drop_id: String, tx_hash: String) -> ApiResult<DropSync> {
    api.record_on_chain_tx(&format!("/api/nft/drops/{drop_id}/sync"), &json!({ "tx_hash": tx_hash })).await
}
