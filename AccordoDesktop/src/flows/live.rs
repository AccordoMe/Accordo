// SPDX-License-Identifier: GPL-3.0-or-later
//! The flows end to end on a scratch Anvil, with Accordo unreachable (records kept locally):
//! issue an invoice → pay it from the kept record; write a cheque → claim it from the kept record.
//!   XDG_CONFIG_HOME=<scratch> ACCORDO_TEST_RPC=http://127.0.0.1:8546 ACCORDO_TEST_DEPLOYMENT=…/deployment.json \
//!   cargo test -- --ignored live_flows --nocapture

use alloy_primitives::{Address, U256};
use iced::futures::channel::mpsc;

use super::claim::MyWallet;
use crate::api::client::Api;
use crate::api::data::{Chain, Token};
use crate::evm::client::{address, Rpc, Wallet};
use crate::store::app_store::WalletVault;
use crate::store::unsaved;

// Public Anvil test accounts #2 (merchant / sender) and #3 (payer / recipient).
const ALICE: &str = "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a";
const BOB: &str = "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6";

fn my(w: &Wallet, name: &str) -> MyWallet {
    MyWallet { address: w.address().to_checksum(None), name: name.into(), vault: WalletVault::Text(String::new()) }
}

#[tokio::test]
#[ignore]
async fn live_flows() {
    let (Ok(url), Ok(dep), Ok(cfg)) = (std::env::var("ACCORDO_TEST_RPC"), std::env::var("ACCORDO_TEST_DEPLOYMENT"), std::env::var("XDG_CONFIG_HOME")) else {
        eprintln!("set ACCORDO_TEST_RPC, ACCORDO_TEST_DEPLOYMENT and XDG_CONFIG_HOME (a scratch folder)");
        return;
    };
    assert!(cfg.contains("scratchpad") || cfg.starts_with("/tmp"), "XDG_CONFIG_HOME must be a scratch folder");
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dep).unwrap()).unwrap();
    let contracts: std::collections::HashMap<String, String> = d["chain"]["contracts"].as_object().unwrap().iter().filter_map(|(k, v)| v.as_str().map(|a| (k.clone(), a.to_string()))).collect();
    let chain = Chain { id: "31337".into(), name: "Anvil".into(), testnet: false, native_symbol: "ETH".into(), explorer: None, rpc_urls: vec![url.clone()], local: true, solana: false, contracts };
    let usdc = d["tokens"].as_array().unwrap().iter().find(|t| t["symbol"] == "USDC").unwrap();
    let token = Token { symbol: "USDC".into(), name: "USD Coin".into(), address: usdc["address"].as_str().unwrap().into(), decimals: 6, logo: None };
    let rpc = Rpc::new(vec![url.clone()], Some(31337)).unwrap();
    let alice = Wallet::new(ALICE, rpc.clone()).unwrap();
    let bob = Wallet::new(BOB, rpc.clone()).unwrap();
    for w in [&alice, &bob] {
        crate::chain_rpc::drip(url.clone(), false, w.address().to_checksum(None), crate::chain_rpc::Drip::Token { symbol: "USDC".into(), address: token.address.clone(), decimals: 6 }).await.unwrap();
    }
    let api = || Api::new("http://127.0.0.1:9", Some("x"));
    let (out, _rx) = mpsc::unbounded();

    // ── invoice: alice bills bob 3 USDC, bob pays it ──
    let contract = super::invoices::contract_of(&chain).unwrap();
    let job = super::invoices::CreateJob {
        draft: super::invoices::Draft { merchant: alice.address(), token: token.clone(), client: "bob".into(), payer: Address::ZERO, amount: "3".into(), units: 3_000_000, fee_bps: 50, note: "live".into(), due_at: 0, allow_redact: false, split: vec![] },
        chain: chain.clone(),
        rpc_urls: vec![url.clone()],
        contract,
        wallet: my(&alice, "Alice"),
        owner: "livetest".into(),
    };
    let done = super::invoices::run_create(job, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
    assert!(!done.saved);
    let rec = unsaved::list().into_iter().rev().find(|r| r.account == "livetest" && r.kind == "invoice").unwrap();
    let ask = super::invoices::PayAsk { invoice: rec.payload.clone(), chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallets: vec![my(&bob, "Bob")], active: Some(bob.address().to_checksum(None)) };
    let ready = super::invoices::prepare_pay(ask).await.unwrap();
    let you_pay = ready.review.items.iter().find_map(|i| match i { crate::components::tx_review::Item::Row { label, value, .. } if label == "You pay" => Some(value.clone()), _ => None });
    assert_eq!(you_pay.as_deref(), Some("3 USDC"));
    let super::Job::PayInvoice(pay) = ready.job else { panic!("a payment") };
    let paid = super::invoices::run_pay(*pay, Wallet::new(BOB, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
    assert!(paid.warning.is_some()); // Accordo unreachable: paid, not confirmed
    let id: alloy_primitives::B256 = rec.payload["invoice_id"].as_str().unwrap().parse().unwrap();
    let inv = crate::evm::invoices::get(&rpc, contract, id).await.unwrap();
    assert!(crate::evm::invoices::status(&inv, 0).0, "settled on-chain");
    // Paying again is refused before anything is signed.
    let again = super::invoices::PayAsk { invoice: rec.payload.clone(), chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallets: vec![my(&bob, "Bob")], active: None };
    assert_eq!(super::invoices::prepare_pay(again).await.unwrap_err(), "This invoice is paid already");
    eprintln!("invoice issued, paid from its record ✓");

    // ── cheque: alice writes 2 USDC to bob, bob claims it from the kept record ──
    let cheque_contract = address(chain.contract("AccordoCheques").unwrap()).unwrap();
    let cjob = super::cheques::Job {
        draft: crate::pages::cheques_page::Draft { token: token.clone(), recipients: vec![("bob".into(), bob.address())], identifiers: vec!["bob".into()], amount: "2".into(), total: U256::from(2_000_000u64), fee_bps: 50, note: String::new(), expires_at: 0, swap: None },
        chain: chain.clone(),
        rpc_urls: vec![url.clone()],
        contract: cheque_contract,
        wallet_address: alice.address().to_checksum(None),
        wallet_vault: WalletVault::Text(String::new()),
        explorer: None,
        owner: "livetest".into(),
    };
    super::cheques::run(cjob, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
    let rec = unsaved::list().into_iter().rev().find(|r| r.account == "livetest" && r.kind == "cheque").unwrap();
    // Only the claim call is kept (no cheque_secret): the secret comes out of it.
    let mut cheque = rec.payload.clone();
    cheque.as_object_mut().unwrap().remove("cheque_secret");
    cheque["chain"] = "Anvil".into();
    let ask = super::claim::Ask { cheque: cheque.clone(), recipient: None, chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallets: vec![my(&bob, "Bob")], active: None, username: "bob".into() };
    let ready = super::claim::prepare(ask).await.unwrap();
    assert_eq!(ready.review.subtitle, "1.99 USDC to your wallet");
    let super::Job::Claim(c) = ready.job else { panic!("a claim") };
    let super::claim::Job::Cheques(job) = *c else { panic!("not an AccordoCheques claim") };
    let claimed = super::claim::run_cheques(*job, Wallet::new(BOB, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
    assert!(claimed.warning.is_some());
    // Someone who isn't a recipient can't even get to the review.
    let ask = super::claim::Ask { cheque, recipient: None, chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallets: vec![my(&alice, "Alice")], active: None, username: "alice".into() };
    let err = super::claim::prepare(ask).await.err().unwrap();
    eprintln!("non-recipient → {err}");
    eprintln!("cheque written, claimed from its record ✓");

    #[cfg(feature = "drops")]
    {
        // ── drop: alice drops 1 + 2 USDC to bob and a third wallet, at once ──
        let drop_job = super::drops::CreateJob {
            draft: super::drops::Draft {
                title: "live drop".into(),
                note: String::new(),
                token: token.clone(),
                recipients: vec![
                    crate::evm::drops::Recipient { address: bob.address(), amount: U256::from(1_000_000u64), schedule: crate::evm::drops::INSTANT },
                    crate::evm::drops::Recipient { address: Address::repeat_byte(0x44), amount: U256::from(2_000_000u64), schedule: crate::evm::drops::INSTANT },
                ],
                total: U256::from(3_000_000u64),
                fee_bps: 50,
                expires_at: now_chain(&rpc).await + 86_400,
            },
            chain: chain.clone(),
            rpc_urls: vec![url.clone()],
            contract: super::drops::contract_of(&chain).unwrap(),
            wallet: my(&alice, "Alice"),
            owner: "livetest".into(),
        };
        let ask = super::drops::CreateAsk { draft: drop_job.draft.clone(), chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallet: my(&alice, "Alice"), owner: "livetest".into() };
        let ready = super::drops::prepare_create(ask).await.unwrap();
        assert_eq!(ready.review.subtitle, "live drop");
        let made = super::drops::run_create(drop_job, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        assert!(!made.saved);
        let rec = unsaved::list().into_iter().rev().find(|r| r.account == "livetest" && r.kind == "drop").unwrap();
        assert_eq!(rec.payload["recipients"].as_array().unwrap().len(), 2);
        assert_eq!(rec.payload["recipients"][0]["amountWei"], "1000000");
        let drop_id: alloy_primitives::B256 = rec.payload["drop_id"].as_str().unwrap().parse().unwrap();
        let info = crate::evm::drops::info(&rpc, super::drops::contract_of(&chain).unwrap(), drop_id).await.unwrap();
        assert_eq!(info.totalAmount, U256::from(3_000_000u64));
        eprintln!("drop created, list kept ✓");

        // Bob claims his share (the proof for 2 leaves is the other leaf; Accordo would send it).
        let drops_contract = super::drops::contract_of(&chain).unwrap();
        let sibling = crate::evm::drops::leaf(1, Address::repeat_byte(0x44), U256::from(2_000_000u64), &crate::evm::drops::INSTANT);
        let slot = crate::api::drops::DropSlot {
            slot: 0,
            address: bob.address().to_checksum(None),
            amount: "1".into(),
            amount_wei: "1000000".into(),
            claimable: "1".into(),
            claimable_wei: "1000000".into(),
            schedule: crate::api::drops::SlotSchedule { start: 0, interval: 0, periods: 1, cliff: 0, upfront_bps: 0 },
            proof: Some(vec![format!("{sibling:#x}")]),
            ..Default::default()
        };
        let claim = super::drops::ClaimJob { chain: chain.clone(), rpc_urls: vec![url.clone()], contract: drops_contract, id: drop_id, slot, wallet: my(&bob, "Bob") };
        let claimed = super::drops::run_claim(claim, Wallet::new(BOB, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        assert!(claimed.warning.is_some()); // Accordo unreachable
        assert_eq!(crate::evm::drops::slot_claimed(&rpc, drops_contract, drop_id, 0).await.unwrap(), U256::from(1_000_000u64));
        eprintln!("drop share claimed ✓");
    }

    #[cfg(feature = "crowdfund")]
    {
        // ── campaign: alice launches (target 5 USDC), bob backs 5, alice ends early and withdraws ──
        let cf_contract = super::crowdfund::contract_of(&chain).unwrap();
        let deadline = now_chain(&rpc).await + 7 * 86_400;
        let launch = super::crowdfund::LaunchJob {
            draft: super::crowdfund::LaunchDraft { title: "live campaign".into(), description: String::new(), category: "Community".into(), token: token.clone(), target: "5".into(), target_units: 5_000_000, deadline, fee_bps: 50, all_or_nothing: false },
            chain: chain.clone(),
            rpc_urls: vec![url.clone()],
            contract: cf_contract,
            wallet: my(&alice, "Alice"),
            owner: "livetest".into(),
        };
        let launched = super::crowdfund::run_launch(launch, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        assert!(!launched.saved);
        // Its id: from the CampaignCreated event of the launch.
        let receipt = rpc.call("eth_getTransactionReceipt", serde_json::json!([format!("{:#x}", launched.hash)])).await.unwrap();
        let id_hex = receipt["logs"].as_array().unwrap().iter().find_map(|l| l["topics"].as_array().and_then(|t| t.get(1)).and_then(|t| t.as_str()).map(str::to_string)).unwrap();
        let campaign = crate::api::crowdfund::CampaignRead {
            campaign_id: id_hex.clone(),
            creator_username: "alice".into(),
            title: "live campaign".into(),
            chain_name: "Anvil".into(),
            token_symbol: "USDC".into(),
            token_decimals: 6,
            target_amount: 5.0,
            fee_bps: 50,
            status: crate::api::crowdfund::CampaignStatus::Active,
            ..Default::default()
        };
        let back = super::crowdfund::BackAsk {
            draft: super::crowdfund::BackDraft { campaign_id: id_hex.clone(), amount: "5".into(), message: "go".into() },
            campaign: campaign.clone(),
            chain: chain.clone(),
            rpc_urls: vec![url.clone()],
            explorer: None,
            wallet: my(&bob, "Bob"),
        };
        let ready = super::crowdfund::prepare_back(back).await.unwrap();
        let super::Job::Back(b) = ready.job else { panic!("back") };
        super::crowdfund::run_back(*b, Wallet::new(BOB, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        let act = |a: super::crowdfund::Action, w: &Wallet, name: &str| super::crowdfund::ActionAsk { action: a, campaign: campaign.clone(), chain: chain.clone(), rpc_urls: vec![url.clone()], explorer: None, wallets: vec![my(w, name)] };
        // Bob isn't the creator: no ending it for him.
        assert!(super::crowdfund::prepare_action(act(super::crowdfund::Action::EndEarly, &bob, "Bob")).await.is_err());
        let ready = super::crowdfund::prepare_action(act(super::crowdfund::Action::EndEarly, &alice, "Alice")).await.unwrap();
        let super::Job::Campaign(j) = ready.job else { panic!("action") };
        super::crowdfund::run_action(*j, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        let ready = super::crowdfund::prepare_action(act(super::crowdfund::Action::Withdraw, &alice, "Alice")).await.unwrap();
        let gets = ready.review.items.iter().find_map(|i| match i { crate::components::tx_review::Item::Row { label, value, .. } if label == "Your wallet gets" => Some(value.clone()), _ => None });
        eprintln!("withdraw review: {gets:?}");
        let super::Job::Campaign(j) = ready.job else { panic!("action") };
        super::crowdfund::run_action(*j, Wallet::new(ALICE, rpc.clone()).unwrap(), api(), out.clone()).await.unwrap();
        let id: alloy_primitives::B256 = id_hex.parse().unwrap();
        assert_eq!(crate::evm::crowdfund::state(&rpc, cf_contract, id).await.unwrap(), crate::evm::crowdfund::State::Claimed);
        eprintln!("campaign launched, backed, ended early, withdrawn ✓");
    }
}

#[cfg(any(feature = "drops", feature = "crowdfund"))]
async fn now_chain(rpc: &Rpc) -> u64 {
    let block = rpc.call("eth_getBlockByNumber", serde_json::json!(["latest", false])).await.unwrap();
    crate::evm::client::hex_u64(&block["timestamp"]).unwrap()
}
