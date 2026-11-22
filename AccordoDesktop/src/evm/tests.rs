// SPDX-License-Identifier: GPL-3.0-or-later
//! The desktop encodes exactly what the web (ethers) encodes: calldata, Merkle leaves and roots,
//! ids and the AccordoTask claim signature. Expected values were produced with the web app's own ethers.

use alloy_primitives::{address, b256, keccak256, Address, U256};

use super::client::Call;
use super::{cheques, crowdfund, drops, invoices};

const A: Address = address!("1111111111111111111111111111111111111111");
const B: Address = address!("2222222222222222222222222222222222222222");
const C: Address = address!("5FbDB2315678afecb367f032d93F642f64180aa3");
const ID: alloy_primitives::B256 = b256!("abababababababababababababababababababababababababababababababab");
const SALT: alloy_primitives::B256 = b256!("cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd");

fn hex(c: &Call) -> String {
    format!("{}", c.data)
}

#[test]
fn v4_init_cheque() {
    let c = cheques::init_call(C, ID, B, U256::from(123_456_789u64), vec![A, B], 1_800_000_000, 50);
    assert_eq!(hex(&c), "0xb27001f4abababababababababababababababababababababababababababababababab000000000000000000000000222222222222222222222222222222222222222200000000000000000000000000000000000000000000000000000000075bcd1500000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000006b49d2000000000000000000000000000000000000000000000000000000000000000032000000000000000000000000000000000000000000000000000000000000000200000000000000000000000011111111111111111111111111111111111111110000000000000000000000002222222222222222222222222222222222222222");
    assert_eq!(c.value, U256::ZERO); // a token: nothing sent along
    assert_eq!(cheques::init_call(C, ID, Address::ZERO, U256::from(7u8), vec![A], 0, 50).value, U256::from(7u8));
}

#[test]
fn invoice_calls() {
    assert_eq!(hex(&invoices::pay_call(C, ID, B, U256::from(5_000_000u64), " 1234 ")), "0xc49aa57babababababababababababababababababababababababababababababababab00000000000000000000000000000000000000000000000000000000004c4b40000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000043132333400000000000000000000000000000000000000000000000000000000");
    let item = invoices::NewInvoice { id: ID, merchant: A, token: B, amount: 99, due_at: 0, fee_bps: 100, payer: Address::ZERO, code: "1234".into() };
    assert_eq!(hex(&invoices::create_batch_call(C, &[item]).unwrap()), "0x87c1e12000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001abababababababababababababababababababababababababababababababab000000000000000000000000111111111111111111111111111111111111111100000000000000000000000022222222222222222222222222222222222222220000000000000000000000000000000000000000000000000000000000000063000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000640000000000000000000000000000000000000000000000000000000000000000387a8233c96e1fc0ad5e284353276177af2186e7afa85296f106336e376669f7");
}

#[test]
fn crowdfund_contribute() {
    let c = crowdfund::contribute_call(C, ID, Address::ZERO, U256::from(9u8), "hi there");
    assert_eq!(hex(&c), "0xf6482810abababababababababababababababababababababababababababababababab000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000086869207468657265000000000000000000000000000000000000000000000000");
    assert_eq!(c.value, U256::from(9u8));
}

#[test]
fn drops_tree_and_calls() {
    let vest = drops::Schedule { start: 1000, interval: 60, periods: 4, cliff: 30, upfront_bps: 2500 };
    let list = vec![
        drops::Recipient { address: A, amount: U256::from(10u8), schedule: drops::INSTANT },
        drops::Recipient { address: B, amount: U256::from(20u8), schedule: vest },
        drops::Recipient { address: C, amount: U256::from(30u8), schedule: drops::INSTANT },
    ];
    assert_eq!(format!("{}", drops::leaf(1, B, U256::from(20u8), &vest)), "0x970b7a4436faadd688d9a892908b141f5a3f544b22f0563389024350f6d1ab37");
    let root = drops::root(&list).unwrap();
    assert_eq!(format!("{root}"), "0x3bfa2f8b759a98b228182c7a24c10c0d636f00781eb29164939b2678be6e60c9");
    assert_eq!(drops::vesting_end(&list), 1210);
    let init = Call::new(C, &super::abi::AccordoDrop::InitMerkleChequeCall { salt: SALT, tokenAddr: Address::ZERO, totalAmount: U256::from(60u8), merkleRoot: root, totalRecipientsCount: 3, expiresAt: 1_800_000_000, feeBps: 50, vestingEnd: 1210 });
    assert_eq!(hex(&init), "0x348bbe51cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003c3bfa2f8b759a98b228182c7a24c10c0d636f00781eb29164939b2678be6e60c90000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000006b49d200000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000004ba");
    let claim = Call::new(C, &super::abi::AccordoDrop::CashOutMerkleChequeCall { chequeId: ID, slot: 3, amount: U256::from(777u32), schedule: super::abi::AccordoDrop::Schedule { start: 1000, interval: 60, periods: 4, cliff: 30, upfrontBps: 2500 }, merkleProof: vec![SALT, ID] });
    assert_eq!(hex(&claim), "0x66172f55abababababababababababababababababababababababababababababababab0000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000030900000000000000000000000000000000000000000000000000000000000003e8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001e00000000000000000000000000000000000000000000000000000000000009c400000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000002cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdabababababababababababababababababababababababababababababababab");
}

#[test]
fn ids_and_task_signature() {
    assert_eq!(format!("{}", cheques::id_for(A, SALT)), "0x58816e9e87f7bc4f44bebc929854a3145fa1c92ea836b13558570c9d9275fffb");
    // Anvil's public test key #1 — never a real one.
    let sig = cheques::sign_claim("0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d", 31337, C, ID, A, cheques::NO_DEADLINE).unwrap();
    assert_eq!(format!("{sig}"), "0xf5061e24312903979564a9f3f3d127092c60152f9231cc835e59d7ec807af3cb31af8082ce330ab87c28b48199d38cda35b59b99f250d12989be6105ec24a46b1c");
    assert_eq!(invoices::code_hash("1234"), keccak256(b"1234"));
}

/// End to end on a local Anvil with the Accordo contracts (backend/local_anvil): every flow the
/// pages will use, signed by this code. Public Anvil test keys only.
///   ACCORDO_TEST_RPC=http://127.0.0.1:8546 ACCORDO_TEST_DEPLOYMENT=…/deployment.json \
///   cargo test -- --ignored live_anvil --nocapture
#[tokio::test]
#[ignore]
async fn live_anvil() {
    use super::client::{address, Rpc, Wallet};
    use crate::chain_rpc::{drip, Drip};
    let (Ok(url), Ok(dep)) = (std::env::var("ACCORDO_TEST_RPC"), std::env::var("ACCORDO_TEST_DEPLOYMENT")) else {
        eprintln!("set ACCORDO_TEST_RPC and ACCORDO_TEST_DEPLOYMENT");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dep).unwrap()).unwrap();
    let c = |k: &str| address(d["chain"]["contracts"][k].as_str().unwrap()).unwrap();
    let usdc = d["tokens"].as_array().unwrap().iter().find(|t| t["symbol"] == "USDC").unwrap();
    let usdc_addr = address(usdc["address"].as_str().unwrap()).unwrap();
    let rpc = Rpc::new(vec![url.clone()], Some(31337)).unwrap();
    // Anvil test accounts #2 (pays) and #3 (receives).
    let alice = Wallet::new("0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a", rpc.clone()).unwrap();
    let bob = Wallet::new("0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6", rpc.clone()).unwrap();
    let block = rpc.call("eth_getBlockByNumber", serde_json::json!(["latest", false])).await.unwrap();
    let now = super::client::hex_u64(&block["timestamp"]).unwrap();
    drip(url.clone(), false, format!("{:#x}", alice.address()), Drip::Token { symbol: "USDC".into(), address: format!("{usdc_addr:#x}"), decimals: 6 }).await.unwrap();
    let usd = |n: u64| U256::from(n * 1_000_000);
    let eth = |milli: u64| U256::from(milli) * U256::from(1_000_000_000_000_000u64);

    // AccordoCheques: a cheque in ETH: create, read, claim with the secret.
    let cheques_at = c("AccordoCheques");
    let (secret, id) = cheques::new_v4();
    cheques::create(&alice, cheques_at, id, vec![bob.address()], eth(1000), Address::ZERO, now + 86_400, 50).await.unwrap();
    let info = cheques::info(&rpc, cheques_at, id).await.unwrap();
    assert_eq!(info.to, vec![bob.address()]);
    cheques::claim(&bob, cheques_at, secret, false, U256::ZERO).await.unwrap();
    assert_eq!(cheques::info(&rpc, cheques_at, id).await.unwrap().claimed, vec![true]);
    let again = cheques::claim(&bob, cheques_at, secret, false, U256::ZERO).await.unwrap_err();
    eprintln!("claim twice → {again}");
    // …and in USDC (approve first).
    let (_, id2) = cheques::new_v4();
    cheques::create(&alice, cheques_at, id2, vec![bob.address()], usd(10), usdc_addr, now + 86_400, 50).await.unwrap();
    assert_eq!(cheques::info(&rpc, cheques_at, id2).await.unwrap().token, usdc_addr);
    eprintln!("AccordoCheques ✓");

    // Invoice with a PIN, paid in USDC.
    let hub = c("AccordoInvoice");
    let inv = invoices::NewInvoice { id: invoices::new_id(), merchant: bob.address(), token: usdc_addr, amount: 5_000_000, due_at: 0, fee_bps: 50, payer: Address::ZERO, code: "1234".into() };
    invoices::create(&bob, hub, &inv).await.unwrap();
    let wrong = invoices::pay(&alice, hub, inv.id, usdc_addr, usd(5), "0000").await.unwrap_err();
    eprintln!("wrong PIN → {wrong}");
    invoices::pay(&alice, hub, inv.id, usdc_addr, usd(5), "1234").await.unwrap();
    let on = invoices::get(&rpc, hub, inv.id).await.unwrap();
    assert!(invoices::status(&on, now).0, "settled");
    eprintln!("invoice ✓ (batch supported: {})", invoices::supports_batch(&rpc, hub).await);
    // A split bill: two invoices in one createInvoices, each with its own payer, amount and PIN.
    if invoices::supports_batch(&rpc, hub).await {
        let share = |payer: Address, amount: u128| invoices::NewInvoice { id: invoices::new_id(), merchant: bob.address(), token: usdc_addr, amount, due_at: 0, fee_bps: 50, payer, code: crate::flows::invoices::new_pin() };
        let items = [share(alice.address(), 3_333_334), share(Address::ZERO, 3_333_333)];
        invoices::create_batch(&bob, hub, &items).await.unwrap();
        for i in &items {
            let on = invoices::get(&rpc, hub, i.id).await.unwrap();
            assert_eq!((on.merchant, on.payer, on.amount), (bob.address(), i.payer, i.amount));
            assert!(invoices::code_matches(&i.code, on.codeHash));
        }
        eprintln!("split ✓");
    }

    // Campaign + contribution.
    let cf = c("AccordoCrowdfund");
    let (cid, _) = crowdfund::create(&bob, cf, &crowdfund::NewCampaign { token: Address::ZERO, target: 10u128.pow(18), deadline: now + 86_400 * 3, fee_bps: 50, all_or_nothing: false }).await.unwrap();
    crowdfund::contribute(&alice, cf, cid, Address::ZERO, eth(500), "go!").await.unwrap();
    assert!(crowdfund::contribution(&rpc, cf, cid, alice.address()).await.unwrap() > U256::ZERO);
    assert_eq!(crowdfund::state(&rpc, cf, cid).await.unwrap(), crowdfund::State::Active);
    eprintln!("crowdfund ✓");

    // Drop for one wallet: its proof is empty (the leaf is the root); claim it.
    let drop_at = c("AccordoDrop");
    let list = vec![drops::Recipient { address: bob.address(), amount: usd(10), schedule: drops::INSTANT }];
    let made = drops::create(&alice, drop_at, usdc_addr, &list, now + 86_400 * 7, 50).await.unwrap();
    assert_eq!(drops::info(&rpc, drop_at, made.drop_id).await.unwrap().totalAmount, usd(10));
    drops::claim(&bob, drop_at, made.drop_id, 0, usd(10), &drops::INSTANT, vec![]).await.unwrap();
    assert!(drops::slot_claimed(&rpc, drop_at, made.drop_id, 0).await.unwrap() > U256::ZERO);
    eprintln!("drop ✓ (creation fee {} wei)", made.creation_fee);

    // AccordoTask: create, sign the claim with the link key, claim from the recipient's wallet.
    let task_at = c("AccordoTask");
    let created = cheques::create_task_cheque(&alice, task_at, Address::ZERO, eth(200), vec![bob.address()], 0, now + 86_400, 50, None).await.unwrap();
    let key = created.claim_key.expect("a link key");
    let sig = cheques::sign_claim(&key, 31337, task_at, created.cheque_id, bob.address(), cheques::NO_DEADLINE).unwrap();
    cheques::claim_task_cheque(&bob, task_at, created.cheque_id, bob.address(), sig, cheques::NO_DEADLINE).await.unwrap();
    let r = cheques::read_task_cheque(&rpc, task_at, created.cheque_id).await.unwrap().expect("exists");
    assert_eq!(r.namedClaimed, vec![true]);
    eprintln!("AccordoTask ✓");

    // Token picker reads: balances (no Multicall3 on Anvil → one by one) and a token's own info.
    let found = super::balances::of(&rpc, alice.address(), &[usdc_addr]).await;
    assert!(found.get(&Address::ZERO).is_some_and(|b| !b.is_zero()), "gas coin balance");
    assert!(found.get(&usdc_addr).is_some(), "USDC balance");
    let (name, symbol, decimals) = super::balances::token_info(&rpc, usdc_addr).await.unwrap();
    assert_eq!((symbol.as_str(), decimals), ("USDC", 6));
    assert!(super::balances::token_info(&rpc, alice.address()).await.is_err(), "a wallet isn't a token");
    eprintln!("token reads ✓ ({name}: {} USDC)", super::client::format_units(found[&usdc_addr], 6));
}
