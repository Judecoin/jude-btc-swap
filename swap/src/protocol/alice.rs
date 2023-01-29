//! Run an jude/BTC swap in the role of Alice.
//! Alice holds jude and wishes receive BTC.
use crate::{bitcoin, database::Database, execution_params::ExecutionParams, jude};
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
