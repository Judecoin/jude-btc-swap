use crate::testutils::init_tracing;
use jude_harness::jude;
use spectral::prelude::*;
use std::time::Duration;
use testcontainers::clients::Cli;
use tokio::time;

mod testutils;

#[tokio::test]
async fn init_miner_and_mine_to_miner_address() {
    let _guard = init_tracing();

    let tc = Cli::default();
    let (jude, _juded_container) = jude::new(&tc, None, vec![]).await.unwrap();

    jude.init(vec![]).await.unwrap();

    let juded = jude.juded();
    let miner_wallet = jude.wallet("miner").unwrap();

    let got_miner_balance = miner_wallet.balance().await.unwrap();
    assert_that!(got_miner_balance).is_greater_than(0);

    time::sleep(Duration::from_millis(1010)).await;

    // after a bit more than 1 sec another block should have been mined
    let block_height = juded.client().get_block_count().await.unwrap();

    assert_that(&block_height).is_greater_than(70);
}
