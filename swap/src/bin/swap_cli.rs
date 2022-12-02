#![warn(
    unused_extern_crates,
    missing_copy_implementations,
    rust_2018_idioms,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::fallible_impl_from,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::dbg_macro
)]
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

use anyhow::{Context, Result};
use prettytable::{row, Table};
use reqwest::Url;
use std::{path::Path, sync::Arc, time::Duration};
use structopt::StructOpt;
use swap::{
    bitcoin,
    bitcoin::Amount,
    cli::{
        command::{Arguments, Command},
        config::{read_config, Config},
    },
    database::Database,
    execution_params,
    execution_params::GetExecutionParams,
    jude,
    jude::{CreateWallet, OpenWallet},
    protocol::{
        bob,
        bob::{cancel::CancelError, Builder},
    },
    seed::Seed,
    trace::init_tracing,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::filter::LevelFilter;
use uuid::Uuid;

#[macro_use]
extern crate prettytable;

const jude_BLOCKCHAIN_MONITORING_WALLET_NAME: &str = "swap-tool-blockchain-monitoring-wallet";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing(LevelFilter::DEBUG).expect("initialize tracing");

    let opt = Arguments::from_args();

    let config = match opt.config {
        Some(config_path) => read_config(config_path)??,
        None => Config::testnet(),
    };

    info!(
        "Database and Seed will be stored in directory: {}",
        config.data.dir.display()
    );

    let db = Database::open(config.data.dir.join("database").as_path())
        .context("Could not open database")?;

    let wallet_data_dir = config.data.dir.join("wallet");
    let seed =
        Seed::from_file_or_generate(&config.data.dir).expect("Could not retrieve/initialize seed");

    // hardcode to testnet/stagenet
    let bitcoin_network = bitcoin::Network::Testnet;
    let jude_network = jude::Network::Stagenet;
    let execution_params = execution_params::Testnet::get_execution_params();

    let jude_wallet_rpc = jude::WalletRpc::new(config.data.dir.join("jude")).await?;

    let jude_wallet_rpc_process = jude_wallet_rpc
        .run(jude_network, "stagenet.community.jude.to")
        .await?;

    match opt.cmd.unwrap_or_default() {
        Command::Buyjude {
            alice_peer_id,
            alice_addr,
        } => {
            let (bitcoin_wallet, jude_wallet) = init_wallets(
                config,
                bitcoin_network,
                &wallet_data_dir,
                jude_network,
                seed,
                jude_wallet_rpc_process.endpoint(),
            )
            .await?;

            let swap_id = Uuid::new_v4();

            // TODO: Also wait for more funds if balance < dust
            if bitcoin_wallet.balance().await? == Amount::ZERO {
                debug!(
                    "Waiting for BTC at address {}",
                    bitcoin_wallet.new_address().await?
                );

                while bitcoin_wallet.balance().await? == Amount::ZERO {
                    bitcoin_wallet.sync_wallet().await?;

                    tokio::time::sleep(Duration::from_secs(1)).await;
                }

                debug!("Received {}", bitcoin_wallet.balance().await?);
            }

            let send_bitcoin = bitcoin_wallet.max_giveable().await?;

            info!("Swapping {} ...", send_bitcoin);

            let bob_factory = Builder::new(
                seed,
                db,
                swap_id,
                Arc::new(bitcoin_wallet),
                Arc::new(jude_wallet),
                alice_addr,
                alice_peer_id,
                execution_params,
            );
            let (swap, event_loop) = bob_factory.with_init_params(send_bitcoin).build().await?;

            let handle = tokio::spawn(async move { event_loop.run().await });
            let swap = bob::run(swap);
            tokio::select! {
                event_loop_result = handle => {
                    event_loop_result??;
                },
                swap_result = swap => {
                    swap_result?;
                }
            }
        }
        Command::History => {
            let mut table = Table::new();

            table.add_row(row!["SWAP ID", "STATE"]);

            for (swap_id, state) in db.all()? {
                table.add_row(row![swap_id, state]);
            }

            // Print the table to stdout
            table.printstd();
        }
        Command::Resume {
            swap_id,
            alice_peer_id,
            alice_addr,
        } => {
            let (bitcoin_wallet, jude_wallet) = init_wallets(
                config,
                bitcoin_network,
                &wallet_data_dir,
                jude_network,
                seed,
                jude_wallet_rpc_process.endpoint(),
            )
            .await?;

            let bob_factory = Builder::new(
                seed,
                db,
                swap_id,
                Arc::new(bitcoin_wallet),
                Arc::new(jude_wallet),
                alice_addr,
                alice_peer_id,
                execution_params,
            );
            let (swap, event_loop) = bob_factory.build().await?;
            let handle = tokio::spawn(async move { event_loop.run().await });
            let swap = bob::run(swap);
            tokio::select! {
                event_loop_result = handle => {
                    event_loop_result??;
                },
                swap_result = swap => {
                    swap_result?;
                }
            }
        }
        Command::Cancel {
            swap_id,
            alice_peer_id,
            alice_addr,
            force,
        } => {
            // TODO: Optimization: Only init the Bitcoin wallet, jude wallet unnecessary
            let (bitcoin_wallet, jude_wallet) = init_wallets(
                config,
                bitcoin_network,
                &wallet_data_dir,
                jude_network,
                seed,
                jude_wallet_rpc_process.endpoint(),
            )
            .await?;

            let bob_factory = Builder::new(
                seed,
                db,
                swap_id,
                Arc::new(bitcoin_wallet),
                Arc::new(jude_wallet),
                alice_addr,
                alice_peer_id,
                execution_params,
            );
            let (swap, event_loop) = bob_factory.build().await?;
            let handle = tokio::spawn(async move { event_loop.run().await });

            let cancel = bob::cancel(
                swap.swap_id,
                swap.state,
                swap.bitcoin_wallet,
                swap.db,
                force,
            );

            tokio::select! {
                event_loop_result = handle => {
                    event_loop_result??;
                },
                cancel_result = cancel => {
                    match cancel_result? {
                        Ok((txid, _)) => {
                            info!("Cancel transaction successfully published with id {}", txid)
                        }
                        Err(CancelError::CancelTimelockNotExpiredYet) => error!(
                            "The Cancel Transaction cannot be published yet, \
                            because the timelock has not expired. Please try again later."
                        ),
                        Err(CancelError::CancelTxAlreadyPublished) => {
                            warn!("The Cancel Transaction has already been published.")
                        }
                    }
                }
            }
        }
        Command::Refund {
            swap_id,
            alice_peer_id,
            alice_addr,
            force,
        } => {
            let (bitcoin_wallet, jude_wallet) = init_wallets(
                config,
                bitcoin_network,
                &wallet_data_dir,
                jude_network,
                seed,
                jude_wallet_rpc_process.endpoint(),
            )
            .await?;

            // TODO: Optimize to only use the Bitcoin wallet, jude wallet is unnecessary
            let bob_factory = Builder::new(
                seed,
                db,
                swap_id,
                Arc::new(bitcoin_wallet),
                Arc::new(jude_wallet),
                alice_addr,
                alice_peer_id,
                execution_params,
            );
            let (swap, event_loop) = bob_factory.build().await?;

            let handle = tokio::spawn(async move { event_loop.run().await });
            let refund = bob::refund(
                swap.swap_id,
                swap.state,
                swap.execution_params,
                swap.bitcoin_wallet,
                swap.db,
                force,
            );

            tokio::select! {
                event_loop_result = handle => {
                    event_loop_result??;
                },
                refund_result = refund => {
                    refund_result??;
                }
            }
        }
    };
    Ok(())
}

async fn init_wallets(
    config: Config,
    bitcoin_network: bitcoin::Network,
    bitcoin_wallet_data_dir: &Path,
    jude_network: jude::Network,
    seed: Seed,
    jude_wallet_rpc_url: Url,
) -> Result<(bitcoin::Wallet, jude::Wallet)> {
    let bitcoin_wallet = bitcoin::Wallet::new(
        config.bitcoin.electrum_rpc_url,
        config.bitcoin.electrum_http_url,
        bitcoin_network,
        bitcoin_wallet_data_dir,
        seed.extended_private_key(bitcoin_network)?.private_key,
    )
    .await?;

    bitcoin_wallet
        .sync_wallet()
        .await
        .expect("Could not sync btc wallet");

    let bitcoin_balance = bitcoin_wallet.balance().await?;
    info!(
        "Connection to Bitcoin wallet succeeded, balance: {}",
        bitcoin_balance
    );

    let jude_wallet = jude::Wallet::new(
        jude_wallet_rpc_url.clone(),
        jude_network,
        jude_BLOCKCHAIN_MONITORING_WALLET_NAME.to_string(),
    );

    // Setup the temporary jude wallet necessary for monitoring the blockchain
    let open_monitoring_wallet_response = jude_wallet
        .open_wallet(jude_BLOCKCHAIN_MONITORING_WALLET_NAME)
        .await;
    if open_monitoring_wallet_response.is_err() {
        jude_wallet
            .create_wallet(jude_BLOCKCHAIN_MONITORING_WALLET_NAME)
            .await
            .context(format!(
                "Unable to create jude wallet for blockchain monitoring.\
             Please ensure that the jude-wallet-rpc is available at {}",
                jude_wallet_rpc_url
            ))?;

        info!(
            "Created jude wallet for blockchain monitoring with name {}",
            jude_BLOCKCHAIN_MONITORING_WALLET_NAME
        );
    } else {
        info!(
            "Opened jude wallet for blockchain monitoring with name {}",
            jude_BLOCKCHAIN_MONITORING_WALLET_NAME
        );
    }

    let _test_wallet_connection = jude_wallet.block_height().await?;
    info!("The jude wallet RPC is set up correctly!");

    Ok((bitcoin_wallet, jude_wallet))
}
