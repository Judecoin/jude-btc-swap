use crate::{
    jude::TransferProof,
    protocol::{bob, bob::BobState},
};
use ::bitcoin::hashes::core::fmt::Display;
use jude_rpc::wallet::BlockHeight;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Bob {
    Started {
        #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
        btc_amount: bitcoin::Amount,
    },
    ExecutionSetupDone {
        state2: bob::State2,
    },
    BtcLocked {
        state3: bob::State3,
    },
    judeLockProofReceived {
        state: bob::State3,
        lock_transfer_proof: TransferProof,
        jude_wallet_restore_blockheight: BlockHeight,
    },
    judeLocked {
        state4: bob::State4,
    },
    EncSigSent {
        state4: bob::State4,
    },
    BtcRedeemed(bob::State5),
    CancelTimelockExpired(bob::State4),
    BtcCancelled(bob::State4),
    Done(BobEndState),
}

#[derive(Clone, strum::Display, Debug, Deserialize, Serialize, PartialEq)]
pub enum BobEndState {
    SafelyAborted,
    judeRedeemed { tx_lock_id: bitcoin::Txid },
    BtcRefunded(Box<bob::State4>),
    BtcPunished { tx_lock_id: bitcoin::Txid },
}

impl From<BobState> for Bob {
    fn from(bob_state: BobState) -> Self {
        match bob_state {
            BobState::Started { btc_amount } => Bob::Started { btc_amount },
            BobState::ExecutionSetupDone(state2) => Bob::ExecutionSetupDone { state2 },
            BobState::BtcLocked(state3) => Bob::BtcLocked { state3 },
            BobState::judeLockProofReceived {
                state,
                lock_transfer_proof,
                jude_wallet_restore_blockheight,
            } => Bob::judeLockProofReceived {
                state,
                lock_transfer_proof,
                jude_wallet_restore_blockheight,
            },
            BobState::judeLocked(state4) => Bob::judeLocked { state4 },
            BobState::EncSigSent(state4) => Bob::EncSigSent { state4 },
            BobState::BtcRedeemed(state5) => Bob::BtcRedeemed(state5),
            BobState::CancelTimelockExpired(state4) => Bob::CancelTimelockExpired(state4),
            BobState::BtcCancelled(state4) => Bob::BtcCancelled(state4),
            BobState::BtcRefunded(state4) => Bob::Done(BobEndState::BtcRefunded(Box::new(state4))),
            BobState::judeRedeemed { tx_lock_id } => {
                Bob::Done(BobEndState::judeRedeemed { tx_lock_id })
            }
            BobState::BtcPunished { tx_lock_id } => {
                Bob::Done(BobEndState::BtcPunished { tx_lock_id })
            }
            BobState::SafelyAborted => Bob::Done(BobEndState::SafelyAborted),
        }
    }
}

impl From<Bob> for BobState {
    fn from(db_state: Bob) -> Self {
        match db_state {
            Bob::Started { btc_amount } => BobState::Started { btc_amount },
            Bob::ExecutionSetupDone { state2 } => BobState::ExecutionSetupDone(state2),
            Bob::BtcLocked { state3 } => BobState::BtcLocked(state3),
            Bob::judeLockProofReceived {
                state,
                lock_transfer_proof,
                jude_wallet_restore_blockheight,
            } => BobState::judeLockProofReceived {
                state,
                lock_transfer_proof,
                jude_wallet_restore_blockheight,
            },
            Bob::judeLocked { state4 } => BobState::judeLocked(state4),
            Bob::EncSigSent { state4 } => BobState::EncSigSent(state4),
            Bob::BtcRedeemed(state5) => BobState::BtcRedeemed(state5),
            Bob::CancelTimelockExpired(state4) => BobState::CancelTimelockExpired(state4),
            Bob::BtcCancelled(state4) => BobState::BtcCancelled(state4),
            Bob::Done(end_state) => match end_state {
                BobEndState::SafelyAborted => BobState::SafelyAborted,
                BobEndState::judeRedeemed { tx_lock_id } => BobState::judeRedeemed { tx_lock_id },
                BobEndState::BtcRefunded(state4) => BobState::BtcRefunded(*state4),
                BobEndState::BtcPunished { tx_lock_id } => BobState::BtcPunished { tx_lock_id },
            },
        }
    }
}

impl Display for Bob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bob::Started { .. } => write!(f, "Started"),
            Bob::ExecutionSetupDone { .. } => f.write_str("Execution setup done"),
            Bob::BtcLocked { .. } => f.write_str("Bitcoin locked"),
            Bob::judeLockProofReceived { .. } => {
                f.write_str("jude lock transaction transfer proof received")
            }
            Bob::judeLocked { .. } => f.write_str("jude locked"),
            Bob::CancelTimelockExpired(_) => f.write_str("Cancel timelock is expired"),
            Bob::BtcCancelled(_) => f.write_str("Bitcoin refundable"),
            Bob::BtcRedeemed(_) => f.write_str("jude redeemable"),
            Bob::Done(end_state) => write!(f, "Done: {}", end_state),
            Bob::EncSigSent { .. } => f.write_str("Encrypted signature sent"),
        }
    }
}
