#![warn(
    unused_extern_crates,
    missing_debug_implementations,
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

//! # jude-harness
//!
//! A simple lib to start a jude container (incl. juded and
//! jude-wallet-rpc). Provides initialisation methods to generate blocks,
//! create and fund accounts, and start a continuous mining task mining blocks
//! every BLOCK_TIME_SECS seconds.
//!
//! Also provides standalone JSON RPC clients for juded and jude-wallet-rpc.
pub mod image;

use crate::image::{JUDED_DAEMON_CONTAINER_NAME, JUDED_DEFAULT_NETWORK, RPC_PORT};
use anyhow::{anyhow, bail, Context, Result};
use jude_rpc::juded;
use jude_rpc::juded::JudedRpc as _;
use jude_rpc::wallet::{self, GetAddress, JudeWalletRpc as _, Refreshed, Transfer};
use std::time::Duration;
use testcontainers::clients::Cli;
use testcontainers::{Container, Docker, RunArgs};
use tokio::time;

/// How often we mine a block.
const BLOCK_TIME_SECS: u64 = 1;

/// Poll interval when checking if the wallet has synced with juded.
const WAIT_WALLET_SYNC_MILLIS: u64 = 1000;

#[derive(Clone, Debug)]
pub struct Jude {
    juded: Juded,
    wallets: Vec<JudeWalletRpc>,
}
impl<'c> Jude {
    /// Starts a new regtest jude container setup consisting out of 1 juded
    /// node and n wallets. The docker container and network will be prefixed
    /// with a randomly generated `prefix`. One miner wallet is started
    /// automatically.
    /// juded container name is: `prefix`_`juded`
    /// network is: `prefix`_`jude`
    /// miner wallet container name is: `miner`
    pub async fn new(
        cli: &'c Cli,
        additional_wallets: Vec<&'static str>,
    ) -> Result<(
        Self,
        Container<'c, Cli, image::Juded>,
        Vec<Container<'c, Cli, image::JudeWalletRpc>>,
    )> {
        let prefix = format!("{}_", random_prefix());
        let juded_name = format!("{}{}", prefix, JUDED_DAEMON_CONTAINER_NAME);
        let network = format!("{}{}", prefix, JUDED_DEFAULT_NETWORK);

        tracing::info!("Starting juded: {}", juded_name);
        let (juded, juded_container) = Juded::new(cli, juded_name, network)?;
        let mut containers = vec![];
        let mut wallets = vec![];

        let miner = "miner";
        tracing::info!("Starting miner wallet: {}", miner);
        let (miner_wallet, miner_container) =
            JudeWalletRpc::new(cli, miner, &juded, prefix.clone()).await?;

        wallets.push(miner_wallet);
        containers.push(miner_container);
        for wallet in additional_wallets.iter() {
            tracing::info!("Starting wallet: {}", wallet);

            // Create new wallet, the RPC sometimes has startup problems so we allow retries
            // (drop the container that failed and try again) Times out after
            // trying for 5 minutes
            let (wallet, container) = tokio::time::timeout(Duration::from_secs(300), async {
                loop {
                    let result = JudeWalletRpc::new(cli, wallet, &juded, prefix.clone()).await;

                    match result {
                        Ok(tuple) => { return tuple; }
                        Err(e) => { tracing::warn!("Jude wallet RPC emitted error {} - retrying to create wallet in 2 seconds...", e); }
                    }
                }
            }).await.context("All retry attempts for creating a wallet exhausted")?;

            wallets.push(wallet);
            containers.push(container);
        }

        Ok((Self { juded, wallets }, juded_container, containers))
    }

    pub fn juded(&self) -> &Juded {
        &self.juded
    }

    pub fn wallet(&self, name: &str) -> Result<&JudeWalletRpc> {
        let wallet = self
            .wallets
            .iter()
            .find(|wallet| wallet.name.eq(&name))
            .ok_or_else(|| anyhow!("Could not find wallet container."))?;

        Ok(wallet)
    }

    pub async fn init_miner(&self) -> Result<()> {
        let miner_wallet = self.wallet("miner")?;
        let miner_address = miner_wallet.address().await?.address;

        // generate the first 120 as bulk
        let amount_of_blocks = 120;
        let juded = &self.juded;
        let res = juded
            .client()
            .generateblocks(amount_of_blocks, miner_address.clone())
            .await?;
        tracing::info!("Generated {:?} blocks", res.blocks.len());
        miner_wallet.refresh().await?;

        Ok(())
    }

    pub async fn init_wallet(&self, name: &str, amount_in_outputs: Vec<u64>) -> Result<()> {
        let miner_wallet = self.wallet("miner")?;
        let miner_address = miner_wallet.address().await?.address;
        let juded = &self.juded;

        let wallet = self.wallet(name)?;
        let address = wallet.address().await?.address;

        for amount in amount_in_outputs {
            if amount > 0 {
                miner_wallet.transfer(&address, amount).await?;
                tracing::info!("Funded {} wallet with {}", wallet.name, amount);
                juded
                    .client()
                    .generateblocks(10, miner_address.clone())
                    .await?;
                wallet.refresh().await?;
            }
        }

        Ok(())
    }

    pub async fn start_miner(&self) -> Result<()> {
        let miner_wallet = self.wallet("miner")?;
        let miner_address = miner_wallet.address().await?.address;
        let juded = &self.juded;

        juded.start_miner(&miner_address).await?;

        tracing::info!("Waiting for miner wallet to catch up...");
        let block_height = juded.client().get_block_count().await?.count;
        miner_wallet
            .wait_for_wallet_height(block_height)
            .await
            .unwrap();

        Ok(())
    }

    pub async fn init_and_start_miner(&self) -> Result<()> {
        self.init_miner().await?;
        self.start_miner().await?;

        Ok(())
    }
}

fn random_prefix() -> String {
    use rand::Rng;

    rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(4)
        .collect()
}

#[derive(Clone, Debug)]
pub struct Juded {
    name: String,
    network: String,
    client: juded::Client,
}

#[derive(Clone, Debug)]
pub struct JudeWalletRpc {
    name: String,
    client: wallet::Client,
}

impl<'c> Juded {
    /// Starts a new regtest jude container.
    fn new(
        cli: &'c Cli,
        name: String,
        network: String,
    ) -> Result<(Self, Container<'c, Cli, image::Juded>)> {
        let image = image::Juded::default();
        let run_args = RunArgs::default()
            .with_name(name.clone())
            .with_network(network.clone());
        let container = cli.run_with_args(image, run_args);
        let juded_rpc_port = container
            .get_host_port(RPC_PORT)
            .context("port not exposed")?;

        Ok((
            Self {
                name,
                network,
                client: juded::Client::localhost(juded_rpc_port)?,
            },
            container,
        ))
    }

    pub fn client(&self) -> &juded::Client {
        &self.client
    }

    /// Spawns a task to mine blocks in a regular interval to the provided
    /// address
    pub async fn start_miner(&self, miner_wallet_address: &str) -> Result<()> {
        let juded = self.client().clone();
        let _ = tokio::spawn(mine(juded, miner_wallet_address.to_string()));
        Ok(())
    }
}

impl<'c> JudeWalletRpc {
    /// Starts a new wallet container which is attached to
    /// JUDED_DEFAULT_NETWORK and JUDED_DAEMON_CONTAINER_NAME
    async fn new(
        cli: &'c Cli,
        name: &str,
        juded: &Juded,
        prefix: String,
    ) -> Result<(Self, Container<'c, Cli, image::JudeWalletRpc>)> {
        let daemon_address = format!("{}:{}", juded.name, RPC_PORT);
        let image = image::JudeWalletRpc::new(name, daemon_address);

        let network = juded.network.clone();
        let run_args = RunArgs::default()
            // prefix the container name so we can run multiple tests
            .with_name(format!("{}{}", prefix, name))
            .with_network(network.clone());
        let container = cli.run_with_args(image, run_args);
        let wallet_rpc_port = container
            .get_host_port(RPC_PORT)
            .context("port not exposed")?;

        let client = wallet::Client::localhost(wallet_rpc_port)?;

        client
            .create_wallet(name.to_owned(), "English".to_owned())
            .await?;

        Ok((
            Self {
                name: name.to_string(),
                client,
            },
            container,
        ))
    }

    pub fn client(&self) -> &wallet::Client {
        &self.client
    }

    // It takes a little while for the wallet to sync with juded.
    pub async fn wait_for_wallet_height(&self, height: u32) -> Result<()> {
        let mut retry: u8 = 0;
        while self.client().get_height().await?.height < height {
            if retry >= 30 {
                // ~30 seconds
                bail!("Wallet could not catch up with juded after 30 retries.")
            }
            time::sleep(Duration::from_millis(WAIT_WALLET_SYNC_MILLIS)).await;
            retry += 1;
        }
        Ok(())
    }

    /// Sends amount to address
    pub async fn transfer(&self, address: &str, amount: u64) -> Result<Transfer> {
        self.client().transfer_single(0, amount, address).await
    }

    pub async fn address(&self) -> Result<GetAddress> {
        Ok(self.client().get_address(0).await?)
    }

    pub async fn balance(&self) -> Result<u64> {
        self.client().refresh().await?;
        let balance = self.client().get_balance(0).await?.balance;

        Ok(balance)
    }

    pub async fn refresh(&self) -> Result<Refreshed> {
        Ok(self.client().refresh().await?)
    }
}
/// Mine a block ever BLOCK_TIME_SECS seconds.
async fn mine(juded: juded::Client, reward_address: String) -> Result<()> {
    loop {
        time::sleep(Duration::from_secs(BLOCK_TIME_SECS)).await;
        juded.generateblocks(1, reward_address.clone()).await?;
    }
}
