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

use crate::image::{
    judeD_DAEMON_CONTAINER_NAME, judeD_DEFAULT_NETWORK, judeD_RPC_PORT, WALLET_RPC_PORT,
};
use anyhow::{anyhow, bail, Result};
use jude_rpc::{
    juded,
    wallet::{self, GetAddress, Refreshed, Transfer},
};
use std::time::Duration;
use testcontainers::{clients::Cli, core::Port, Container, Docker, RunArgs};
use tokio::time;

/// How often we mine a block.
const BLOCK_TIME_SECS: u64 = 1;

/// Poll interval when checking if the wallet has synced with juded.
const WAIT_WALLET_SYNC_MILLIS: u64 = 1000;

#[derive(Clone, Debug)]
pub struct jude {
    juded: juded,
    wallets: Vec<judeWalletRpc>,
    prefix: String,
}
impl<'c> jude {
    /// Starts a new regtest jude container setup consisting out of 1 juded
    /// node and n wallets. The containers and network will be prefixed, either
    /// randomly generated or as defined in `prefix` if provided. There will
    /// be 1 miner wallet started automatically. Default juded container
    /// name will be: `prefix`_`juded` Default miner wallet container name
    /// will be: `prefix`_`miner` Default network will be: `prefix`_`jude`
    pub async fn new(
        cli: &'c Cli,
        prefix: Option<String>,
        additional_wallets: Vec<String>,
    ) -> Result<(Self, Vec<Container<'c, Cli, image::jude>>)> {
        let prefix = format!("{}_", prefix.unwrap_or_else(random_prefix));

        let juded_name = format!("{}{}", prefix, judeD_DAEMON_CONTAINER_NAME);
        let network = format!("{}{}", prefix, judeD_DEFAULT_NETWORK);

        tracing::info!("Starting juded... {}", juded_name);
        let (juded, juded_container) = juded::new(cli, juded_name, network)?;
        let mut containers = vec![juded_container];
        let mut wallets = vec![];

        let miner = format!("{}{}", prefix, "miner");
        tracing::info!("Starting miner wallet... {}", miner);
        let (miner_wallet, miner_container) = judeWalletRpc::new(cli, &miner, &juded).await?;

        wallets.push(miner_wallet);
        containers.push(miner_container);
        for wallet in additional_wallets.iter() {
            tracing::info!("Starting wallet: {}...", wallet);
            let wallet = format!("{}{}", prefix, wallet);
            let (wallet, container) = judeWalletRpc::new(cli, &wallet, &juded).await?;
            wallets.push(wallet);
            containers.push(container);
        }

        Ok((
            Self {
                juded,
                wallets,
                prefix,
            },
            containers,
        ))
    }

    pub fn juded(&self) -> &juded {
        &self.juded
    }

    pub fn wallet(&self, name: &str) -> Result<&judeWalletRpc> {
        let name = format!("{}{}", self.prefix, name);
        let wallet = self
            .wallets
            .iter()
            .find(|wallet| wallet.name.eq(&name))
            .ok_or_else(|| anyhow!("Could not find wallet container."))?;

        Ok(wallet)
    }

    pub async fn init(&self, wallet_amount: Vec<(&str, u64)>) -> Result<()> {
        let miner_wallet = self.wallet("miner")?;
        let miner_address = miner_wallet.address().await?.address;

        // generate the first 70 as bulk
        let juded = &self.juded;
        let res = juded.client().generate_blocks(70, &miner_address).await?;
        tracing::info!("Generated {:?} blocks", res.blocks.len());
        miner_wallet.refresh().await?;

        for (wallet, amount) in wallet_amount.iter() {
            if *amount > 0 {
                let wallet = self.wallet(wallet)?;
                let address = wallet.address().await?.address;
                miner_wallet.transfer(&address, *amount).await?;
                tracing::info!("Funded {} wallet with {}", wallet.name, amount);
                juded.client().generate_blocks(10, &miner_address).await?;
                wallet.refresh().await?;
            }
        }

        juded.start_miner(&miner_address).await?;

        tracing::info!("Waiting for miner wallet to catch up...");
        let block_height = juded.client().get_block_count().await?;
        miner_wallet
            .wait_for_wallet_height(block_height)
            .await
            .unwrap();

        Ok(())
    }
}

fn random_prefix() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const LEN: usize = 4;
    let mut rng = rand::thread_rng();

    let prefix: String = (0..LEN)
        .map(|_| {
            let idx = rng.gen_range(0, CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    prefix
}

#[derive(Clone, Debug)]
pub struct juded {
    rpc_port: u16,
    name: String,
    network: String,
}

#[derive(Clone, Debug)]
pub struct judeWalletRpc {
    rpc_port: u16,
    name: String,
    network: String,
}

impl<'c> juded {
    /// Starts a new regtest jude container.
    fn new(
        cli: &'c Cli,
        name: String,
        network: String,
    ) -> Result<(Self, Container<'c, Cli, image::jude>)> {
        let juded_rpc_port: u16 =
            port_check::free_local_port().ok_or_else(|| anyhow!("Could not retrieve free port"))?;

        let image = image::jude::default();
        let run_args = RunArgs::default()
            .with_name(name.clone())
            .with_network(network.clone())
            .with_mapped_port(Port {
                local: juded_rpc_port,
                internal: judeD_RPC_PORT,
            });
        let docker = cli.run_with_args(image, run_args);

        Ok((
            Self {
                rpc_port: juded_rpc_port,
                name,
                network,
            },
            docker,
        ))
    }

    pub fn client(&self) -> juded::Client {
        juded::Client::localhost(self.rpc_port)
    }

    /// Spawns a task to mine blocks in a regular interval to the provided
    /// address
    pub async fn start_miner(&self, miner_wallet_address: &str) -> Result<()> {
        let juded = self.client();
        let _ = tokio::spawn(mine(juded, miner_wallet_address.to_string()));
        Ok(())
    }
}

impl<'c> judeWalletRpc {
    /// Starts a new wallet container which is attached to
    /// judeD_DEFAULT_NETWORK and judeD_DAEMON_CONTAINER_NAME
    async fn new(
        cli: &'c Cli,
        name: &str,
        juded: &juded,
    ) -> Result<(Self, Container<'c, Cli, image::jude>)> {
        let wallet_rpc_port: u16 =
            port_check::free_local_port().ok_or_else(|| anyhow!("Could not retrieve free port"))?;

        let daemon_address = format!("{}:{}", juded.name, judeD_RPC_PORT);
        let image = image::jude::wallet(&name, daemon_address);

        let network = juded.network.clone();
        let run_args = RunArgs::default()
            .with_name(name)
            .with_network(network.clone())
            .with_mapped_port(Port {
                local: wallet_rpc_port,
                internal: WALLET_RPC_PORT,
            });
        let docker = cli.run_with_args(image, run_args);

        // create new wallet
        wallet::Client::localhost(wallet_rpc_port)
            .create_wallet(name)
            .await
            .unwrap();

        Ok((
            Self {
                rpc_port: wallet_rpc_port,
                name: name.to_string(),
                network,
            },
            docker,
        ))
    }

    pub fn client(&self) -> wallet::Client {
        wallet::Client::localhost(self.rpc_port)
    }

    // It takes a little while for the wallet to sync with juded.
    pub async fn wait_for_wallet_height(&self, height: u32) -> Result<()> {
        let mut retry: u8 = 0;
        while self.client().block_height().await?.height < height {
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
        self.client().transfer(0, amount, address).await
    }

    pub async fn address(&self) -> Result<GetAddress> {
        self.client().get_address(0).await
    }

    pub async fn balance(&self) -> Result<u64> {
        self.client().refresh().await?;
        self.client().get_balance(0).await
    }

    pub async fn refresh(&self) -> Result<Refreshed> {
        self.client().refresh().await
    }
}
/// Mine a block ever BLOCK_TIME_SECS seconds.
async fn mine(juded: juded::Client, reward_address: String) -> Result<()> {
    loop {
        time::sleep(Duration::from_secs(BLOCK_TIME_SECS)).await;
        juded.generate_blocks(1, &reward_address).await?;
    }
}
