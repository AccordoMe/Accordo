// SPDX-License-Identifier: GPL-3.0-or-later
//! Invoices: issuing one (IssueInvoiceCard.handleIssueInvoice, Accordo wallet path), splitting a
//! bill between several payers (issueSplit) and paying one (InvoiceDetailsCard.handlePayInvoice +
//! paymentReview.ts).
//!
//! Issuing moves no money: createInvoice with a random id and a 6-digit PIN (its hash on-chain),
//! then POST /api/invoices — kept on this computer if Accordo can't take it (the PIN is in it).
//! A split is one createInvoices transaction (an invoice per payer, each with its own PIN), then
//! one record per share, all with the same split id.
//! Paying reads the invoice from the contract first: the amount, token, merchant and fee shown
//! are the contract's, not the record's; then payETH / payERC20 (an approve first) and
//! /api/invoices/{id}/pay.

use alloy_primitives::{Address, B256, U256};
use iced::futures::channel::mpsc;
use serde_json::{json, Value};

use super::claim::MyWallet;
use super::{failed, Ready};
use crate::api::client::Api;
use crate::api::data::{Chain, Token};
use crate::components::tx_review::{Failed, Finished, Item, Review, TxMsg};
use crate::evm::client::{address, format_units, token_or_native, Rpc, Wallet};
use crate::evm::invoices::{self as hub, NewInvoice};
use crate::store::unsaved;
use crate::wallets::short_address;

/// A checked invoice form.
#[derive(Debug, Clone)]
pub struct Draft {
    pub merchant: Address,
    pub token: Token,
    /// Who it's billed to (username / email; "" = anyone with the link).
    pub client: String,
    /// Only this wallet may pay; ZERO = any.
    pub payer: Address,
    pub amount: String,
    pub units: u128,
    pub fee_bps: u16,
    pub note: String,
    /// 0 = no due date.
    pub due_at: u64,
    pub allow_redact: bool,
    /// A bill split: one invoice per share, created together (empty: one invoice, as above).
    /// Then `amount` / `units` are the total, and `client` / `payer` aren't used.
    pub split: Vec<Share>,
}

/// One payer's share of a split bill: their own invoice.
#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    /// Who it's billed to (username / email / ID).
    pub client: String,
    /// Only this wallet may pay; ZERO = any.
    pub payer: Address,
    pub units: u128,
    /// `units` in the token's decimals.
    pub amount: String,
}

#[derive(Clone)]
pub struct CreateJob {
    pub draft: Draft,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub wallet: MyWallet,
    pub owner: String,
}

/// Six digits, from the system's randomness (the web's generateUniqueSecretPin gives six too).
pub fn new_pin() -> String {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).expect("system randomness");
    format!("{:06}", u32::from_le_bytes(b) % 1_000_000)
}

fn when(ts: u64) -> String {
    if ts == 0 {
        return "No due date".into();
    }
    chrono::DateTime::from_timestamp(ts as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
}

fn pct(bps: u16) -> String {
    format!("{}", f64::from(bps) / 100.0)
}

/// A random UUID (v4), like the web's crypto.randomUUID(): the id every share of a split carries.
fn new_split_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("system randomness");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The window for a split: the total, who pays what, then as for one invoice.
fn split_review(job: &CreateJob) -> Review {
    let d = &job.draft;
    let sym = d.token.symbol.as_str();
    let native = token_or_native(Some(&d.token.address)).map(|a| a == Address::ZERO).unwrap_or(true);
    let mut items = vec![Item::strong("Total", format!("{} {sym}", d.amount)), Item::Section("Who pays what")];
    for s in &d.split {
        let who = if s.client.trim().is_empty() { s.payer.to_checksum(None) } else { s.client.trim().to_string() };
        items.push(Item::row(who, format!("{} {sym}", s.amount)));
    }
    items.push(Item::row("Accordo fee, from each payment", format!("{}%", pct(d.fee_bps))));
    items.push(Item::row("Due", when(d.due_at)));
    items.push(Item::address("Paid to", d.merchant.to_checksum(None)));
    if !d.note.trim().is_empty() {
        items.push(Item::row("Note", d.note.trim().to_string()));
    }
    items.push(Item::note("Each payer gets their own invoice for their share, payable only from the wallet shown (none: anyone). Creating them moves no money — only the network fee."));
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{} · {}", job.wallet.name, short_address(&job.wallet.address))));
    items.push(Item::row("Network", job.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo invoices", job.contract.to_checksum(None)));
    if !native {
        items.push(Item::address(format!("{sym} token"), d.token.address.clone()));
    }
    Review { title: "Split the bill".into(), subtitle: format!("{} invoices in one transaction", d.split.len()), confirm: "Create", items, approve: false, record: true, done_title: "Invoices created", gasless: false }
}

pub fn create_review(job: &CreateJob) -> Review {
    if !job.draft.split.is_empty() {
        return split_review(job);
    }
    let d = &job.draft;
    let sym = d.token.symbol.as_str();
    let total = U256::from(d.units);
    let fee = total * U256::from(d.fee_bps) / U256::from(10_000u32);
    let native = token_or_native(Some(&d.token.address)).map(|a| a == Address::ZERO).unwrap_or(true);
    let mut items = vec![
        Item::strong("You ask for", format!("{} {sym}", d.amount)),
        Item::row(format!("Accordo fee, from the payment ({}%)", pct(d.fee_bps)), format!("{} {sym}", format_units(fee, d.token.decimals))),
        Item::row("You receive", format!("{} {sym}", format_units(total - fee, d.token.decimals))),
        Item::row("Billed to", if d.client.trim().is_empty() { "Anyone with the link".to_string() } else { d.client.trim().to_string() }),
    ];
    if d.payer != Address::ZERO {
        items.push(Item::address("Only this wallet may pay", d.payer.to_checksum(None)));
    }
    items.push(Item::row("Due", when(d.due_at)));
    items.push(Item::address("Paid to", d.merchant.to_checksum(None)));
    if !d.note.trim().is_empty() {
        items.push(Item::row("Note", d.note.trim().to_string()));
    }
    items.push(Item::note("Creating the invoice moves no money — only the network fee."));
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{} · {}", job.wallet.name, short_address(&job.wallet.address))));
    items.push(Item::row("Network", job.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo invoices", job.contract.to_checksum(None)));
    if !native {
        items.push(Item::address(format!("{sym} token"), d.token.address.clone()));
    }
    Review { title: "Create invoice".into(), subtitle: format!("{} {sym}", d.amount), confirm: "Create", items, approve: false, record: true, done_title: "Invoice created", gasless: false }
}

/// What one recorded invoice is: the whole draft, or one share of a split.
struct One<'a> {
    client: &'a str,
    payer: Address,
    units: u128,
    amount: &'a str,
    /// (split id, its index, how many shares, the total).
    split: Option<(&'a str, usize, usize, &'a str)>,
}

impl<'a> One<'a> {
    fn whole(d: &'a Draft) -> Self {
        Self { client: &d.client, payer: d.payer, units: d.units, amount: &d.amount, split: None }
    }
}

/// POST /api/invoices for one invoice, as IssueInvoiceCard sends it.
fn record(job: &CreateJob, one: &One, id: B256, pin: &str, hash: B256, token: Address) -> Value {
    let d = &job.draft;
    let tx = format!("{hash:#x}");
    let due_iso = (d.due_at > 0).then(|| chrono::DateTime::from_timestamp(d.due_at as i64, 0).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))).flatten();
    let claim = crate::evm::cheques::call_json(&hub::pay_call(job.contract, id, token, U256::from(one.units), pin));
    let mentioned: Vec<String> = one.client.split('\n').map(|a| a.trim().trim_start_matches('@').to_string()).filter(|a| !a.is_empty()).collect();
    let mut detail = json!({
        "invoiceId": format!("{id:#x}"),
        "amount": one.amount,
        // A share is a fixed amount: never redacted down.
        "allow_redact": d.allow_redact && one.split.is_none(),
        "token": d.token.symbol,
        "token_address": d.token.address,
        "token_decimals": d.token.decimals,
        "merchant": d.merchant.to_checksum(None),
        "note": d.note,
        "has_pin": true,
        "secret_code": pin,
        "claim": claim,
        "chain": job.chain.name,
        "fee_percent": f64::from(d.fee_bps) / 100.0,
        "webhookUrl": "N/A",
        "mentioned_users": mentioned,
        "tx_hash": tx,
    });
    if d.due_at > 0 {
        detail["due_at_timestamp"] = json!(d.due_at);
    }
    let mut body = json!({
        "invoice_id": format!("{id:#x}"),
        "tx_hash": tx,
        "creator_identificator": job.owner,
        "payer_identifier": if one.client.trim().is_empty() { "Open Client".to_string() } else { one.client.trim().to_string() },
        "payer_address": one.payer.to_checksum(None),
        "due_date": due_iso,
        "detail": detail,
        "status": "UNPAID",
    });
    if let Some((split_id, index, count, total)) = one.split {
        body["split_id"] = json!(split_id);
        body["detail"]["split"] = json!({ "id": split_id, "index": index, "count": count, "total": total });
    }
    body
}

pub async fn run_create(job: CreateJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    if !job.draft.split.is_empty() {
        return run_split(job, wallet, api, out).await;
    }
    let d = &job.draft;
    let token = token_or_native(Some(&d.token.address)).map_err(failed(None))?;
    let id = hub::new_id();
    let pin = new_pin();
    let inv = NewInvoice { id, merchant: d.merchant, token, amount: d.units, due_at: d.due_at, fee_bps: d.fee_bps, payer: d.payer, code: pin.clone() };
    let hash = hub::create(&wallet, job.contract, &inv).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let body = record(&job, &One::whole(d), id, &pin, hash, token);
    let server = api.base().to_string();
    match crate::api::invoices::record(api, body.clone()).await {
        Ok(_) => Ok(Finished { hash, saved: true, warning: None, id: None }),
        Err(_) => match unsaved::keep("invoice", "/api/invoices", body, Some(format!("{hash:#x}")), &job.owner, &server) {
            Ok(()) => Ok(Finished { hash, saved: false, warning: None, id: None }),
            Err(e) => Err(Failed { message: format!("The invoice is live, but Accordo couldn't save it and it couldn't be kept on this computer either ({e})."), hash: Some(hash) }),
        },
    }
}

/// Every share in one createInvoices, then a record for each (kept on this computer if Accordo
/// can't take it, as for one invoice).
async fn run_split(job: CreateJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let d = &job.draft;
    let token = token_or_native(Some(&d.token.address)).map_err(failed(None))?;
    // Older InvoiceHub deployments create one at a time.
    let rpc = Rpc::new(job.rpc_urls.clone(), job.chain.evm_id()).map_err(|e| Failed { message: e.to_string(), hash: None })?;
    if !hub::supports_batch(&rpc, job.contract).await {
        return Err(Failed { message: format!("Splitting a bill needs the updated invoice contract, and the one on {} is older. Issue separate invoices for now.", job.chain.name), hash: None });
    }
    let split_id = new_split_id();
    let shares: Vec<(B256, String, &Share)> = d.split.iter().map(|s| (hub::new_id(), new_pin(), s)).collect();
    let items: Vec<NewInvoice> = shares.iter().map(|(id, pin, s)| NewInvoice { id: *id, merchant: d.merchant, token, amount: s.units, due_at: d.due_at, fee_bps: d.fee_bps, payer: s.payer, code: pin.clone() }).collect();
    let hash = hub::create_batch(&wallet, job.contract, &items).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let server = api.base().to_string();
    let (mut kept, mut lost) = (0usize, None);
    for (index, (id, pin, s)) in shares.iter().enumerate() {
        let one = One { client: &s.client, payer: s.payer, units: s.units, amount: &s.amount, split: Some((&split_id, index, shares.len(), &d.amount)) };
        let body = record(&job, &one, *id, pin, hash, token);
        if crate::api::invoices::record(api.clone(), body.clone()).await.is_ok() {
            continue;
        }
        match unsaved::keep("invoice", "/api/invoices", body, Some(format!("{hash:#x}")), &job.owner, &server) {
            Ok(()) => kept += 1,
            Err(e) => lost = Some(e),
        }
    }
    if let Some(e) = lost {
        return Err(Failed { message: format!("The split is live, but some shares couldn't be saved to Accordo or kept on this computer ({e})."), hash: Some(hash) });
    }
    Ok(Finished { hash, saved: kept == 0, warning: None, id: None })
}

// ── paying ──

pub struct PayAsk {
    pub invoice: Value,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub explorer: Option<String>,
    pub wallets: Vec<MyWallet>,
    pub active: Option<String>,
}

#[derive(Clone)]
pub struct PayJob {
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub id: B256,
    pub token: Address,
    pub amount: U256,
    pub code: String,
    pub wallet: MyWallet,
}

fn s<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(str::trim).filter(|x| !x.is_empty())
}

/// The invoice's contract on this network (older deployments call it AccordoInvoices).
pub fn contract_of(chain: &Chain) -> Option<Address> {
    chain.contract("AccordoInvoice").or_else(|| chain.contract("AccordoInvoices")).and_then(|a| address(a).ok()).filter(|a| *a != Address::ZERO)
}

pub async fn prepare_pay(ask: PayAsk) -> Result<Ready, String> {
    let v = &ask.invoice;
    let id: B256 = s(v, &["invoice_id"]).or_else(|| s(v, &["detail", "invoiceId"])).and_then(|x| x.parse().ok()).ok_or("This invoice was never created on-chain, so it can't be paid through Accordo.")?;
    let contract = super::own_contract(s(v, &["contract_address"])).or_else(|| contract_of(&ask.chain)).ok_or_else(|| format!("Accordo invoices aren't on {}", ask.chain.name))?;
    let rpc = Rpc::new(ask.rpc_urls.clone(), ask.chain.evm_id()).map_err(|e| e.to_string())?;
    let inv = hub::get(&rpc, contract, id).await.map_err(|e| format!("Couldn't read the invoice on-chain: {e}"))?;
    if inv.merchant == Address::ZERO {
        return Err(format!("This invoice isn't on {} — is the right network picked?", ask.chain.name));
    }
    if inv.canceled {
        return Err("This invoice was cancelled".into());
    }
    let (settled, overdue) = hub::status(&inv, chrono::Utc::now().timestamp().max(0) as u64);
    if settled {
        return Err("This invoice is paid already".into());
    }
    let left = U256::from(inv.amount - inv.paid.min(inv.amount));

    // The wallet: the one the invoice is locked to, or the active one.
    let wallet = if inv.payer != Address::ZERO {
        ask.wallets.iter().find(|w| address(&w.address).ok() == Some(inv.payer)).cloned().ok_or_else(|| format!("Only the wallet {} may pay this invoice, and it isn't one of yours", short_address(&inv.payer.to_checksum(None))))?
    } else {
        ask.active.as_deref().and_then(|a| ask.wallets.iter().find(|w| w.address.eq_ignore_ascii_case(a))).or_else(|| ask.wallets.first()).cloned().ok_or("Select an EVM wallet to pay with")?
    };

    // The PIN, when the invoice has one: the record gives it to the person billed.
    let code = s(v, &["detail", "secret_code"]).or_else(|| s(v, &["secret_code"])).unwrap_or("").to_string();
    if !hub::code_matches(&code, inv.codeHash) {
        return Err("This invoice needs its PIN, and it isn't shared with you — ask the merchant for the payment link".into());
    }

    let (symbol, decimals) = if inv.token == Address::ZERO {
        (ask.chain.native_symbol.clone(), 18)
    } else {
        crate::evm::balances::token_info(&rpc, inv.token).await.map(|(_, sym, d)| (sym, d)).map_err(|e| format!("Couldn't read the token: {e}"))?
    };
    let fee = left * U256::from(inv.feeBps) / U256::from(10_000u32);
    let merchant_name = s(v, &["invoice_owner_identifier"]).or_else(|| s(v, &["creator_identificator"]));
    let item = crate::components::finance::records::invoice_item(v).map(str::to_string);
    let mut items = vec![
        Item::strong("You pay", format!("{} {symbol}", format_units(left, decimals))),
        Item::row(format!("Accordo fee, included ({}%)", pct(inv.feeBps)), format!("{} {symbol}", format_units(fee, decimals))),
        Item::row("Merchant receives", format!("{} {symbol}", format_units(left - fee, decimals))),
        Item::row("Paid to", match merchant_name {
            Some(n) => format!("{n} · {}", short_address(&inv.merchant.to_checksum(None))),
            None => short_address(&inv.merchant.to_checksum(None)),
        }),
        Item::row("Due", if overdue { format!("{} (overdue)", when(inv.dueAt)) } else { when(inv.dueAt) }),
    ];
    if inv.paid > 0 {
        items.push(Item::row("Paid already", format!("{} {symbol}", format_units(U256::from(inv.paid), decimals))));
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{} · {}", wallet.name, short_address(&wallet.address))));
    items.push(Item::row("Network", ask.chain.name.clone()));
    if inv.token != Address::ZERO {
        items.push(Item::note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {} {symbol}, then the payment.", format_units(left, decimals))));
    }
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo invoices", contract.to_checksum(None)));
    if inv.token != Address::ZERO {
        items.push(Item::address(format!("{symbol} token"), inv.token.to_checksum(None)));
    }
    let review = Review {
        title: "Pay invoice".into(),
        subtitle: item.map(|i| format!("For {i}")).unwrap_or_else(|| format!("{} {symbol}", format_units(left, decimals))),
        confirm: "Pay",
        items,
        approve: inv.token != Address::ZERO,
        record: true,
        done_title: "Invoice paid",
        gasless: false,
    };
    let job = PayJob { chain: ask.chain, rpc_urls: ask.rpc_urls, contract, id, token: inv.token, amount: left, code, wallet };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::PayInvoice(Box::new(job)) })
}

pub async fn run_pay(job: PayJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let hash = hub::pay(&wallet, job.contract, job.id, job.token, job.amount, &job.code).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    // The money has moved; only Accordo's confirmation can still fail — that's not a failed payment.
    let warning = crate::api::invoices::record_payment(api, format!("{:#x}", job.id), format!("{hash:#x}"))
        .await
        .err()
        .map(|e| format!("Paid on-chain, but Accordo couldn't confirm it yet: {}", e.message));
    Ok(Finished { hash, saved: true, warning, id: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_are_six_digits() {
        for _ in 0..50 {
            let p = new_pin();
            assert_eq!(p.len(), 6);
            assert!(p.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn record_like_the_web() {
        let job = CreateJob {
            draft: Draft {
                merchant: Address::repeat_byte(0x11),
                token: Token { symbol: "USDC".into(), name: "USD Coin".into(), address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into(), decimals: 6, logo: None },
                client: "@zuko\nbob".into(),
                payer: Address::ZERO,
                amount: "12.5".into(),
                units: 12_500_000,
                fee_bps: 50,
                note: "logo".into(),
                due_at: 0,
                allow_redact: false,
                split: vec![],
            },
            chain: Chain { id: "8453".into(), name: "Base".into(), testnet: false, native_symbol: "ETH".into(), explorer: None, rpc_urls: vec![], local: false, solana: false, contracts: Default::default() },
            rpc_urls: vec![],
            contract: Address::repeat_byte(0x22),
            wallet: MyWallet { address: "0x1111111111111111111111111111111111111111".into(), name: "Main".into(), vault: crate::store::app_store::WalletVault::Text(String::new()) },
            owner: "aang".into(),
        };
        let token = token_or_native(Some(&job.draft.token.address)).unwrap();
        let r = record(&job, &One::whole(&job.draft), B256::repeat_byte(3), "123456", B256::repeat_byte(4), token);
        assert_eq!(r["payer_identifier"], "@zuko\nbob");
        assert_eq!(r["detail"]["mentioned_users"], json!(["zuko", "bob"]));
        assert_eq!(r["detail"]["secret_code"], "123456");
        assert_eq!(r["due_date"], Value::Null);
        assert!(r["detail"].get("due_at_timestamp").is_none());
        assert_eq!(r["detail"]["claim"]["to"], Address::repeat_byte(0x22).to_checksum(None));
        assert_eq!(r["status"], "UNPAID");
        let rv = create_review(&job);
        assert!(rv.items.iter().any(|i| matches!(i, Item::Row { label, value, .. } if label == "You receive" && value == "12.4375 USDC")));
        assert!(r.get("split_id").is_none());

        // A share of a split: its own payer and amount, the split's id and place, never redacted.
        let mut job = job;
        job.draft.allow_redact = true;
        job.draft.split = vec![
            Share { client: "@zuko".into(), payer: Address::repeat_byte(0x33), units: 6_250_000, amount: "6.25".into() },
            Share { client: "bob".into(), payer: Address::ZERO, units: 6_250_000, amount: "6.25".into() },
        ];
        let s = &job.draft.split[0];
        let one = One { client: &s.client, payer: s.payer, units: s.units, amount: &s.amount, split: Some(("abcd-1234", 0, 2, "12.5")) };
        let r = record(&job, &one, B256::repeat_byte(3), "123456", B256::repeat_byte(4), token);
        assert_eq!((r["split_id"].as_str(), r["payer_identifier"].as_str()), (Some("abcd-1234"), Some("@zuko")));
        assert_eq!(r["payer_address"], Address::repeat_byte(0x33).to_checksum(None));
        assert_eq!(r["detail"]["split"], json!({ "id": "abcd-1234", "index": 0, "count": 2, "total": "12.5" }));
        assert_eq!((r["detail"]["amount"].as_str(), r["detail"]["allow_redact"].as_bool()), (Some("6.25"), Some(false)));
        let rv = create_review(&job);
        assert_eq!((rv.title.as_str(), rv.subtitle.as_str()), ("Split the bill", "2 invoices in one transaction"));
        let id = new_split_id();
        assert!(id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }
}
