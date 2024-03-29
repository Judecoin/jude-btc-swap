//! Run an jude/BTC swap in the role of Bob.
//! Bob holds BTC and wishes receive jude.
use crate::{
    bitcoin,
    database::Database,
    execution_params::ExecutionParams,
    jude,
    network::peer_tracker::{self, PeerTracker},
    protocol::{alice, alice::TransferProof, bob},
};
use anyhow::{Error, Result};
use libp2p::{core::Multiaddr, NetworkBehaviour, PeerId};
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

pub use self::{
    cancel::cancel,
    encrypted_signature::EncryptedSignature,
    event_loop::{EventLoop, EventLoopHandle},
    quote_request::*,
    refund::refund,
    state::*,
    swap::{run, run_until},
};
pub use execution_setup::{Message0, Message2, Message4};
use libp2p::request_response::ResponseChannel;

pub mod cancel;
mod encrypted_signature;
pub mod event_loop;
mod execution_setup;
mod quote_request;
pub mod refund;
pub mod state;
pub mod swap;
mod transfer_proof;

pub struct Swap {
    pub state: BobState,
    pub event_loop_handle: bob::EventLoopHandle,
    pub db: Database,
    pub bitcoin_wallet: Arc<bitcoin::Wallet>,
    pub jude_wallet: Arc<jude::Wallet>,
    pub execution_params: ExecutionParams,
    pub swap_id: Uuid,
}

pub struct Builder {
    swap_id: Uuid,
    db: Database,

    bitcoin_wallet: Arc<bitcoin::Wallet>,
    jude_wallet: Arc<jude::Wallet>,

    init_params: InitParams,
    execution_params: ExecutionParams,

    event_loop_handle: bob::EventLoopHandle,
}

enum InitParams {
    None,
    New { btc_amount: bitcoin::Amount },
}

impl Builder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Database,
        swap_id: Uuid,
        bitcoin_wallet: Arc<bitcoin::Wallet>,
        jude_wallet: Arc<jude::Wallet>,
        execution_params: ExecutionParams,
        event_loop_handle: bob::EventLoopHandle,
    ) -> Self {
        Self {
            swap_id,
            db,
            bitcoin_wallet,
            jude_wallet,
            init_params: InitParams::None,
            execution_params,
            event_loop_handle,
        }
    }

    pub fn with_init_params(self, btc_amount: bitcoin::Amount) -> Self {
        Self {
            init_params: InitParams::New { btc_amount },
            ..self
        }
    }

    pub fn build(self) -> Result<bob::Swap> {
        let state = match self.init_params {
            InitParams::New { btc_amount } => BobState::Started { btc_amount },
            InitParams::None => self.db.get_state(self.swap_id)?.try_into_bob()?.into(),
        };

        Ok(Swap {
            state,
            event_loop_handle: self.event_loop_handle,
            db: self.db,
            bitcoin_wallet: self.bitcoin_wallet.clone(),
            jude_wallet: self.jude_wallet.clone(),
            swap_id: self.swap_id,
            execution_params: self.execution_params,
        })
    }
}

#[derive(Debug)]
pub enum OutEvent {
    ConnectionEstablished(PeerId),
    QuoteResponse(alice::QuoteResponse),
    ExecutionSetupDone(Result<Box<State2>>),
    TransferProof {
        msg: Box<TransferProof>,
        channel: ResponseChannel<()>,
    },
    EncryptedSignatureAcknowledged,
    ResponseSent, // Same variant is used for all messages as no processing is done
    CommunicationError(Error),
}

impl From<peer_tracker::OutEvent> for OutEvent {
    fn from(event: peer_tracker::OutEvent) -> Self {
        match event {
            peer_tracker::OutEvent::ConnectionEstablished(id) => {
                OutEvent::ConnectionEstablished(id)
            }
        }
    }
}

impl From<quote_request::OutEvent> for OutEvent {
    fn from(event: quote_request::OutEvent) -> Self {
        use quote_request::OutEvent::*;
        match event {
            MsgReceived(quote_response) => OutEvent::QuoteResponse(quote_response),
            Failure(err) => OutEvent::CommunicationError(err.context("Failure with Quote Request")),
        }
    }
}

impl From<execution_setup::OutEvent> for OutEvent {
    fn from(event: execution_setup::OutEvent) -> Self {
        match event {
            execution_setup::OutEvent::Done(res) => OutEvent::ExecutionSetupDone(res.map(Box::new)),
        }
    }
}

impl From<transfer_proof::OutEvent> for OutEvent {
    fn from(event: transfer_proof::OutEvent) -> Self {
        use transfer_proof::OutEvent::*;
        match event {
            MsgReceived { msg, channel } => OutEvent::TransferProof {
                msg: Box::new(msg),
                channel,
            },
            AckSent => OutEvent::ResponseSent,
            Failure(err) => {
                OutEvent::CommunicationError(err.context("Failure with Transfer Proof"))
            }
        }
    }
}

impl From<encrypted_signature::OutEvent> for OutEvent {
    fn from(event: encrypted_signature::OutEvent) -> Self {
        use encrypted_signature::OutEvent::*;
        match event {
            Acknowledged => OutEvent::EncryptedSignatureAcknowledged,
            Failure(err) => {
                OutEvent::CommunicationError(err.context("Failure with Encrypted Signature"))
            }
        }
    }
}

/// A `NetworkBehaviour` that represents an jude/BTC swap node as Bob.
#[derive(NetworkBehaviour, Default)]
#[behaviour(out_event = "OutEvent", event_process = false)]
#[allow(missing_debug_implementations)]
pub struct Behaviour {
    pt: PeerTracker,
    quote_request: quote_request::Behaviour,
    execution_setup: execution_setup::Behaviour,
    transfer_proof: transfer_proof::Behaviour,
    encrypted_signature: encrypted_signature::Behaviour,
}

impl Behaviour {
    /// Sends a quote request to Alice to retrieve the rate.
    pub fn send_quote_request(&mut self, alice: PeerId, quote_request: QuoteRequest) {
        let _ = self.quote_request.send(alice, quote_request);
    }

    pub fn start_execution_setup(
        &mut self,
        alice_peer_id: PeerId,
        state0: State0,
        bitcoin_wallet: Arc<bitcoin::Wallet>,
    ) {
        self.execution_setup
            .run(alice_peer_id, state0, bitcoin_wallet);
    }

    pub fn send_encrypted_signature(
        &mut self,
        alice: PeerId,
        tx_redeem_encsig: bitcoin::EncryptedSignature,
    ) {
        let msg = EncryptedSignature { tx_redeem_encsig };
        self.encrypted_signature.send(alice, msg);
        debug!("Encrypted signature sent");
    }

    /// Add a known address for the given peer
    pub fn add_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        self.pt.add_address(peer_id, address)
    }
}
