mod bitcoind;
mod electrs;

use crate::testutils;
use anyhow::{Context, Result};
use bitcoin_harness::{BitcoindRpcApi, Client};
use futures::{future::RemoteHandle, Future};
use get_port::get_port;
use libp2p::{core::Multiaddr, PeerId};
use jude_harness::{image, jude};
use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use swap::{
    asb::{fixed_rate, fixed_rate::RATE},
    bitcoin,
    bitcoin::{CancelTimelock, PunishTimelock},
    database::Database,
    execution_params,
    execution_params::{ExecutionParams, GetExecutionParams},
    jude,
    protocol::{alice, alice::AliceState, bob, bob::BobState},
    seed::Seed,
};
use tempfile::tempdir;
use testcontainers::{clients::Cli, Container, Docker, RunArgs};
use tokio::{sync::mpsc, task::JoinHandle, time::interval};
use tracing::dispatcher::DefaultGuard;
use tracing_log::LogTracer;
use url::Url;
use uuid::Uuid;

const TEST_WALLET_NAME: &str = "testwallet";

#[derive(Debug, Clone)]
pub struct StartingBalances {
    pub jude: jude::Amount,
    pub btc: bitcoin::Amount,
}

#[derive(Clone)]
struct BobParams {
    seed: Seed,
    db_path: PathBuf,
    swap_id: Uuid,
    bitcoin_wallet: Arc<bitcoin::Wallet>,
    jude_wallet: Arc<jude::Wallet>,
    alice_address: Multiaddr,
    alice_peer_id: PeerId,
    execution_params: ExecutionParams,
}

impl BobParams {
    pub fn builder(&self, event_loop_handle: bob::EventLoopHandle) -> bob::Builder {
        bob::Builder::new(
            Database::open(&self.db_path.clone().as_path()).unwrap(),
            self.swap_id,
            self.bitcoin_wallet.clone(),
            self.jude_wallet.clone(),
            self.execution_params,
            event_loop_handle,
        )
    }

    pub fn new_eventloop(&self) -> Result<(bob::EventLoop, bob::EventLoopHandle)> {
        bob::EventLoop::new(
            &self.seed.derive_libp2p_identity(),
            self.alice_peer_id,
            self.alice_address.clone(),
            self.bitcoin_wallet.clone(),
        )
    }
}

pub struct BobEventLoopJoinHandle(JoinHandle<Result<Infallible>>);

impl BobEventLoopJoinHandle {
    pub fn abort(&self) {
        self.0.abort()
    }
}

pub struct AliceEventLoopJoinHandle(JoinHandle<()>);

pub struct TestContext {
    btc_amount: bitcoin::Amount,
    jude_amount: jude::Amount,

    alice_starting_balances: StartingBalances,
    alice_bitcoin_wallet: Arc<bitcoin::Wallet>,
    alice_jude_wallet: Arc<jude::Wallet>,
    alice_swap_handle: mpsc::Receiver<RemoteHandle<Result<AliceState>>>,

    bob_params: BobParams,
    bob_starting_balances: StartingBalances,
    bob_bitcoin_wallet: Arc<bitcoin::Wallet>,
    bob_jude_wallet: Arc<jude::Wallet>,
}

impl TestContext {
    pub async fn new_swap_as_bob(&mut self) -> (bob::Swap, BobEventLoopJoinHandle) {
        let (event_loop, event_loop_handle) = self.bob_params.new_eventloop().unwrap();

        let swap = self
            .bob_params
            .builder(event_loop_handle)
            .with_init_params(self.btc_amount)
            .build()
            .unwrap();

        let join_handle = tokio::spawn(event_loop.run());

        (swap, BobEventLoopJoinHandle(join_handle))
    }

    pub async fn stop_and_resume_bob_from_db(
        &mut self,
        join_handle: BobEventLoopJoinHandle,
    ) -> (bob::Swap, BobEventLoopJoinHandle) {
        join_handle.abort();

        let (event_loop, event_loop_handle) = self.bob_params.new_eventloop().unwrap();

        let swap = self.bob_params.builder(event_loop_handle).build().unwrap();

        let join_handle = tokio::spawn(event_loop.run());

        (swap, BobEventLoopJoinHandle(join_handle))
    }

    pub async fn assert_alice_redeemed(&mut self) {
        let swap_handle = self.alice_swap_handle.recv().await.unwrap();
        let state = swap_handle.await.unwrap();

        assert!(matches!(state, AliceState::BtcRedeemed));

        self.alice_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let btc_balance_after_swap = self.alice_bitcoin_wallet.as_ref().balance().await.unwrap();
        assert_eq!(
            btc_balance_after_swap,
            self.alice_starting_balances.btc + self.btc_amount
                - bitcoin::Amount::from_sat(bitcoin::TX_FEE)
        );

        let jude_balance_after_swap = self
            .alice_jude_wallet
            .as_ref()
            .get_balance()
            .await
            .unwrap();
        assert!(
            jude_balance_after_swap <= self.alice_starting_balances.jude - self.jude_amount,
            "{} !< {} - {}",
            jude_balance_after_swap,
            self.alice_starting_balances.jude,
            self.jude_amount
        );
    }

    pub async fn assert_alice_refunded(&mut self) {
        let swap_handle = self.alice_swap_handle.recv().await.unwrap();
        let state = swap_handle.await.unwrap();

        assert!(matches!(state, AliceState::judeRefunded));

        self.alice_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let btc_balance_after_swap = self.alice_bitcoin_wallet.as_ref().balance().await.unwrap();
        assert_eq!(btc_balance_after_swap, self.alice_starting_balances.btc);

        // Ensure that Alice's balance is refreshed as we use a newly created wallet
        self.alice_jude_wallet.as_ref().refresh().await.unwrap();
        let jude_balance_after_swap = self
            .alice_jude_wallet
            .as_ref()
            .get_balance()
            .await
            .unwrap();
        assert_eq!(jude_balance_after_swap, self.jude_amount);
    }

    pub async fn assert_alice_punished(&self, state: AliceState) {
        assert!(matches!(state, AliceState::BtcPunished));

        self.alice_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let btc_balance_after_swap = self.alice_bitcoin_wallet.as_ref().balance().await.unwrap();
        assert_eq!(
            btc_balance_after_swap,
            self.alice_starting_balances.btc + self.btc_amount
                - bitcoin::Amount::from_sat(2 * bitcoin::TX_FEE)
        );

        let jude_balance_after_swap = self
            .alice_jude_wallet
            .as_ref()
            .get_balance()
            .await
            .unwrap();
        assert!(jude_balance_after_swap <= self.alice_starting_balances.jude - self.jude_amount);
    }

    pub async fn assert_bob_redeemed(&self, state: BobState) {
        self.bob_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let lock_tx_id = if let BobState::judeRedeemed { tx_lock_id } = state {
            tx_lock_id
        } else {
            panic!("Bob in not in jude redeemed state: {:?}", state);
        };

        let lock_tx_bitcoin_fee = self
            .bob_bitcoin_wallet
            .transaction_fee(lock_tx_id)
            .await
            .unwrap();

        let btc_balance_after_swap = self.bob_bitcoin_wallet.as_ref().balance().await.unwrap();
        assert_eq!(
            btc_balance_after_swap,
            self.bob_starting_balances.btc - self.btc_amount - lock_tx_bitcoin_fee
        );

        // Ensure that Bob's balance is refreshed as we use a newly created wallet
        self.bob_jude_wallet.as_ref().refresh().await.unwrap();
        let jude_balance_after_swap = self.bob_jude_wallet.as_ref().get_balance().await.unwrap();
        assert_eq!(
            jude_balance_after_swap,
            self.bob_starting_balances.jude + self.jude_amount
        );
    }

    pub async fn assert_bob_refunded(&self, state: BobState) {
        self.bob_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let lock_tx_id = if let BobState::BtcRefunded(state4) = state {
            state4.tx_lock_id()
        } else {
            panic!("Bob in not in btc refunded state: {:?}", state);
        };
        let lock_tx_bitcoin_fee = self
            .bob_bitcoin_wallet
            .transaction_fee(lock_tx_id)
            .await
            .unwrap();

        let btc_balance_after_swap = self.bob_bitcoin_wallet.as_ref().balance().await.unwrap();

        let alice_submitted_cancel = btc_balance_after_swap
            == self.bob_starting_balances.btc
                - lock_tx_bitcoin_fee
                - bitcoin::Amount::from_sat(bitcoin::TX_FEE);

        let bob_submitted_cancel = btc_balance_after_swap
            == self.bob_starting_balances.btc
                - lock_tx_bitcoin_fee
                - bitcoin::Amount::from_sat(2 * bitcoin::TX_FEE);

        // The cancel tx can be submitted by both Alice and Bob.
        // Since we cannot be sure who submitted it we have to assert accordingly
        assert!(alice_submitted_cancel || bob_submitted_cancel);

        let jude_balance_after_swap = self.bob_jude_wallet.as_ref().get_balance().await.unwrap();
        assert_eq!(jude_balance_after_swap, self.bob_starting_balances.jude);
    }

    pub async fn assert_bob_punished(&self, state: BobState) {
        self.bob_bitcoin_wallet
            .sync_wallet()
            .await
            .expect("Could not sync wallet");

        let lock_tx_id = if let BobState::BtcPunished { tx_lock_id } = state {
            tx_lock_id
        } else {
            panic!("Bob in not in btc punished state: {:?}", state);
        };

        let lock_tx_bitcoin_fee = self
            .bob_bitcoin_wallet
            .transaction_fee(lock_tx_id)
            .await
            .unwrap();

        let btc_balance_after_swap = self.bob_bitcoin_wallet.as_ref().balance().await.unwrap();
        assert_eq!(
            btc_balance_after_swap,
            self.bob_starting_balances.btc - self.btc_amount - lock_tx_bitcoin_fee
        );

        let jude_balance_after_swap = self.bob_jude_wallet.as_ref().get_balance().await.unwrap();
        assert_eq!(jude_balance_after_swap, self.bob_starting_balances.jude);
    }
}

pub async fn setup_test<T, F, C>(_config: C, testfn: T)
where
    T: Fn(TestContext) -> F,
    F: Future<Output = ()>,
    C: GetExecutionParams,
{
    let cli = Cli::default();

    let _guard = init_tracing();

    let execution_params = C::get_execution_params();

    let (jude, containers) = testutils::init_containers(&cli).await;

    let btc_amount = bitcoin::Amount::from_sat(1_000_000);
    let jude_amount = jude::Amount::from_jude(btc_amount.as_btc() / RATE).unwrap();

    let alice_starting_balances = StartingBalances {
        jude: jude_amount * 10,
        btc: bitcoin::Amount::ZERO,
    };

    let port = get_port().expect("Failed to find a free port");

    let alice_listen_address: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port)
        .parse()
        .expect("failed to parse Alice's address");

    let electrs_rpc_port = containers
        .electrs
        .get_host_port(testutils::electrs::RPC_PORT)
        .expect("Could not map electrs rpc port");
    let electrs_http_port = containers
        .electrs
        .get_host_port(testutils::electrs::HTTP_PORT)
        .expect("Could not map electrs http port");

    let alice_seed = Seed::random().unwrap();
    let bob_seed = Seed::random().unwrap();

    let (alice_bitcoin_wallet, alice_jude_wallet) = init_test_wallets(
        "alice",
        containers.bitcoind_url.clone(),
        &jude,
        alice_starting_balances.clone(),
        tempdir().unwrap().path(),
        electrs_rpc_port,
        electrs_http_port,
        alice_seed,
    )
    .await;

    let db_path = tempdir().unwrap();
    let alice_db = Arc::new(Database::open(db_path.path()).unwrap());

    let alice_seed = Seed::random().unwrap();

    let bob_starting_balances = StartingBalances {
        jude: jude::Amount::ZERO,
        btc: btc_amount * 10,
    };

    let (bob_bitcoin_wallet, bob_jude_wallet) = init_test_wallets(
        "bob",
        containers.bitcoind_url,
        &jude,
        bob_starting_balances.clone(),
        tempdir().unwrap().path(),
        electrs_rpc_port,
        electrs_http_port,
        bob_seed,
    )
    .await;

    let (alice_event_loop, alice_swap_handle) = alice::EventLoop::new(
        alice_listen_address.clone(),
        alice_seed,
        execution_params,
        alice_bitcoin_wallet.clone(),
        alice_jude_wallet.clone(),
        alice_db,
        fixed_rate::RateService::default(),
        alice_starting_balances.jude,
    )
    .unwrap();

    let alice_peer_id = alice_event_loop.peer_id();

    tokio::spawn(alice_event_loop.run());

    let bob_params = BobParams {
        seed: Seed::random().unwrap(),
        db_path: tempdir().unwrap().path().to_path_buf(),
        swap_id: Uuid::new_v4(),
        bitcoin_wallet: bob_bitcoin_wallet.clone(),
        jude_wallet: bob_jude_wallet.clone(),
        alice_address: alice_listen_address,
        alice_peer_id,
        execution_params,
    };

    let test = TestContext {
        btc_amount,
        jude_amount,
        alice_starting_balances,
        alice_bitcoin_wallet,
        alice_jude_wallet,
        alice_swap_handle,
        bob_params,
        bob_starting_balances,
        bob_bitcoin_wallet,
        bob_jude_wallet,
    };

    testfn(test).await;
}

fn random_prefix() -> String {
    use rand::{distributions::Alphanumeric, thread_rng, Rng};
    use std::iter;
    const LEN: usize = 8;
    let mut rng = thread_rng();
    let chars: String = iter::repeat(())
        .map(|()| rng.sample(Alphanumeric))
        .map(char::from)
        .take(LEN)
        .collect();
    chars
}

async fn init_containers(cli: &Cli) -> (jude, Containers<'_>) {
    let prefix = random_prefix();
    let bitcoind_name = format!("{}_{}", prefix, "bitcoind");
    let (bitcoind, bitcoind_url) =
        init_bitcoind_container(&cli, prefix.clone(), bitcoind_name.clone(), prefix.clone())
            .await
            .expect("could not init bitcoind");
    let electrs = init_electrs_container(&cli, prefix.clone(), bitcoind_name, prefix)
        .await
        .expect("could not init electrs");
    let (jude, judeds) = init_jude_container(&cli).await;
    (jude, Containers {
        bitcoind_url,
        bitcoind,
        judeds,
        electrs,
    })
}

async fn init_bitcoind_container(
    cli: &Cli,
    volume: String,
    name: String,
    network: String,
) -> Result<(Container<'_, Cli, bitcoind::Bitcoind>, Url)> {
    let image = bitcoind::Bitcoind::default()
        .with_volume(volume)
        .with_tag("0.19.1");

    let run_args = RunArgs::default().with_name(name).with_network(network);

    let docker = cli.run_with_args(image, run_args);
    let a = docker
        .get_host_port(testutils::bitcoind::RPC_PORT)
        .context("Could not map bitcoind rpc port")?;

    let bitcoind_url = {
        let input = format!(
            "http://{}:{}@localhost:{}",
            bitcoind::RPC_USER,
            bitcoind::RPC_PASSWORD,
            a
        );
        Url::parse(&input).unwrap()
    };

    init_bitcoind(bitcoind_url.clone(), 5).await?;

    Ok((docker, bitcoind_url.clone()))
}

pub async fn init_electrs_container(
    cli: &Cli,
    volume: String,
    bitcoind_container_name: String,
    network: String,
) -> Result<Container<'_, Cli, electrs::Electrs>> {
    let bitcoind_rpc_addr = format!(
        "{}:{}",
        bitcoind_container_name,
        testutils::bitcoind::RPC_PORT
    );
    let image = electrs::Electrs::default()
        .with_volume(volume)
        .with_daemon_rpc_addr(bitcoind_rpc_addr)
        .with_tag("latest");

    let run_args = RunArgs::default().with_network(network);

    let docker = cli.run_with_args(image, run_args);

    Ok(docker)
}

async fn mine(bitcoind_client: Client, reward_address: bitcoin::Address) -> Result<()> {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        bitcoind_client
            .generatetoaddress(1, reward_address.clone(), None)
            .await?;
    }
}

async fn init_bitcoind(node_url: Url, spendable_quantity: u32) -> Result<Client> {
    let bitcoind_client = Client::new(node_url.clone());

    bitcoind_client
        .createwallet(TEST_WALLET_NAME, None, None, None, None)
        .await?;

    let reward_address = bitcoind_client
        .with_wallet(TEST_WALLET_NAME)?
        .getnewaddress(None, None)
        .await?;

    bitcoind_client
        .generatetoaddress(101 + spendable_quantity, reward_address.clone(), None)
        .await?;
    let _ = tokio::spawn(mine(bitcoind_client.clone(), reward_address));
    Ok(bitcoind_client)
}

/// Send Bitcoin to the specified address, limited to the spendable bitcoin
/// quantity.
pub async fn mint(node_url: Url, address: bitcoin::Address, amount: bitcoin::Amount) -> Result<()> {
    let bitcoind_client = Client::new(node_url.clone());

    bitcoind_client
        .send_to_address(TEST_WALLET_NAME, address.clone(), amount)
        .await?;

    // Confirm the transaction
    let reward_address = bitcoind_client
        .with_wallet(TEST_WALLET_NAME)?
        .getnewaddress(None, None)
        .await?;
    bitcoind_client
        .generatetoaddress(1, reward_address, None)
        .await?;

    Ok(())
}

async fn init_jude_container(
    cli: &Cli,
) -> (
    jude,
    Vec<Container<'_, Cli, jude_harness::image::jude>>,
) {
    let (jude, judeds) = jude::new(&cli, None, vec!["alice".to_string(), "bob".to_string()])
        .await
        .unwrap();

    (jude, judeds)
}

#[allow(clippy::too_many_arguments)]
async fn init_test_wallets(
    name: &str,
    bitcoind_url: Url,
    jude: &jude,
    starting_balances: StartingBalances,
    datadir: &Path,
    electrum_rpc_port: u16,
    electrum_http_port: u16,
    seed: Seed,
) -> (Arc<bitcoin::Wallet>, Arc<jude::Wallet>) {
    jude
        .init(vec![(name, starting_balances.jude.as_piconero())])
        .await
        .unwrap();

    let jude_wallet = swap::jude::Wallet::new_with_client(
        jude.wallet(name).unwrap().client(),
        jude::Network::default(),
        "irrelevant_for_tests".to_string(),
    );

    let electrum_rpc_url = {
        let input = format!("tcp://@localhost:{}", electrum_rpc_port);
        Url::parse(&input).unwrap()
    };
    let electrum_http_url = {
        let input = format!("http://@localhost:{}", electrum_http_port);
        Url::parse(&input).unwrap()
    };

    let btc_wallet = swap::bitcoin::Wallet::new(
        electrum_rpc_url,
        electrum_http_url,
        bitcoin::Network::Regtest,
        datadir,
        seed.derive_extended_private_key(bitcoin::Network::Regtest)
            .expect("Could not create extended private key from seed"),
    )
    .await
    .expect("could not init btc wallet");

    if starting_balances.btc != bitcoin::Amount::ZERO {
        mint(
            bitcoind_url,
            btc_wallet.new_address().await.unwrap(),
            starting_balances.btc,
        )
        .await
        .expect("could not mint btc starting balance");

        let mut interval = interval(Duration::from_secs(1u64));
        let mut retries = 0u8;
        let max_retries = 30u8;
        loop {
            retries += 1;
            btc_wallet
                .sync_wallet()
                .await
                .expect("Could not sync btc wallet");

            let btc_balance = btc_wallet.balance().await.unwrap();

            if btc_balance == starting_balances.btc {
                break;
            } else if retries == max_retries {
                panic!(
                    "Bitcoin wallet initialization failed, reached max retries upon balance sync"
                )
            }

            interval.tick().await;
        }
    }

    (Arc::new(btc_wallet), Arc::new(jude_wallet))
}

// This is just to keep the containers alive
#[allow(dead_code)]
struct Containers<'a> {
    bitcoind_url: Url,
    bitcoind: Container<'a, Cli, bitcoind::Bitcoind>,
    judeds: Vec<Container<'a, Cli, image::jude>>,
    electrs: Container<'a, Cli, electrs::Electrs>,
}

/// Utility function to initialize logging in the test environment.
/// Note that you have to keep the `_guard` in scope after calling in test:
///
/// ```rust
/// let _guard = init_tracing();
/// ```
pub fn init_tracing() -> DefaultGuard {
    // converts all log records into tracing events
    // Note: Make sure to initialize without unwrapping, otherwise this causes
    // trouble when running multiple tests.
    let _ = LogTracer::init();

    let global_filter = tracing::Level::WARN;
    let swap_filter = tracing::Level::DEBUG;
    let jude_btc_filter = tracing::Level::DEBUG;
    let jude_harness_filter = tracing::Level::INFO;
    let bitcoin_harness_filter = tracing::Level::INFO;
    let testcontainers_filter = tracing::Level::DEBUG;

    use tracing_subscriber::util::SubscriberInitExt as _;
    tracing_subscriber::fmt()
        .with_env_filter(format!(
            "{},swap={},jude_btc={},jude_harness={},bitcoin_harness={},testcontainers={}",
            global_filter,
            swap_filter,
            jude_btc_filter,
            jude_harness_filter,
            bitcoin_harness_filter,
            testcontainers_filter
        ))
        .set_default()
}

pub mod alice_run_until {
    use swap::protocol::alice::AliceState;

    pub fn is_jude_locked(state: &AliceState) -> bool {
        matches!(state, AliceState::judeLocked { .. })
    }

    pub fn is_encsig_learned(state: &AliceState) -> bool {
        matches!(state, AliceState::EncSigLearned { .. })
    }
}

pub mod bob_run_until {
    use swap::protocol::bob::BobState;

    pub fn is_btc_locked(state: &BobState) -> bool {
        matches!(state, BobState::BtcLocked(..))
    }

    pub fn is_lock_proof_received(state: &BobState) -> bool {
        matches!(state, BobState::judeLockProofReceived { .. })
    }

    pub fn is_jude_locked(state: &BobState) -> bool {
        matches!(state, BobState::judeLocked(..))
    }

    pub fn is_encsig_sent(state: &BobState) -> bool {
        matches!(state, BobState::EncSigSent(..))
    }
}

pub struct SlowCancelConfig;

impl GetExecutionParams for SlowCancelConfig {
    fn get_execution_params() -> ExecutionParams {
        ExecutionParams {
            bitcoin_cancel_timelock: CancelTimelock::new(180),
            ..execution_params::Regtest::get_execution_params()
        }
    }
}

pub struct FastCancelConfig;

impl GetExecutionParams for FastCancelConfig {
    fn get_execution_params() -> ExecutionParams {
        ExecutionParams {
            bitcoin_cancel_timelock: CancelTimelock::new(1),
            ..execution_params::Regtest::get_execution_params()
        }
    }
}

pub struct FastPunishConfig;

impl GetExecutionParams for FastPunishConfig {
    fn get_execution_params() -> ExecutionParams {
        ExecutionParams {
            bitcoin_cancel_timelock: CancelTimelock::new(1),
            bitcoin_punish_timelock: PunishTimelock::new(1),
            ..execution_params::Regtest::get_execution_params()
        }
    }
}
