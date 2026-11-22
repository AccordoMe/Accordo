// SPDX-License-Identifier: GPL-3.0-or-later
//! Accordo's contracts per network (src/api/contracts.ts): addresses, ABIs, fees.

use std::collections::HashMap;

use serde::Deserialize;

use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AbiInput {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub internal_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AbiItem {
    pub name: Option<String>,
    /// function | event | constructor | error | …
    #[serde(rename = "type")]
    pub kind: String,
    /// pure | view | nonpayable | payable
    pub state_mutability: Option<String>,
    pub inputs: Option<Vec<AbiInput>>,
    pub outputs: Option<Vec<AbiInput>>,
    pub anonymous: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ContractInfo {
    pub name: String,
    pub address: String,
    pub abi: Vec<AbiItem>,
    pub bytecode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NativeCurrency {
    pub name: String,
    pub symbol: String,
    pub coingecko_id: Option<String>,
    pub decimals: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fees {
    #[serde(rename = "NATIVE_BPS")]
    pub native_bps: Option<i64>,
    #[serde(rename = "MULTI_BPS")]
    pub multi_bps: Option<i64>,
    #[serde(rename = "TOKEN_BPS")]
    pub token_bps: Option<i64>,
    #[serde(rename = "SWAP_BPS")]
    pub swap_bps: Option<i64>,
    #[serde(rename = "maxBps")]
    pub max_bps: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NetworkInfo {
    pub name: String,
    pub address: Option<String>,
    pub testnet: Option<bool>,
    pub chain_id_hex: Option<String>,
    pub chain_id_dec: Option<i64>,
    pub native_currency: Option<NativeCurrency>,
    pub rpc_urls: Option<Vec<String>>,
    pub wss_urls: Option<Vec<String>>,
    pub block_explorer_urls: Option<Vec<String>>,
    pub treasury: Option<String>,
    pub fees: Option<Fees>,
    pub tx_hash: Option<String>,
    pub explorer: Option<String>,
    pub contract_address: Option<String>,
    pub contracts: Option<HashMap<String, String>>,
    #[serde(rename = "contracts_list")]
    pub contracts_list: Option<Vec<ContractInfo>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NetworksResponse {
    pub networks: Vec<NetworkInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Abi {
    pub abi: Vec<AbiItem>,
}

pub async fn get_networks(api: Api) -> ApiResult<NetworksResponse> {
    api.get("/api/contracts/networks").await
}

pub async fn get_abi(api: Api, contract_name: String) -> ApiResult<Abi> {
    api.get(&format!("/api/contracts/abi/{contract_name}")).await
}
