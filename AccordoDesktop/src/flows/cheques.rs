// SPDX-License-Identifier: GPL-3.0-or-later
//! Writing a cheque (WriteChequeCard.tsx → handleCreateCheque, Accordo wallet path): the review
//! window's lines, then InitCheque (an approve first for a token) and POST /api/cheques. If the
//! backend can't take the record, it's kept on this computer — the claim secret is in it and
//! exists nowhere else.
//!
//! A swap (Draft::swap) is InitSwapCheque on the same contract: the creator locks one token and asks
//! for another; the Accordo fee is taken from what they get. Same review window, same saving.

use alloy_primitives::{Address, U256};
use iced::futures::channel::mpsc;

use super::failed;
use crate::wallets::short_address;
use crate::api::client::Api;
use crate::api::data::Chain;
use crate::components::tx_review::{Failed, Finished, Item, Review, TxMsg};
use crate::evm::cheques::{self as onchain, ChequeRecord};
use crate::evm::client::{format_units, token_or_native, Wallet};
use crate::pages::cheques_page::Draft;
use crate::store::unsaved;

/// Everything the transaction needs, fixed when the review window opens.
#[derive(Clone)]
pub struct Job {
    pub draft: Draft,
    pub chain: Chain,
    pub rpc_urls: Vec<String>,
    pub contract: Address,
    pub wallet_address: String,
    pub wallet_vault: crate::store::app_store::WalletVault,
    pub explorer: Option<String>,
    /// The account's username (cheque_owner_identificator).
    pub owner: String,
}

fn bps_percent(bps: u16) -> String {
    let p = f64::from(bps) / 100.0;
    format!("{p}")
}

fn when(expires_at: u64) -> String {
    if expires_at == 0 {
        "Never".to_string()
    } else {
        chrono::DateTime::from_timestamp(expires_at as i64, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %d, %Y, %H:%M").to_string()).unwrap_or_default()
    }
}

/// A swap's review: what you lock, what you get after the fee, and what the other side pays and gets.
fn swap_review(job: &Job, ask: &crate::pages::cheques_page::SwapAsk, wallet_name: &str) -> Review {
    let d = &job.draft;
    let (give, get) = (d.token.symbol.as_str(), ask.token.symbol.as_str());
    let (fee, net) = onchain::split_fee(ask.total, d.fee_bps);
    let native = token_or_native(Some(&d.token.address)).map(|a| a == Address::ZERO).unwrap_or(true);
    let (name, who) = d.recipients.first().cloned().unwrap_or_default();
    let mut items = vec![
        Item::strong("You lock", format!("{} {give}", d.amount)),
        Item::row("You ask for", format!("{} {get}", ask.amount)),
        Item::row(format!("Accordo fee, from what you get ({}%)", bps_percent(d.fee_bps)), format!("{} {get}", format_units(fee, ask.token.decimals))),
        Item::strong("You receive", format!("{} {get}", format_units(net, ask.token.decimals))),
        Item::row("Expires", when(d.expires_at)),
        Item::Section("The other side"),
        Item::row("They pay", format!("{} {get}", ask.amount)),
        Item::row("They get", format!("{} {give}", d.amount)),
    ];
    let label = if name.is_empty() { "Who".to_string() } else { name };
    if who == Address::ZERO {
        items.push(Item::row(label, "Anyone with the link"));
    } else {
        items.push(Item::address(label, who.to_checksum(None)));
    }
    items.push(Item::Note("Both sides move in one transaction when they take it: they can't get yours without paying. Until then you can take it back once it expires.".into()));
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{wallet_name} · {}", short_address(&job.wallet_address))));
    items.push(Item::row("Network", job.chain.name.clone()));
    if !native {
        items.push(Item::Note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {} {give}, then the swap itself.", d.amount)));
    }
    if !d.note.trim().is_empty() {
        items.push(Item::row("Note", d.note.trim().to_string()));
    }
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo cheques", job.contract.to_checksum(None)));
    if !native {
        items.push(Item::address(format!("{give} token"), d.token.address.clone()));
    }
    if token_or_native(Some(&ask.token.address)).is_ok_and(|a| a != Address::ZERO) {
        items.push(Item::address(format!("{get} token"), ask.token.address.clone()));
    }
    Review {
        title: "Create swap".into(),
        subtitle: format!("{} {give} for {} {get}", d.amount, ask.amount),
        confirm: "Create",
        items,
        approve: !native,
        record: true,
        done_title: "Swap created",
        gasless: false,
    }
}

pub fn review(job: &Job, wallet_name: &str) -> Review {
    if let Some(ask) = &job.draft.swap {
        return swap_review(job, ask, wallet_name);
    }
    let d = &job.draft;
    let sym = d.token.symbol.as_str();
    let n = d.recipients.len().max(1);
    let (fee, net) = onchain::split_fee(d.total, d.fee_bps);
    let native = token_or_native(Some(&d.token.address)).map(|a| a == Address::ZERO).unwrap_or(true);
    let expires = when(d.expires_at);
    let mut items = vec![
        Item::strong("You lock", format!("{} {sym}", d.amount)),
        Item::row(format!("Accordo fee, taken on claim ({}%)", bps_percent(d.fee_bps)), format!("{} {sym}", format_units(fee, d.token.decimals))),
        Item::row(if n == 1 { "Recipient gets" } else { "Each recipient gets" }, format!("{} {sym}", format_units(net / U256::from(n), d.token.decimals))),
        Item::row("Expires", expires),
        Item::Section("Recipients"),
    ];
    for (i, (name, addr)) in d.recipients.iter().enumerate() {
        let label = if name.is_empty() { format!("#{}", i + 1) } else { name.clone() };
        if *addr == Address::ZERO {
            items.push(Item::row(label, "Open slot — any wallet"));
        } else {
            items.push(Item::address(label, addr.to_checksum(None)));
        }
    }
    items.push(Item::Section("Your side"));
    items.push(Item::row("From wallet", format!("{wallet_name} · {}", short_address(&job.wallet_address))));
    items.push(Item::row("Network", job.chain.name.clone()));
    if !native {
        items.push(Item::Note(format!("If this wallet hasn't given the permission yet, it's two transactions: first a permission for exactly {} {sym}, then the cheque itself.", d.amount)));
    }
    if !d.note.trim().is_empty() {
        items.push(Item::row("Note", d.note.trim().to_string()));
    }
    items.push(Item::Section("Contracts"));
    items.push(Item::address("Accordo cheques", job.contract.to_checksum(None)));
    if !native {
        items.push(Item::address(format!("{sym} token"), d.token.address.clone()));
    }
    Review {
        title: "Create cheque".into(),
        subtitle: if n == 1 { "For 1 recipient".into() } else { format!("For {n} recipients, split equally") },
        confirm: "Create",
        items,
        approve: !native,
        record: true,
        done_title: "Cheque created",
        gasless: false,
    }
}

/// Locks the money, then saves the record (kept locally when Accordo can't take it).
pub async fn run(job: Job, wallet: Wallet, api: Api, out: mpsc::UnboundedSender<TxMsg>) -> Result<Finished, Failed> {
    let d = &job.draft;
    let token = token_or_native(Some(&d.token.address)).map_err(failed(None))?;
    let recipients: Vec<Address> = d.recipients.iter().map(|r| r.1).collect();
    let (secret, id) = onchain::new_v4();
    let (hash, body) = if let Some(ask) = &d.swap {
        let terms = onchain::SwapTerms {
            receiver: recipients.first().copied().unwrap_or(Address::ZERO),
            give_token: token,
            give: d.total,
            get_token: token_or_native(Some(&ask.token.address)).map_err(failed(None))?,
            get: ask.total,
        };
        let hash = onchain::create_swap(&wallet, job.contract, id, &terms, d.expires_at, d.fee_bps).await.map_err(failed(None))?;
        let body = onchain::swap_record(&onchain::SwapRecord {
            recipient_identifiers: &d.identifiers,
            owner_username: &job.owner,
            secret,
            id,
            tx_hash: hash,
            contract: job.contract,
            from: wallet.address(),
            terms,
            give_amount: &d.amount,
            give_symbol: &d.token.symbol,
            get_amount: &ask.amount,
            get_symbol: &ask.token.symbol,
            expires_at: d.expires_at,
            note: &d.note,
            explorer: job.explorer.as_deref(),
            chain_name: &job.chain.name,
            fee_bps: d.fee_bps,
        });
        (hash, body)
    } else {
        let hash = onchain::create(&wallet, job.contract, id, recipients.clone(), d.total, token, d.expires_at, d.fee_bps).await.map_err(failed(None))?;
        (hash, cheque_body(&job, &wallet, secret, id, hash, &recipients))
    };
    let _ = out.unbounded_send(TxMsg::Saving(hash));

    let cheque_id = Some(format!("{id:#x}"));
    let server = api.base().to_string();
    let what = if d.swap.is_some() { "swap" } else { "cheque" };
    match crate::api::cheques::record(api, body.clone()).await {
        Ok(_) => Ok(Finished { hash, saved: true, warning: None, id: cheque_id }),
        Err(_) => match unsaved::keep("cheque", "/api/cheques", body, Some(format!("{hash:#x}")), &job.owner, &server) {
            Ok(()) => Ok(Finished { hash, saved: false, warning: None, id: cheque_id }),
            Err(e) => Err(Failed { message: format!("The {what} is live, but Accordo couldn't save it and it couldn't be kept on this computer either ({e})."), hash: Some(hash) }),
        },
    }
}

/// POST /api/cheques for a plain cheque.
fn cheque_body(job: &Job, wallet: &Wallet, secret: alloy_primitives::B256, id: alloy_primitives::B256, hash: alloy_primitives::B256, recipients: &[Address]) -> serde_json::Value {
    let d = &job.draft;
    let net = format_units(onchain::split_fee(d.total, d.fee_bps).1, d.token.decimals);
    onchain::cheque_record(&ChequeRecord {
        recipient_identifiers: &d.identifiers,
        owner_username: &job.owner,
        secret,
        id,
        tx_hash: hash,
        contract: job.contract,
        from: wallet.address(),
        recipients,
        amount: &d.amount,
        net_amount: &net,
        expires_at: d.expires_at,
        token_symbol: &d.token.symbol,
        note: &d.note,
        explorer: job.explorer.as_deref(),
        chain_name: &job.chain.name,
        fee_bps: d.fee_bps,
        room_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::data::Token;

    fn job(token: &str) -> Job {
        Job {
            draft: Draft {
                token: Token { symbol: "USDC".into(), name: "USD Coin".into(), address: token.into(), decimals: 6, logo: None },
                recipients: vec![("bob".into(), Address::ZERO), (String::new(), Address::repeat_byte(0x11))],
                identifiers: vec!["bob".into()],
                amount: "10".into(),
                total: U256::from(10_000_000u64),
                fee_bps: 50,
                note: String::new(),
                expires_at: 0,
                swap: None,
            },
            chain: Chain { id: "8453".into(), name: "Base".into(), testnet: false, native_symbol: "ETH".into(), explorer: None, rpc_urls: vec![], local: false, solana: false, contracts: Default::default() },
            rpc_urls: vec![],
            contract: Address::repeat_byte(0x22),
            wallet_address: "0x3333333333333333333333333333333333333333".into(),
            wallet_vault: crate::store::app_store::WalletVault::Text(String::new()),
            explorer: None,
            owner: "zuko".into(),
        }
    }

    fn value<'a>(r: &'a Review, label: &str) -> Option<&'a str> {
        r.items.iter().find_map(|i| match i {
            Item::Row { label: l, value, .. } if l == label => Some(value.as_str()),
            Item::Address { label: l, address } if l == label => Some(address.as_str()),
            _ => None,
        })
    }

    #[test]
    fn review_lines() {
        let r = review(&job("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"), "Main");
        assert_eq!(r.subtitle, "For 2 recipients, split equally");
        assert_eq!(value(&r, "You lock"), Some("10 USDC"));
        assert_eq!(value(&r, "Accordo fee, taken on claim (0.5%)"), Some("0.05 USDC"));
        assert_eq!(value(&r, "Each recipient gets"), Some("4.975 USDC"));
        assert_eq!(value(&r, "Expires"), Some("Never"));
        assert_eq!(value(&r, "bob"), Some("Open slot — any wallet"));
        assert_eq!(value(&r, "#2"), Some("0x1111111111111111111111111111111111111111"));
        assert!(r.approve);
        assert!(r.items.iter().any(|i| matches!(i, Item::Note(_))));

        let native = review(&job("0x0000000000000000000000000000000000000000"), "Main");
        assert!(!native.approve && !native.items.iter().any(|i| matches!(i, Item::Note(_))));
    }

    #[test]
    fn swap_review_lines() {
        let mut j = job("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
        j.draft.recipients = vec![("bob".into(), Address::ZERO)];
        j.draft.swap = Some(crate::pages::cheques_page::SwapAsk {
            token: Token { symbol: "ETH".into(), name: "Ether".into(), address: "0x0000000000000000000000000000000000000000".into(), decimals: 18, logo: None },
            amount: "0.004".into(),
            total: U256::from(4_000_000_000_000_000u64),
        });
        let r = review(&j, "Main");
        assert_eq!(r.subtitle, "10 USDC for 0.004 ETH");
        assert_eq!(value(&r, "You lock"), Some("10 USDC"));
        // The fee comes out of what the creator gets, in the other token.
        assert_eq!(value(&r, "Accordo fee, from what you get (0.5%)"), Some("0.00002 ETH"));
        assert_eq!(value(&r, "You receive"), Some("0.00398 ETH"));
        assert_eq!((value(&r, "They pay"), value(&r, "They get")), (Some("0.004 ETH"), Some("10 USDC")));
        assert_eq!(value(&r, "bob"), Some("Anyone with the link"));
        assert!(r.approve, "locking USDC needs its permission first");
    }
}

/// The whole write-cheque path on a scratch Anvil, with Accordo unreachable: the cheque goes
/// on-chain, the record is kept on this computer, and the secret in it really claims the cheque.
///   XDG_CONFIG_HOME=<scratch> ACCORDO_TEST_RPC=http://127.0.0.1:8546 ACCORDO_TEST_DEPLOYMENT=…/deployment.json \
///   cargo test -- --ignored live_write_cheque --nocapture
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn live_write_cheque() {
    use crate::evm::client::{address, Rpc, Stage};
    let (Ok(url), Ok(dep), Ok(cfg)) = (std::env::var("ACCORDO_TEST_RPC"), std::env::var("ACCORDO_TEST_DEPLOYMENT"), std::env::var("XDG_CONFIG_HOME")) else {
        eprintln!("set ACCORDO_TEST_RPC, ACCORDO_TEST_DEPLOYMENT and XDG_CONFIG_HOME (a scratch folder)");
        return;
    };
    assert!(cfg.contains("scratchpad") || cfg.starts_with("/tmp"), "XDG_CONFIG_HOME must be a scratch folder");
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dep).unwrap()).unwrap();
    let contract = address(d["chain"]["contracts"]["AccordoCheques"].as_str().unwrap()).unwrap();
    let usdc = d["tokens"].as_array().unwrap().iter().find(|t| t["symbol"] == "USDC").unwrap().clone();
    let rpc = Rpc::new(vec![url.clone()], Some(31337)).unwrap();
    let alice_key = "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a"; // Anvil #2
    let bob = Wallet::new("0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6", rpc.clone()).unwrap(); // Anvil #3
    let alice_addr = Wallet::new(alice_key, rpc.clone()).unwrap().address().to_checksum(None);
    crate::chain_rpc::drip(url.clone(), false, alice_addr.clone(), crate::chain_rpc::Drip::Token { symbol: "USDC".into(), address: usdc["address"].as_str().unwrap().into(), decimals: 6 }).await.unwrap();

    let before = unsaved::pending("livetest", "http://127.0.0.1:9");
    let token = crate::api::data::Token { symbol: "USDC".into(), name: "USD Coin".into(), address: usdc["address"].as_str().unwrap().into(), decimals: 6, logo: None };
    let job = Job {
        draft: Draft { token, recipients: vec![("bob".into(), bob.address())], identifiers: vec!["bob".into()], amount: "2.5".into(), total: U256::from(2_500_000u64), fee_bps: 50, note: "live test".into(), expires_at: 0, swap: None },
        chain: Chain { id: "31337".into(), name: "Anvil".into(), testnet: false, native_symbol: "ETH".into(), explorer: None, rpc_urls: vec![url.clone()], local: true, solana: false, contracts: Default::default() },
        rpc_urls: vec![url.clone()],
        contract,
        wallet_address: alice_addr.clone(),
        wallet_vault: crate::store::app_store::WalletVault::Text(String::new()),
        explorer: None,
        owner: "livetest".into(),
    };
    let stages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = stages.clone();
    let progress: crate::evm::client::Progress = std::sync::Arc::new(move |s, _| seen.lock().unwrap().push(s));
    let alice = Wallet::new(alice_key, rpc.clone()).unwrap().with_progress(Some(progress));
    let (out, _rx) = mpsc::unbounded();
    // Nothing listens on port 9: the record can't be saved.
    let api = Api::new("http://127.0.0.1:9", Some("x"));
    let done = run(job, alice, api, out).await.unwrap();
    assert!(!done.saved);
    let st = stages.lock().unwrap().clone();
    assert_eq!(st.last(), Some(&Stage::Confirming));
    assert!(st.contains(&Stage::Signing));
    eprintln!("stages: {st:?}");

    assert_eq!(unsaved::pending("livetest", "http://127.0.0.1:9"), before + 1);
    let rec = unsaved::list().into_iter().rev().find(|r| r.account == "livetest").unwrap();
    assert_eq!(rec.tx_hash.as_deref(), Some(format!("{:#x}", done.hash).as_str()));
    let secret: alloy_primitives::B256 = rec.payload["cheque_secret"].as_str().unwrap().parse().unwrap();
    let id: alloy_primitives::B256 = rec.payload["cheque_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(alloy_primitives::keccak256(secret), id);
    assert_eq!(onchain::info(&rpc, contract, id).await.unwrap().to, vec![bob.address()]);
    onchain::claim(&bob, contract, secret, false, U256::ZERO).await.unwrap();
    assert_eq!(onchain::info(&rpc, contract, id).await.unwrap().claimed, vec![true]);
    eprintln!("written, kept locally, claimed with the kept secret ✓");
}

/// A swap end to end on a scratch Anvil, Accordo unreachable: Alice locks USDC asking ETH from Bob,
/// the record is kept on this computer, and Bob takes it through the claim flow (review read from the
/// contract, then CashOutSwapCheque paying the ETH) — both sides end up with what they were promised.
///   XDG_CONFIG_HOME=<scratch> ACCORDO_TEST_RPC=http://127.0.0.1:8546 ACCORDO_TEST_DEPLOYMENT=…/deployment.json \
///   cargo test -- --ignored live_swap --nocapture
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn live_swap() {
    use crate::evm::client::{address, Rpc};
    let (Ok(url), Ok(dep), Ok(cfg)) = (std::env::var("ACCORDO_TEST_RPC"), std::env::var("ACCORDO_TEST_DEPLOYMENT"), std::env::var("XDG_CONFIG_HOME")) else {
        eprintln!("set ACCORDO_TEST_RPC, ACCORDO_TEST_DEPLOYMENT and XDG_CONFIG_HOME (a scratch folder)");
        return;
    };
    assert!(cfg.contains("scratchpad") || cfg.starts_with("/tmp"), "XDG_CONFIG_HOME must be a scratch folder");
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dep).unwrap()).unwrap();
    let contract = address(d["chain"]["contracts"]["AccordoCheques"].as_str().unwrap()).unwrap();
    let usdc = d["tokens"].as_array().unwrap().iter().find(|t| t["symbol"] == "USDC").unwrap().clone();
    let usdc_addr = address(usdc["address"].as_str().unwrap()).unwrap();
    let rpc = Rpc::new(vec![url.clone()], Some(31337)).unwrap();
    let alice_key = "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a"; // Anvil #2
    let bob_key = "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6"; // Anvil #3
    let alice = Wallet::new(alice_key, rpc.clone()).unwrap();
    let alice_addr = alice.address();
    let bob_addr = Wallet::new(bob_key, rpc.clone()).unwrap().address();
    crate::chain_rpc::drip(url.clone(), false, alice_addr.to_checksum(None), crate::chain_rpc::Drip::Token { symbol: "USDC".into(), address: usdc["address"].as_str().unwrap().into(), decimals: 6 }).await.unwrap();

    let chain = Chain { id: "31337".into(), name: "Anvil".into(), testnet: false, native_symbol: "ETH".into(), explorer: None, rpc_urls: vec![url.clone()], local: true, solana: false, contracts: Default::default() };
    let token = crate::api::data::Token { symbol: "USDC".into(), name: "USD Coin".into(), address: usdc["address"].as_str().unwrap().into(), decimals: 6, logo: None };
    let eth = crate::api::data::Token { symbol: "ETH".into(), name: "Ether".into(), address: format!("{:#x}", Address::ZERO), decimals: 18, logo: None };
    let price = U256::from(10_000_000_000_000_000u64); // 0.01 ETH
    let job = Job {
        draft: Draft {
            token,
            recipients: vec![("bob".into(), bob_addr)],
            identifiers: vec!["bob".into()],
            amount: "2.5".into(),
            total: U256::from(2_500_000u64),
            fee_bps: 50,
            note: "live swap".into(),
            expires_at: 0,
            swap: Some(crate::pages::cheques_page::SwapAsk { token: eth, amount: "0.01".into(), total: price }),
        },
        chain: chain.clone(),
        rpc_urls: vec![url.clone()],
        contract,
        wallet_address: alice_addr.to_checksum(None),
        wallet_vault: crate::store::app_store::WalletVault::Text(String::new()),
        explorer: None,
        owner: "liveswap".into(),
    };
    let (out, _rx) = mpsc::unbounded();
    let api = Api::new("http://127.0.0.1:9", Some("x")); // nothing listens: kept locally
    let done = run(job, alice, api.clone(), out.clone()).await.unwrap();
    assert!(!done.saved);
    let rec = unsaved::list().into_iter().rev().find(|r| r.account == "liveswap").unwrap();
    let mut cheque = rec.payload.clone();
    cheque["contract_address"] = serde_json::json!(contract.to_checksum(None));
    assert_eq!(cheque["cheque_type"], "SwapCheque");

    let bob_usdc = crate::evm::balances::spendable(&rpc, bob_addr, usdc_addr).await.unwrap();
    let alice_eth = rpc.balance(alice_addr).await.unwrap();
    let me = crate::flows::claim::MyWallet { address: bob_addr.to_checksum(None), name: "Bob".into(), vault: crate::store::app_store::WalletVault::Text(String::new()) };
    let ask = crate::flows::claim::Ask { cheque, recipient: None, chain, rpc_urls: vec![url.clone()], explorer: None, wallets: vec![me], active: None, username: "bob".into() };
    let ready = crate::flows::claim::prepare(ask).await.unwrap();
    let lines: Vec<String> = ready.review.items.iter().filter_map(|i| match i { Item::Row { label, value, .. } => Some(format!("{label}: {value}")), _ => None }).collect();
    eprintln!("take review: {lines:?}");
    assert!(lines.contains(&"You get: 2.5 USDC".to_string()) && lines.contains(&"You pay: 0.01 ETH".to_string()));
    let crate::flows::Job::Claim(c) = ready.job else { panic!("not a claim") };
    let crate::flows::claim::Job::Cheques(take) = *c else { panic!("not an AccordoCheques claim") };
    crate::flows::claim::run_cheques(*take, Wallet::new(bob_key, rpc.clone()).unwrap(), api, out).await.unwrap();

    assert_eq!(crate::evm::balances::spendable(&rpc, bob_addr, usdc_addr).await.unwrap() - bob_usdc, U256::from(2_500_000u64));
    // Alice gets the price less the 0.5% fee (and spent no gas meanwhile).
    assert_eq!(rpc.balance(alice_addr).await.unwrap() - alice_eth, price - price * U256::from(50u8) / U256::from(10_000u32));
    eprintln!("swap locked, kept locally, taken: both sides paid ✓");
}
