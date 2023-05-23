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
    bitcoin::{Amount, TxLock},
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
        bob::{cancel::CancelError, Builder, EventLoop},
    },
    seed::Seed,
};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

#[macro_use]
extern crate prettytable;

const jude_BLOCKCHAIN_MONITORING_WALLET_NAME: &str = "swap-tool-blockchain-monitoring-wallet";

#[tokio::main]
async fn main() -> Result<()> {
    let args = Arguments::from_args();

    let is_terminal = atty::is(atty::Stream::Stderr);
    let base_subscriber = |level| {
        FmtSubscriber::builder()
            .with_writer(std::io::stderr)
            .with_ansi(is_terminal)
            .with_target(false)
            .with_env_filter(format!("swap={}", level))
    };

    if args.debug {
        let subscriber = base_subscriber(Level::DEBUG)
            .with_timer(tracing_subscriber::fmt::time::ChronoLocal::with_format(
                "%F %T".to_owned(),
            ))
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let subscriber = base_subscriber(Level::INFO)
            .without_time()
            .with_level(false)
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;
    }

    let config = match args.config {
        Some(config_path) => read_config(config_path)??,
        None => Config::testnet(),
    };

    debug!(
        "Database and seed will be stored in {}",
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

    match args.cmd.unwrap_or_default() {
        Command::Buyjude {
            alice_peer_id,
            alice_addr,
        } => {
            let bitcoin_wallet =
                init_bitcoin_wallet(config, bitcoin_network, &wallet_data_dir, seed).await?;
            let jude_wallet =
                init_jude_wallet(jude_network, jude_wallet_rpc_process.endpoint()).await?;
            let bitcoin_wallet = Arc::new(bitcoin_wallet);

            let swap_id = Uuid::new_v4();

            // TODO: Also wait for more funds if balance < dust
            if bitcoin_wallet.balance().await? == Amount::ZERO {
                info!(
                    "Please deposit BTC to {}",
                    bitcoin_wallet.new_address().await?
                );

                while bitcoin_wallet.balance().await? == Amount::ZERO {
                    bitcoin_wallet.sync_wallet().await?;

                    tokio::time::sleep(Duration::from_secs(1)).await;
                }

                debug!("Received {}", bitcoin_wallet.balance().await?);
            } else {
                info!(
                    "Still got {} left in wallet, swapping ...",
                    bitcoin_wallet.balance().await?
                );
            }

            let send_bitcoin = bitcoin_wallet.max_giveable(TxLock::script_size()).await?;

            let (event_loop, event_loop_handle) = EventLoop::new(
                &seed.derive_libp2p_identity(),
                alice_peer_id,
                alice_addr,
                bitcoin_wallet.clone(),
            )?;
            let handle = tokio::spawn(event_loop.run());

            let swap = Builder::new(
                db,
                swap_id,
                bitcoin_wallet.clone(),
                Arc::new(jude_wallet),
                execution_params,
                event_loop_handle,
            )
            .with_init_params(send_bitcoin)
            .build()?;

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
            let bitcoin_wallet =
                init_bitcoin_wallet(config, bitcoin_network, &wallet_data_dir, seed).await?;
            let jude_wallet =
                init_jude_wallet(jude_network, jude_wallet_rpc_process.endpoint()).await?;
            let bitcoin_wallet = Arc::new(bitcoin_wallet);

            let (event_loop, event_loop_handle) = EventLoop::new(
                &seed.derive_libp2p_identity(),
                alice_peer_id,
                alice_addr,
                bitcoin_wallet.clone(),
            )?;
            let handle = tokio::spawn(event_loop.run());

            let swap = Builder::new(
                db,
                swap_id,
                bitcoin_wallet.clone(),
                Arc::new(jude_wallet),
                execution_params,
                event_loop_handle,
            )
            .build()?;

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
        Command::Cancel { swap_id, force } => {
            let bitcoin_wallet =
                init_bitcoin_wallet(config, bitcoin_network, &wallet_data_dir, seed).await?;

            let resume_state = db.get_state(swap_id)?.try_into_bob()?.into();
            let cancel =
                bob::cancel(swap_id, resume_state, Arc::new(bitcoin_wallet), db, force).await?;

            match cancel {
                Ok((txid, _)) => {
                    debug!("Cancel transaction successfully published with id {}", txid)
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
        Command::Refund { swap_id, force } => {
            let bitcoin_wallet =
                init_bitcoin_wallet(config, bitcoin_network, &wallet_data_dir, seed).await?;

            let resume_state = db.get_state(swap_id)?.try_into_bob()?.into();

            bob::refund(
                swap_id,
                resume_state,
                execution_params,
                Arc::new(bitcoin_wallet),
                db,
                force,
            )
            .await??;
        }
    };
    Ok(())
}

async fn init_bitcoin_wallet(
    config: Config,
    bitcoin_network: bitcoin::Network,
    bitcoin_wallet_data_dir: &Path,
    seed: Seed,
) -> Result<bitcoin::Wallet> {
    let bitcoin_wallet = bitcoin::Wallet::new(
        config.bitcoin.electrum_rpc_url,
        config.bitcoin.electrum_http_url,
        bitcoin_network,
        bitcoin_wallet_data_dir,
        seed.derive_extended_private_key(bitcoin_network)?,
    )
    .await?;

    bitcoin_wallet
        .sync_wallet()
        .await
        .context("failed to sync balance of bitcoin wallet")?;

    Ok(bitcoin_wallet)
}

async fn init_jude_wallet(
    jude_network: jude::Network,
    jude_wallet_rpc_url: Url,
) -> Result<jude::Wallet> {
    let jude_wallet = jude::Wallet::new(
        jude_wallet_rpc_url.clone(),
        jude_network,
        jude_BLOCKCHAIN_MONITORING_WALLET_NAME.to_string(),
    );

    // Setup the temporary jude wallet necessary for monitoring the blockchain
    let open_monitoring_wallet_response = jude_wallet.open().await;
    if open_monitoring_wallet_response.is_err() {
        jude_wallet
            .create_wallet(jude_BLOCKCHAIN_MONITORING_WALLET_NAME)
            .await
            .context(format!(
                "Unable to create jude wallet for blockchain monitoring.\
             Please ensure that the jude-wallet-rpc is available at {}",
                jude_wallet_rpc_url
            ))?;

        debug!(
            "Created jude wallet for blockchain monitoring with name {}",
            jude_BLOCKCHAIN_MONITORING_WALLET_NAME
        );
    }

    let _test_wallet_connection = jude_wallet
        .block_height()
        .await
        .context("failed to validate connection to jude-wallet-rpc")?;

    Ok(jude_wallet)
}
