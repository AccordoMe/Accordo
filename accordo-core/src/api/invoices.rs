// SPDX-License-Identifier: GPL-3.0-or-later
//! Invoices (src/api/invoices.ts, and the lists of queries.ts).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::{enc, Api, ApiResult};

#[derive(Debug, Clone, Default, Serialize)]
pub struct InvoiceCreateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer_identifier: Option<String>,
    pub detail: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceRead {
    pub id: i64,
    pub invoice_owner_identifier: String,
    pub payer_identifier: String,
    pub detail: HashMap<String, Value>,
    pub status: String,
    pub created_at: String,
}

/// One on-chain payment as a document. Fee and net are empty when they can't be known exactly.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaymentReceipt {
    pub number: String,
    pub invoice_id: String,
    pub invoice_status: String,
    pub merchant: String,
    pub merchant_address: Option<String>,
    pub billed_to: Option<String>,
    pub payer_address: Option<String>,
    pub amount: f64,
    pub fee: Option<f64>,
    pub net: Option<f64>,
    pub amount_due: Option<f64>,
    pub total_paid: Option<f64>,
    pub token_symbol: Option<String>,
    pub token_address: Option<String>,
    pub chain_name: Option<String>,
    pub tx_hash: String,
    pub explorer_url: Option<String>,
    pub paid_at: Option<String>,
    pub item_name: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ExportDirection {
    Received,
    Paid,
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Csv,
    Json,
}

/// Invoices billed to you.
pub async fn get_invoices(api: Api) -> ApiResult<Vec<InvoiceRead>> {
    api.get("/api/invoices").await
}

/// Incoming invoices as the list views read them (queries.ts).
pub async fn incoming(api: Api) -> ApiResult<Vec<Value>> {
    api.get_list("/api/invoices").await
}

/// Invoices you issued (queries.ts myInvoices).
pub async fn mine(api: Api) -> ApiResult<Vec<Value>> {
    api.get_list("/api/invoices/my").await
}

pub async fn get_receipt(api: Api, invoice_id: String, payment_id: String) -> ApiResult<PaymentReceipt> {
    api.get(&format!("/api/invoices/{}/receipts/{}", enc(&invoice_id), enc(&payment_id))).await
}

/// The payments as a file (CSV or JSON), for accounting.
pub async fn export_payments(api: Api, direction: ExportDirection, format: ExportFormat) -> ApiResult<Vec<u8>> {
    let direction = match direction {
        ExportDirection::Received => "received",
        ExportDirection::Paid => "paid",
    };
    let format = match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    };
    api.get_bytes(&format!("/api/invoices/export?direction={direction}&format={format}")).await
}

pub async fn generate_evm(api: Api, data: InvoiceCreateRequest) -> ApiResult<Value> {
    api.post("/api/invoices/evm", &data).await
}

pub async fn generate_sol(api: Api, data: InvoiceCreateRequest) -> ApiResult<Value> {
    api.post("/api/invoices/sol", &data).await
}

pub async fn generate_ton(api: Api, data: InvoiceCreateRequest) -> ApiResult<Value> {
    api.post("/api/invoices/ton", &data).await
}

pub async fn generate_tron(api: Api, data: InvoiceCreateRequest) -> ApiResult<Value> {
    api.post("/api/invoices/tron", &data).await
}

// ── after an on-chain step (IssueInvoiceCard / PayInvoiceCard / InvoiceDetailsCard) ──

/// Saves an invoice that is live on-chain (the body IssueInvoiceCard builds).
pub async fn record(api: Api, body: Value) -> ApiResult<Value> {
    api.record_on_chain_tx("/api/invoices", &body).await
}

/// Records a payment by its transaction; the backend reads the amounts from the InvoicePaid event.
pub async fn record_payment(api: Api, invoice_id: String, tx_hash: String) -> ApiResult<Value> {
    api.record_on_chain_tx(&format!("/api/invoices/{}/pay", super::client::enc(invoice_id.trim())), &serde_json::json!({"tx_hash": tx_hash})).await
}

/// One invoice as the server keeps it (the card's live copy).
pub async fn get(api: Api, invoice_id: String) -> ApiResult<Value> {
    api.get_data(&format!("/api/invoices/{}", super::client::enc(invoice_id.trim()))).await
}

pub async fn payments(api: Api, invoice_id: String) -> ApiResult<Vec<Value>> {
    api.get_list(&format!("/api/invoices/{}/payments", super::client::enc(invoice_id.trim()))).await
}

/// Marks it cancelled at Accordo (after cancelInvoice went through on-chain).
pub async fn record_cancel(api: Api, invoice_id: String) -> ApiResult<Value> {
    api.post_empty(&format!("/api/invoices/{}/cancel", super::client::enc(invoice_id.trim()))).await
}
