use jude_harness::Jude;
use jude_rpc::juded::JudedRpc as _;
use spectral::prelude::*;
use std::time::Duration;
use testcontainers::clients::Cli;
use tokio::time;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::test]
async fn init_miner_and_mine_to_miner_address() {
    let _guard = tracing_subscriber::fmt()
        .with_env_filter("warn,test=debug,jude_harness=debug,jude_rpc=debug")
        .set_default();

    let tc = Cli::default();
    let (jude, _juded_container, _wallet_containers) = Jude::new(&tc, vec![]).await.unwrap();

    jude.init_and_start_miner().await.unwrap();

    let juded = jude.juded();
    let miner_wallet = jude.wallet("miner").unwrap();

    let got_miner_balance = miner_wallet.balance().await.unwrap();
    assert_that!(got_miner_balance).is_greater_than(0);

    time::sleep(Duration::from_millis(1010)).await;

    // after a bit more than 1 sec another block should have been mined
    let block_height = juded.client().get_block_count().await.unwrap().count;

    assert_that(&block_height).is_greater_than(70);
}
