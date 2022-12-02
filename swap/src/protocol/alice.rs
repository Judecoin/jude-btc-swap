//! Run an jude/BTC swap in the role of Alice.
//! Alice holds jude and wishes receive BTC.
use crate::{bitcoin, database, database::Database, execution_params::ExecutionParams, jude};
use anyhow::{bail, Result};
use libp2p::{core::Multiaddr, PeerId};
use std::sync::Arc;
use uuid::Uuid;

pub use self::{
    behaviour::{Behaviour, OutEvent},
    event_loop::{EventLoop, EventLoopHandle},
    execution_setup::Message1,
    quote_response::*,
    state::*,
    swap::{run, run_until},
    transfer_proof::TransferProof,
};
pub use execution_setup::Message3;

mod behaviour;
mod encrypted_signature;
pub mod event_loop;
mod execution_setup;
mod quote_response;
pub mod state;
mod steps;
pub mod swap;
mod transfer_proof;

pub struct Swap {
    pub state: AliceState,
    pub event_loop_handle: EventLoopHandle,
    pub bitcoin_wallet: Arc<bitcoin::Wallet>,
    pub jude_wallet: Arc<jude::Wallet>,
    pub execution_params: ExecutionParams,
    pub swap_id: Uuid,
    pub db: Arc<Database>,
}

pub struct Builder {
    swap_id: Uuid,
    peer_id: PeerId,
    db: Arc<Database>,
    execution_params: ExecutionParams,
    event_loop_handle: EventLoopHandle,
    listen_address: Multiaddr,

    bitcoin_wallet: Arc<bitcoin::Wallet>,
    jude_wallet: Arc<jude::Wallet>,

    init_params: InitParams,
}

enum InitParams {
    None,
    New {
        bob_peer_id: PeerId,
        state3: Box<State3>,
    },
}

impl Builder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        self_peer_id: PeerId,
        execution_params: ExecutionParams,
        swap_id: Uuid,
        bitcoin_wallet: Arc<bitcoin::Wallet>,
        jude_wallet: Arc<jude::Wallet>,
        db: Arc<Database>,
        listen_address: Multiaddr,
        event_loop_handle: EventLoopHandle,
    ) -> Self {
        Self {
            swap_id,
            peer_id: self_peer_id,
            db,
            execution_params,
            event_loop_handle,
            listen_address,
            bitcoin_wallet,
            jude_wallet,
            init_params: InitParams::None,
        }
    }

    pub fn with_init_params(self, bob_peer_id: PeerId, state3: State3) -> Self {
        Self {
            init_params: InitParams::New {
                bob_peer_id,
                state3: Box::new(state3),
            },
            ..self
        }
    }

    pub async fn build(self) -> Result<Swap> {
        match self.init_params {
            InitParams::New {
                bob_peer_id,
                ref state3,
            } => {
                let initial_state = AliceState::Started {
                    state3: state3.clone(),
                    bob_peer_id,
                };

                Ok(Swap {
                    event_loop_handle: self.event_loop_handle,
                    bitcoin_wallet: self.bitcoin_wallet,
                    jude_wallet: self.jude_wallet,
                    execution_params: self.execution_params,
                    db: self.db,
                    state: initial_state,
                    swap_id: self.swap_id,
                })
            }
            InitParams::None => {
                let resume_state =
                    if let database::Swap::Alice(state) = self.db.get_state(self.swap_id)? {
                        state.into()
                    } else {
                        bail!(
                            "Trying to load swap with id {} for the wrong direction.",
                            self.swap_id
                        )
                    };

                Ok(Swap {
                    state: resume_state,
                    event_loop_handle: self.event_loop_handle,
                    bitcoin_wallet: self.bitcoin_wallet,
                    jude_wallet: self.jude_wallet,
                    execution_params: self.execution_params,
                    swap_id: self.swap_id,
                    db: self.db,
                })
            }
        }
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn listen_address(&self) -> Multiaddr {
        self.listen_address.clone()
    }
}
