// SPDX-License-Identifier: GPL-3.0-or-later
//! Drops (CreateDropCard.tsx, DropsPage.tsx): payout plans, reading a pasted list, creating a drop
//! (InitMerkleCheque + POST /api/drops, kept on this computer if Accordo can't take the list —
//! without it nobody can claim), and taking back what's left once it ended.

use alloy_primitives::{Address, B256, U256};
use iced::futures::channel::mpsc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::claim::MyWallet;
use super::{failed, Ready};
use crate::api::client::Api;
use crate::api::data::{Chain, Token};
use crate::api::drops::{DropCreate, DropInfo, NewRecipient};
use crate::components::chat::fund_cards::{ChainRef, OnChain};
use crate::components::tx_review::{Failed, Finished, Item, Review, TxMsg};
use crate::evm::client::{address, format_units, token_or_native, Rpc, Wallet};
use crate::evm::drops::{self as onchain, Recipient, Schedule, INSTANT};
use crate::store::unsaved;
use crate::wallets::short_address;

const DAY: f64 = 86_400.0;
/// Average calendar month (365.25 / 12 days): payouts stay on the same date.
pub const MONTH_DAYS: f64 = 30.4375;
pub const MAX_RECIPIENTS: usize = onchain::MAX_RECIPIENTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Week,
    #[default]
    Month,
    Quarter,
}

impl Unit {
    pub const ALL: [Unit; 3] = [Unit::Week, Unit::Month, Unit::Quarter];

    fn days(self) -> f64 {
        match self {
            Unit::Week => 7.0,
            Unit::Month => MONTH_DAYS,
            Unit::Quarter => MONTH_DAYS * 3.0,
        }
    }

    pub fn many(self) -> &'static str {
        match self {
            Unit::Week => "weeks",
            Unit::Month => "months",
            Unit::Quarter => "quarters",
        }
    }

    fn adverb(self) -> &'static str {
        match self {
            Unit::Week => "weekly",
            Unit::Month => "monthly",
            Unit::Quarter => "quarterly",
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.many())
    }
}

/// A payout plan, in words: "10% right away, the rest over 12 months, first payout after 6".
/// Kept with the app's state, reused across drops (the web keeps them in localStorage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub id: u64,
    pub name: String,
    pub payouts: u32,
    pub unit: Unit,
    pub delay_months: u32,
    pub upfront_pct: f64,
    pub window_months: u32,
}

impl Default for Plan {
    fn default() -> Self {
        Self { id: 1, name: "Everyone".into(), payouts: 12, unit: Unit::Month, delay_months: 0, upfront_pct: 0.0, window_months: 3 }
    }
}

impl Plan {
    pub fn valid(&self) -> bool {
        (1..=1000).contains(&self.payouts) && (0.0..=100.0).contains(&self.upfront_pct)
    }

    /// Everything unlocks at the start: 100% right away — the parts would pay nothing.
    pub fn all_at_once(&self) -> bool {
        self.upfront_pct >= 100.0
    }

    /// As the contract sees it: equal parts, the first at start + cliff, then every interval.
    /// 100% right away is one payout at the start: otherwise its "parts" of nothing would push
    /// the end, and with it the claim window, months out.
    pub fn schedule(&self, start: u64) -> Schedule {
        if self.all_at_once() {
            return Schedule { start, ..INSTANT };
        }
        Schedule {
            start,
            interval: (self.unit.days() * DAY).round() as u32,
            periods: self.payouts.clamp(1, 1000) as u16,
            cliff: (f64::from(self.delay_months) * MONTH_DAYS * DAY).round() as u32,
            upfront_bps: (self.upfront_pct * 100.0).round().clamp(0.0, 10_000.0) as u16,
        }
    }

    /// When its wallets can no longer claim: the last payout + the claim window.
    pub fn closes_at(&self, start: u64) -> u64 {
        self.schedule(start).end().max(start) + (f64::from(self.window_months.max(1)) * MONTH_DAYS * DAY).round() as u64
    }

    pub fn summary(&self, start: u64) -> String {
        let s = self.schedule(start);
        let date = |t: u64| chrono::DateTime::from_timestamp(t as i64, 0).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default();
        let first = s.start + u64::from(s.cliff);
        let close = format!(" · claim until {}", date(self.closes_at(start)));
        if s.periods == 1 && s.cliff == 0 {
            return format!("Everything on {}{close}", date(s.start));
        }
        let up = if s.upfront_bps > 0 { format!("{}% on {}, then ", f64::from(s.upfront_bps) / 100.0, date(s.start)) } else { String::new() };
        if s.periods == 1 {
            return format!("{up}{} on {}{close}", if up.is_empty() { "one payout" } else { "the rest" }, date(first));
        }
        format!("{up}{} {} payouts{}, {} → {}{close}", s.periods, self.unit.adverb(), if up.is_empty() { "" } else { " of the rest" }, date(first), date(s.end()))
    }
}

// ── the list ──

/// One row of the form: a wallet and its share. No name: a drop goes to wallets, not to people
/// (unlike cheques and invoices, nobody gets it by @username).
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub address: String,
    pub amount: String,
    /// None: the first plan.
    pub plan: Option<u64>,
}

impl Row {
    pub fn blank(&self) -> bool {
        self.address.trim().is_empty() && self.amount.trim().is_empty()
    }
}

/// A line read from pasted text: (address, amount, name, plan name). A name is read only so the
/// columns line up (lists made for the web have one), then skipped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub address: String,
    pub amount: String,
    pub name: String,
    pub plan: String,
}

fn is_addr(c: &str) -> bool {
    c.len() == 42 && c.starts_with("0x") && c[2..].chars().all(|x| x.is_ascii_hexdigit())
}

/// Lines → rows. Separators: comma, semicolon, tab or |. Columns by header (address/wallet,
/// amount, name/label, plan) or by position: `address, amount[, name[, plan]]` or
/// `name, address, amount[, plan]` (parseLines).
pub fn parse_lines(text: &str) -> Vec<Line> {
    let mut lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let split = |l: &str| -> Vec<String> {
        if l.contains([',', ';', '\t', '|']) { l.split([',', ';', '\t', '|']).map(|c| c.trim().to_string()).collect() } else { l.split_whitespace().map(str::to_string).collect() }
    };
    let first: Vec<String> = split(lines[0]).iter().map(|c| c.to_lowercase()).collect();
    let mut idx: Option<[Option<usize>; 4]> = None;
    if !first.iter().any(|c| is_addr(c)) && first.iter().any(|c| c.contains("address") || c.contains("wallet")) {
        let col = |words: &[&str]| first.iter().position(|h| words.iter().any(|w| h.contains(w)));
        idx = Some([col(&["address", "wallet"]), col(&["amount", "sum", "value", "tokens"]), col(&["name", "label"]), col(&["plan"])]);
        lines.remove(0);
    }
    lines
        .into_iter()
        .map(|line| {
            let c = split(line);
            let g = |i: Option<usize>| i.and_then(|i| c.get(i)).cloned().unwrap_or_default();
            let at = |i: usize| c.get(i).cloned().unwrap_or_default();
            match idx {
                Some([a, m, n, p]) => Line { address: g(a), amount: g(m), name: g(n), plan: g(p) },
                None if c.first().is_some_and(|x| is_addr(x)) => Line { address: at(0), amount: at(1), name: at(2), plan: at(3) },
                None => Line { name: at(0), address: at(1), amount: at(2), plan: at(3) },
            }
        })
        .collect()
}

fn pick<'a>(o: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    for k in keys {
        if let Some(v) = o.iter().find(|(key, _)| key.trim().eq_ignore_ascii_case(k)).map(|(_, v)| v).filter(|v| !v.is_null() && v.as_str() != Some("")) {
            return Some(v);
        }
    }
    None
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().replace(char::is_whitespace, "").replace(',', "."),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// JSON → rows: an array, `{recipients: [...]}`, one object, or a fragment copied out of an array.
pub fn parse_json(content: &str) -> Result<Vec<Line>, String> {
    let mut t = content.trim().trim_end_matches(',').trim().to_string();
    if t.starts_with('{') && !t.to_lowercase().replace(' ', "").starts_with("{\"recipients\"") {
        t = format!("[{t}]");
    }
    // Trailing commas before ] or }.
    let mut cleaned = String::with_capacity(t.len());
    let chars: Vec<char> = t.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if *ch == ',' && chars[i + 1..].iter().find(|c| !c.is_whitespace()).is_some_and(|c| *c == ']' || *c == '}') {
            continue;
        }
        cleaned.push(*ch);
    }
    let data: Value = serde_json::from_str(&cleaned).map_err(|_| "That JSON doesn't parse — check brackets and quotes".to_string())?;
    let list = match &data {
        Value::Array(a) => a.clone(),
        Value::Object(o) if o.get("recipients").is_some_and(Value::is_array) => o["recipients"].as_array().cloned().unwrap_or_default(),
        other => vec![other.clone()],
    };
    Ok(list
        .into_iter()
        .map(|r| match r {
            Value::String(s) => Line { address: s.trim().to_string(), ..Default::default() },
            Value::Object(o) => Line {
                address: pick(&o, &["address", "wallet", "addr", "to"]).and_then(Value::as_str).unwrap_or("").trim().to_string(),
                amount: text_of(pick(&o, &["amount", "sum", "value", "tokens"])),
                name: pick(&o, &["name", "label", "username"]).and_then(Value::as_str).unwrap_or("").trim().to_string(),
                plan: pick(&o, &["plan"]).and_then(Value::as_str).unwrap_or("").trim().to_string(),
            },
            _ => Line::default(),
        })
        .collect())
}

/// Pasted text: JSON if it looks like JSON, otherwise lines.
pub fn parse_list(text: &str) -> Result<Vec<Line>, String> {
    if text.trim_start().starts_with(['[', '{']) { parse_json(text) } else { Ok(parse_lines(text)) }
}

/// "Plan2", "plan 2", "PLAN_2" all name the plan "Plan 2".
pub fn norm_plan(n: &str) -> String {
    n.to_lowercase().chars().filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-' | '.')).collect()
}

/// The rows checked (collect): recipients with their schedules, the row errors by index, and
/// what's wrong overall.
pub struct Collected {
    pub recipients: Vec<Recipient>,
    /// Index into the rows of each recipient (for plans).
    pub rows: Vec<usize>,
    pub row_errors: Vec<(usize, &'static str)>,
    pub errors: Vec<String>,
}

pub fn plan_of<'a>(plans: &'a [Plan], row: &Row) -> Option<&'a Plan> {
    row.plan.and_then(|id| plans.iter().find(|p| p.id == id)).or_else(|| plans.first())
}

pub fn collect(rows: &[Row], plans: &[Plan], parts: bool, decimals: u8, start: u64) -> Collected {
    let mut out = Collected { recipients: Vec::new(), rows: Vec::new(), row_errors: Vec::new(), errors: Vec::new() };
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        if r.blank() {
            continue;
        }
        let Ok(a) = address(&r.address) else {
            out.row_errors.push((i, "Not a wallet address"));
            continue;
        };
        if a == Address::ZERO {
            out.row_errors.push((i, "The zero address can't receive a drop"));
            continue;
        }
        if !seen.insert(a) {
            out.row_errors.push((i, "Listed twice — one share per wallet"));
            continue;
        }
        let Ok(amount) = crate::evm::client::parse_units(&r.amount, decimals) else {
            out.row_errors.push((i, "Enter an amount"));
            continue;
        };
        if amount.is_zero() {
            out.row_errors.push((i, "Amount must be above zero"));
            continue;
        }
        let schedule = if parts { plan_of(plans, r).map(|p| p.schedule(start)).unwrap_or(INSTANT) } else { INSTANT };
        out.recipients.push(Recipient { address: a, amount, schedule });
        out.rows.push(i);
    }
    if !out.row_errors.is_empty() {
        out.errors.push(format!("{} row(s) need fixing", out.row_errors.len()));
    }
    if parts && plans.iter().any(|p| !p.valid()) {
        out.errors.push("Check the payout plans".into());
    }
    if out.recipients.len() > MAX_RECIPIENTS {
        out.errors.push(format!("At most {MAX_RECIPIENTS} wallets per drop"));
    }
    out
}

// ── creating ──

#[derive(Debug, Clone)]
pub struct Draft {
    pub title: String,
    pub note: String,
    pub token: Token,
    pub recipients: Vec<Recipient>,
    pub total: U256,
    pub fee_bps: u16,
    pub expires_at: u64,
}

pub struct CreateAsk {
    pub draft: Draft,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub explorer: Option<String>,
    pub wallet: MyWallet,
    pub owner: String,
}

pub struct CreateJob {
    pub draft: Draft,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub wallet: MyWallet,
    pub owner: String,
}

pub fn contract_of(chain: &Chain) -> Option<Address> {
    chain.contract("AccordoDrop").and_then(|a| address(a).ok()).filter(|a| *a != Address::ZERO)
}

fn when(ts: u64) -> String {
    chrono::DateTime::from_timestamp(ts as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
}

/// Reads the creation fee, then the review.
pub async fn prepare_create(ask: CreateAsk) -> Result<Ready, String> {
    let d = &ask.draft;
    let contract = contract_of(&ask.chain).ok_or_else(|| format!("Drops aren't available on {} yet", ask.chain.name))?;
    let rpc = Rpc::new(ask.rpc_urls.clone(), ask.chain.evm_id()).map_err(|e| e.to_string())?;
    let n = d.recipients.len() as u32;
    let fee_now = onchain::creation_fee(&rpc, contract, n).await.map_err(|e| format!("Couldn't read the drop contract: {e}"))?;
    let sym = d.token.symbol.as_str();
    let dec = d.token.decimals;
    let fmt = |v: U256| format!("{} {sym}", format_units(v, dec));
    let token = token_or_native(Some(&d.token.address)).map_err(|e| e.to_string())?;
    let claim_fee = d.recipients.iter().map(|r| r.amount * U256::from(d.fee_bps) / U256::from(10_000u32)).fold(U256::ZERO, |a, b| a + b);
    let mut items = vec![
        Item::strong("You lock", fmt(d.total)),
        Item::row("Wallets", n.to_string()),
        Item::row(format!("Accordo fee, taken on each claim ({}%)", f64::from(d.fee_bps) / 100.0), fmt(claim_fee)),
        Item::row("Creation fee", if fee_now.is_zero() { "Free".to_string() } else { format!("{} {}", format_units(fee_now, 18), ask.chain.native_symbol) }),
        Item::row("Claims close", when(d.expires_at)),
        Item::note("Whatever nobody claims by then, you can take back."),
    ];
    if token != Address::ZERO {
        items.push(Item::note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {}, then the drop itself.", fmt(d.total))));
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{} · {}", ask.wallet.name, short_address(&ask.wallet.address))));
    items.push(Item::row("Network", ask.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo drops", contract.to_checksum(None)));
    if token != Address::ZERO {
        items.push(Item::address(format!("{sym} token"), d.token.address.clone()));
    }
    let review = Review {
        title: "Create drop".into(),
        subtitle: if d.title.trim().is_empty() { format!("{n} wallets") } else { d.title.trim().to_string() },
        confirm: "Create",
        items,
        approve: token != Address::ZERO,
        record: true,
        done_title: "Drop created",
        gasless: false,
    };
    let job = CreateJob { draft: ask.draft, chain: ask.chain, rpc_urls: ask.rpc_urls, contract, wallet: ask.wallet, owner: ask.owner };
    Ok(Ready { review, explorer: ask.explorer, job: super::Job::Drop(Box::new(job)) })
}

pub async fn run_create(job: CreateJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let d = &job.draft;
    let token = token_or_native(Some(&d.token.address)).map_err(failed(None))?;
    let made = onchain::create(&wallet, job.contract, token, &d.recipients, d.expires_at, d.fee_bps).await.map_err(failed(None))?;
    let hash = made.tx_hash;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let body = DropCreate {
        chain: job.chain.name.clone(),
        drop_id: format!("{:#x}", made.drop_id),
        tx_hash: format!("{hash:#x}"),
        recipients: d
            .recipients
            .iter()
            .map(|r| NewRecipient {
                address: r.address.to_checksum(None),
                amount_wei: r.amount.to_string(),
                start: r.schedule.start,
                interval: r.schedule.interval,
                periods: r.schedule.periods,
                cliff: r.schedule.cliff,
                upfront_bps: r.schedule.upfront_bps,
            })
            .collect(),
        title: Some(d.title.trim().to_string()).filter(|t| !t.is_empty()),
        note: Some(d.note.trim().to_string()).filter(|t| !t.is_empty()),
    };
    let server = api.base().to_string();
    match crate::api::drops::create(api, body.clone()).await {
        Ok(_) => Ok(Finished { hash, saved: true, warning: None, id: None }),
        Err(_) => {
            let payload = serde_json::to_value(&body).unwrap_or_default();
            match unsaved::keep("drop", "/api/drops", payload, Some(format!("{hash:#x}")), &job.owner, &server) {
                Ok(()) => Ok(Finished { hash, saved: false, warning: None, id: None }),
                Err(e) => Err(Failed { message: format!("The drop is live, but Accordo couldn't save its list and it couldn't be kept on this computer either ({e})."), hash: Some(hash) }),
            }
        }
    }
}

// ── claiming your share ──

pub struct ClaimJob {
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub id: B256,
    pub slot: crate::api::drops::DropSlot,
    pub wallet: MyWallet,
}

/// The slot's leaf, as the contract checks it: the whole allocation and its schedule.
fn slot_call_parts(s: &crate::api::drops::DropSlot) -> Result<(U256, Schedule, Vec<B256>), String> {
    let amount: U256 = s.amount_wei.parse().map_err(|_| "This slot's amount isn't readable".to_string())?;
    let schedule = Schedule { start: s.schedule.start, interval: s.schedule.interval, periods: s.schedule.periods.max(1), cliff: s.schedule.cliff, upfront_bps: s.schedule.upfront_bps };
    let proof = s.proof.as_ref().ok_or("Accordo didn't send this slot's proof")?.iter().map(|p| p.parse::<B256>().map_err(|_| "A proof step isn't readable".to_string())).collect::<Result<Vec<_>, _>>()?;
    Ok((amount, schedule, proof))
}

// ── a drop card, from the contract ──

/// What a chat card shows, read from AccordoDrop itself: the drop (creator, token, pool, deadline,
/// fee, refunded) and, for each of the reader's shares Accordo listed, whether its proof leads to
/// the contract's root and how much the contract paid on it so far. The contract is the one the
/// record was made on (`kept`), else the network's current one.
pub async fn card_on_chain(chain: ChainRef, kept: Option<String>, drop_id: String, slots: Vec<crate::api::drops::DropSlot>) -> Result<OnChain, String> {
    use iced::futures::future::join_all;
    let contract = super::own_contract(kept.as_deref()).or(chain.drops).ok_or_else(|| format!("Drops aren't on {} in your network list", chain.name))?;
    let id: B256 = drop_id.trim().parse().map_err(|_| "This drop's id isn't an on-chain one".to_string())?;
    let rpc = Rpc::new(chain.rpc_urls.clone(), chain.chain_id).map_err(|e| e.to_string())?;
    let info = onchain::info(&rpc, contract, id).await.map_err(|e| e.to_string())?;
    if info.from == Address::ZERO {
        return Err(format!("This drop isn't on {}", chain.name));
    }
    let (mut proven, mut unproven) = (Vec::new(), Vec::new());
    for s in &slots {
        let ok = slot_call_parts(s).ok().zip(address(&s.address).ok()).is_some_and(|((amount, schedule, proof), who)| onchain::proves(info.merkleRoot, s.slot, who, amount, &schedule, &proof));
        if ok { proven.push(s.slot) } else { unproven.push(s.address.clone()) }
    }
    let token = async {
        if info.token == Address::ZERO {
            return Ok((Some(chain.native_symbol.clone()), 18u8));
        }
        let (symbol, decimals) = iced::futures::join!(rpc.read(info.token, &crate::evm::abi::IERC20::symbolCall {}), rpc.read(info.token, &crate::evm::abi::IERC20::decimalsCall {}));
        Ok::<_, String>((symbol.ok().filter(|s| !s.trim().is_empty()), decimals.map_err(|e| format!("The token didn't say its decimals: {e}"))?))
    };
    let paid = join_all(proven.iter().map(|slot| onchain::slot_claimed(&rpc, contract, id, *slot)));
    let (token, paid) = iced::futures::join!(token, paid);
    let (symbol, decimals) = token?;
    let paid = proven.into_iter().zip(paid).map(|(slot, r)| r.map(|v| (slot, v))).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    Ok(OnChain {
        creator: info.from,
        token: info.token,
        symbol,
        decimals,
        total: info.totalAmount,
        expires_at: info.expiresAt,
        refunded: info.refunded,
        fee_bps: info.feeBps,
        paid,
        unproven,
    })
}

pub fn claim_review(drop: &DropInfo, job: &ClaimJob) -> Review {
    let s = &job.slot;
    let mut items = vec![
        Item::strong("You claim now", format!("{} {}", s.claimable, drop.token)),
        Item::row(format!("Accordo fee, included ({:.2}%)", f64::from(drop.fee_bps) / 100.0), drop.token.clone()),
        Item::row("Your share", format!("{} {}", s.amount, drop.token)),
    ];
    if s.schedule.periods > 1 || s.claimed_amount.parse::<f64>().is_ok_and(|x| x > 0.0) {
        items.push(Item::row("Claimed before", format!("{} {}", s.claimed_amount, drop.token)));
    }
    if let Some(next) = s.next_unlock_at.filter(|n| *n > 0) {
        items.push(Item::row("Next part unlocks", when(next)));
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("To wallet", format!("{} · {}", job.wallet.name, short_address(&job.wallet.address))));
    items.push(Item::row("Network", job.chain.name.clone()));
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo drops", job.contract.to_checksum(None)));
    if let Some(t) = drop.token_address.as_deref().filter(|t| address(t).is_ok_and(|a| a != Address::ZERO)) {
        items.push(Item::address(format!("{} token", drop.token), t.to_string()));
    }
    Review { title: "Claim from the drop".into(), subtitle: drop.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| format!("{} drop", drop.token)), confirm: "Claim", items, approve: false, record: true, done_title: "Claimed", gasless: false }
}

pub async fn run_claim(job: ClaimJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let (amount, schedule, proof) = slot_call_parts(&job.slot).map_err(|m| Failed { message: m, hash: None })?;
    let hash = onchain::claim(&wallet, job.contract, job.id, job.slot.slot, amount, &schedule, proof).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let warning = crate::api::drops::sync(api, format!("{:#x}", job.id), Some(format!("{hash:#x}"))).await.err().map(|e| format!("Claimed on-chain, but Accordo couldn't record it yet: {}", e.message));
    Ok(Finished { hash, saved: true, warning, id: None })
}

// ── taking back the rest ──

pub struct RefundJob {
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub id: B256,
    pub wallet: MyWallet,
}

pub fn refund_review(drop: &DropInfo, job: &RefundJob) -> Review {
    let left = drop.total.parse::<f64>().unwrap_or(0.0) - drop.claimed.as_deref().and_then(|c| c.parse::<f64>().ok()).unwrap_or(0.0);
    let never = drop.recipients_count.unwrap_or(0).saturating_sub(drop.claimed_count.unwrap_or(0));
    let mut items = vec![
        Item::strong("You get back", format!("{} {}", crate::components::chat::cheque_card::amount(left), drop.token)),
        Item::row("Wallets that never claimed", never.to_string()),
        Item::note("After this, nobody can claim from the drop any more."),
        Item::Section("Your side"),
        Item::row("To wallet", format!("{} · {}", job.wallet.name, short_address(&job.wallet.address))),
        Item::row("Network", job.chain.name.clone()),
        Item::Section("Contracts"),
        Item::address("Accordo drops", job.contract.to_checksum(None)),
    ];
    if let Some(t) = drop.token_address.as_deref().filter(|t| address(t).is_ok_and(|a| a != Address::ZERO)) {
        items.push(Item::address(format!("{} token", drop.token), t.to_string()));
    }
    Review { title: "Take back the rest".into(), subtitle: drop.title.clone().unwrap_or_else(|| format!("Drop {}", short_address(&drop.drop_id))), confirm: "Refund", items, approve: false, record: true, done_title: "Refunded", gasless: false }
}

pub async fn run_refund(job: RefundJob, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let hash = onchain::refund(&wallet, job.contract, job.id).await.map_err(failed(None))?;
    let _ = out.unbounded_send(TxMsg::Saving(hash));
    let warning = crate::api::drops::sync(api, format!("{:#x}", job.id), Some(format!("{hash:#x}"))).await.err().map(|e| format!("Refunded on-chain, but Accordo couldn't record it yet: {}", e.message));
    Ok(Finished { hash, saved: true, warning, id: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0x1111111111111111111111111111111111111111";
    const B: &str = "0x2222222222222222222222222222222222222222";

    #[test]
    fn lists_like_the_web() {
        assert_eq!(parse_lines(&format!("{A}, 10, bob, Team")), vec![Line { address: A.into(), amount: "10".into(), name: "bob".into(), plan: "Team".into() }]);
        assert_eq!(parse_lines(&format!("bob {A} 5"))[0].address, A);
        let with_header = format!("name;wallet;amount\nalice;{B};2.5\n# a comment\n");
        assert_eq!(parse_lines(&with_header), vec![Line { address: B.into(), amount: "2.5".into(), name: "alice".into(), plan: String::new() }]);
        let j = parse_list(&format!("{{\"Wallet\": \"{A}\", \"amount\": 3}}, {{\"address\": \"{B}\", \"sum\": \"1,5\", \"plan\": \"x\"}},")).unwrap();
        assert_eq!(j.len(), 2);
        assert_eq!(j[0].amount, "3");
        assert_eq!(j[1].amount, "1.5");
        assert!(parse_list("[{").is_err());
        assert_eq!(norm_plan("PLAN_2"), norm_plan("plan 2"));
    }

    #[test]
    fn rows_are_checked() {
        let plans = vec![Plan::default()];
        let rows = vec![
            Row { address: A.into(), amount: "1".into(), ..Default::default() },
            Row { address: A.into(), amount: "1".into(), ..Default::default() },
            Row { address: "0x12".into(), amount: "1".into(), ..Default::default() },
            Row { address: B.into(), amount: "0".into(), ..Default::default() },
            Row::default(),
        ];
        let c = collect(&rows, &plans, true, 6, 1_000);
        assert_eq!(c.recipients.len(), 1);
        assert_eq!(c.row_errors.len(), 3);
        assert_eq!(c.recipients[0].schedule.periods, 12);
        let once = collect(&rows[..1], &plans, false, 6, 1_000);
        assert_eq!(once.recipients[0].schedule, INSTANT);
    }

    #[test]
    fn everything_right_away_is_one_payout() {
        let start = 1_000;
        let p = Plan { upfront_pct: 100.0, payouts: 12, delay_months: 6, window_months: 3, ..Plan::default() };
        let s = p.schedule(start);
        assert_eq!((s.start, s.periods, s.cliff, s.interval), (start, 1, 0, 0));
        // The claim window counts from the start, not from twelve payouts of nothing later.
        assert_eq!(p.closes_at(start), start + (3.0 * MONTH_DAYS * DAY).round() as u64);
        assert!(p.summary(start).starts_with("Everything on "), "{}", p.summary(start));
        // 10% right away still pays the rest in parts.
        let ten = Plan { upfront_pct: 10.0, ..p };
        assert_eq!(ten.schedule(start).periods, 12);
    }

    #[test]
    fn plans_end_and_close() {
        let p = Plan { delay_months: 6, payouts: 1, ..Plan::default() };
        let s = p.schedule(0);
        assert_eq!(s.end(), (6.0 * MONTH_DAYS * DAY).round() as u64);
        assert!(p.closes_at(0) > s.end());
        assert!(p.summary(0).contains("one payout"));
        assert!(Plan::default().summary(0).contains("12 monthly payouts"));
    }
}
