use crate::{
    bitcoin::{ExpiredTimelocks, Txid, Wallet},
    database::{Database, Swap},
    protocol::bob::BobState,
};
use anyhow::{bail, Result};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error, Clone, Copy)]
pub enum CancelError {
    #[error("The cancel timelock has not expired yet.")]
    CancelTimelockNotExpiredYet,
    #[error("The cancel transaction has already been published.")]
    CancelTxAlreadyPublished,
}

pub async fn cancel(
    swap_id: Uuid,
    state: BobState,
    bitcoin_wallet: Arc<Wallet>,
    db: Database,
    force: bool,
) -> Result<Result<(Txid, BobState), CancelError>> {
    let state4 = match state {
        BobState::BtcLocked(state3) => state3.cancel(),
        BobState::judeLockProofReceived { state, .. } => state.cancel(),
        BobState::judeLocked(state4) => state4,
        BobState::EncSigSent(state4) => state4,
        BobState::CancelTimelockExpired(state4) => state4,
        _ => bail!(
            "Cannot cancel swap {} because it is in state {} which is not refundable.",
            swap_id,
            state
        ),
    };

    if !force {
        if let ExpiredTimelocks::None = state4.expired_timelock(bitcoin_wallet.as_ref()).await? {
            return Ok(Err(CancelError::CancelTimelockNotExpiredYet));
        }

        if state4
            .check_for_tx_cancel(bitcoin_wallet.as_ref())
            .await
            .is_ok()
        {
            let state = BobState::BtcCancelled(state4);
            let db_state = state.into();
            db.insert_latest_state(swap_id, Swap::Bob(db_state)).await?;

            return Ok(Err(CancelError::CancelTxAlreadyPublished));
        }
    }

    let txid = state4.submit_tx_cancel(bitcoin_wallet.as_ref()).await?;

    let state = BobState::BtcCancelled(state4);
    let db_state = state.clone().into();
    db.insert_latest_state(swap_id, Swap::Bob(db_state)).await?;

    Ok(Ok((txid, state)))
}
