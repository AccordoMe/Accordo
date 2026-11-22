// SPDX-License-Identifier: GPL-3.0-or-later
//! Cheques (src/api/cheques.ts, and the lists of queries.ts).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::{Api, ApiResult};

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChequeCreateRequest {
    pub recipient_identifier: String,
    pub detail: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ChequeRead {
    pub id: i64,
    pub cheque_owner_identifier: String,
    pub recipient_identifier: String,
    pub detail: HashMap<String, Value>,
    pub status: String,
    pub created_at: String,
}

/// Cheques written to you.
pub async fn get_cheques(api: Api) -> ApiResult<Vec<ChequeRead>> {
    api.get("/api/cheques").await
}

/// Incoming cheques as the list views read them (bare or `{data: [...]}`; queries.ts).
pub async fn incoming(api: Api) -> ApiResult<Vec<Value>> {
    api.get_list("/api/cheques").await
}

/// Cheques you wrote (queries.ts myCheques).
pub async fn mine(api: Api) -> ApiResult<Vec<Value>> {
    api.get_list("/api/cheques/my").await
}

pub async fn generate_evm(api: Api, data: ChequeCreateRequest) -> ApiResult<Value> {
    api.post("/api/cheques/evm", &data).await
}

pub async fn generate_sol(api: Api, data: ChequeCreateRequest) -> ApiResult<Value> {
    api.post("/api/cheques/sol", &data).await
}

pub async fn generate_ton(api: Api, data: ChequeCreateRequest) -> ApiResult<Value> {
    api.post("/api/cheques/ton", &data).await
}

pub async fn generate_tron(api: Api, data: ChequeCreateRequest) -> ApiResult<Value> {
    api.post("/api/cheques/tron", &data).await
}

/// One cheque by its 32-byte id (owner or a recipient only; the server says 404 otherwise).
pub async fn get(api: Api, cheque_id: String) -> ApiResult<Value> {
    api.get_data(&format!("/api/cheques/{}", super::client::enc(cheque_id.trim()))).await
}

/// Its claims and refunds, newest first.
pub async fn claims(api: Api, cheque_id: String) -> ApiResult<Vec<Value>> {
    api.get_list(&format!("/api/cheques/{}/claims", super::client::enc(cheque_id.trim()))).await
}

// ── after an on-chain step (WriteChequeCard / ChequeDetailsCard / taskCheques.ts) ──

/// Saves a cheque that is live on-chain (evm::cheques::cheque_record builds the body). Retries while
/// the backend hasn't seen the transaction yet.
pub async fn record(api: Api, body: Value) -> ApiResult<Value> {
    api.record_on_chain_tx("/api/cheques", &body).await
}

/// Records a claim (`slot`: the recipient's address or "slotN").
pub async fn record_claim(api: Api, cheque_id: String, slot: String, recipient_address: String, tx_hash: String, claimant_identifier: String) -> ApiResult<Value> {
    let body = serde_json::json!({"slot": slot, "recipient_address": recipient_address, "tx_hash": tx_hash, "claimant_identifier": claimant_identifier});
    api.record_on_chain_tx(&format!("/api/cheques/{}/claim", super::client::enc(cheque_id.trim())), &body).await
}

pub async fn record_refund(api: Api, cheque_id: String, tx_hash: String) -> ApiResult<Value> {
    api.record_on_chain_tx(&format!("/api/cheques/{}/refund", super::client::enc(cheque_id.trim())), &serde_json::json!({"tx_hash": tx_hash})).await
}

/// AccordoTask, gasless: Accordo submits the signed claim and pays the gas. `deadline` as a decimal string.
pub async fn relay(api: Api, chain_name: String, cheque_id: String, recipient: String, signature: String, deadline: String) -> ApiResult<Value> {
    let body = serde_json::json!({"chain_name": chain_name, "cheque_id": cheque_id, "recipient": recipient, "relayer_fee": 0, "deadline": deadline, "signature": signature});
    api.post("/api/cheques/relay", &body).await
}

/// AccordoTask: a logged-in recipient of a cheque whose key Accordo holds — one click, no gas.
pub async fn claim_for_me(api: Api, cheque_id: String, recipient: String) -> ApiResult<Value> {
    api.post(&format!("/api/cheques/{}/claim-for-me", super::client::enc(cheque_id.trim())), &serde_json::json!({"recipient": recipient})).await
}

/// AccordoTask: the claim key Accordo keeps for this cheque, if you may have it.
pub async fn claim_key(api: Api, cheque_id: String) -> ApiResult<Option<String>> {
    let v: Value = api.get(&format!("/api/cheques/{}/claim-key", super::client::enc(cheque_id.trim()))).await?;
    Ok(super::client::unwrap_data(v).get("claim_key").and_then(Value::as_str).map(str::to_string))
}
