use crate::testutils::init_tracing;
use jude_harness::jude;
use spectral::prelude::*;
use testcontainers::clients::Cli;

mod testutils;

#[tokio::test]
async fn fund_transfer_and_check_tx_key() {
    let _guard = init_tracing();

    let fund_alice: u64 = 1_000_000_000_000;
    let fund_bob = 0;
    let send_to_bob = 5_000_000_000;

    let tc = Cli::default();
    let (jude, _containers) = jude::new(&tc, Some("test_".to_string()), vec![
        "alice".to_string(),
        "bob".to_string(),
    ])
    .await
    .unwrap();
    let alice_wallet = jude.wallet("alice").unwrap();
    let bob_wallet = jude.wallet("bob").unwrap();
    let miner_wallet = jude.wallet("miner").unwrap();

    let miner_address = miner_wallet.address().await.unwrap().address;

    // fund alice
    jude
        .init(vec![("alice", fund_alice), ("bob", fund_bob)])
        .await
        .unwrap();

    // check alice balance
    alice_wallet.refresh().await.unwrap();
    let got_alice_balance = alice_wallet.balance().await.unwrap();
    assert_that(&got_alice_balance).is_equal_to(fund_alice);

    // transfer from alice to bob
    let bob_address = bob_wallet.address().await.unwrap().address;
    let transfer = alice_wallet
        .transfer(&bob_address, send_to_bob)
        .await
        .unwrap();

    jude
        .juded()
        .client()
        .generate_blocks(10, &miner_address)
        .await
        .unwrap();

    bob_wallet.refresh().await.unwrap();
    let got_bob_balance = bob_wallet.balance().await.unwrap();
    assert_that(&got_bob_balance).is_equal_to(send_to_bob);

    // check if tx was actually seen
    let tx_id = transfer.tx_hash;
    let tx_key = transfer.tx_key;
    let res = bob_wallet
        .client()
        .check_tx_key(&tx_id, &tx_key, &bob_address)
        .await
        .expect("failed to check tx by key");

    assert_that!(res.received).is_equal_to(send_to_bob);
}
