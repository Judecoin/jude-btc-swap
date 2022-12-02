use crate::{
    bitcoin::{EncryptedSignature, TxCancel, TxRefund},
    jude,
    jude::jude_private_key,
    protocol::{alice, alice::AliceState},
};
use ::bitcoin::hashes::core::fmt::Display;
use libp2p::PeerId;
use jude_rpc::wallet::BlockHeight;
use serde::{Deserialize, Serialize};

// Large enum variant is fine because this is only used for database
// and is dropped once written in DB.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Alice {
    Started {
        state3: alice::State3,
        #[serde(with = "crate::serde_peer_id")]
        bob_peer_id: PeerId,
    },
    BtcLocked {
        state3: alice::State3,
        #[serde(with = "crate::serde_peer_id")]
        bob_peer_id: PeerId,
    },
    judeLocked {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: alice::State3,
    },
    EncSigLearned {
        jude_wallet_restore_blockheight: BlockHeight,
        encrypted_signature: EncryptedSignature,
        state3: alice::State3,
    },
    CancelTimelockExpired {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: alice::State3,
    },
    BtcCancelled {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: alice::State3,
    },
    BtcPunishable {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: alice::State3,
    },
    BtcRefunded {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: alice::State3,
        #[serde(with = "jude_private_key")]
        spend_key: jude::PrivateKey,
    },
    Done(AliceEndState),
}

#[derive(Copy, Clone, strum::Display, Debug, Deserialize, Serialize, PartialEq)]
pub enum AliceEndState {
    SafelyAborted,
    BtcRedeemed,
    judeRefunded,
    BtcPunished,
}

impl From<&AliceState> for Alice {
    fn from(alice_state: &AliceState) -> Self {
        match alice_state {
            AliceState::Started {
                state3,
                bob_peer_id,
            } => Alice::Started {
                state3: state3.as_ref().clone(),
                bob_peer_id: *bob_peer_id,
            },
            AliceState::BtcLocked {
                state3,
                bob_peer_id,
            } => Alice::BtcLocked {
                state3: state3.as_ref().clone(),
                bob_peer_id: *bob_peer_id,
            },
            AliceState::judeLocked {
                jude_wallet_restore_blockheight,
                state3,
            } => Alice::judeLocked {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                state3: state3.as_ref().clone(),
            },
            AliceState::EncSigLearned {
                jude_wallet_restore_blockheight,
                state3,
                encrypted_signature,
            } => Alice::EncSigLearned {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                state3: state3.as_ref().clone(),
                encrypted_signature: *encrypted_signature.clone(),
            },
            AliceState::BtcRedeemed => Alice::Done(AliceEndState::BtcRedeemed),
            AliceState::BtcCancelled {
                jude_wallet_restore_blockheight,
                state3,
                ..
            } => Alice::BtcCancelled {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                state3: state3.as_ref().clone(),
            },
            AliceState::BtcRefunded {
                jude_wallet_restore_blockheight,
                spend_key,
                state3,
            } => Alice::BtcRefunded {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                spend_key: *spend_key,
                state3: state3.as_ref().clone(),
            },
            AliceState::BtcPunishable {
                jude_wallet_restore_blockheight,
                state3,
                ..
            } => Alice::BtcPunishable {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                state3: state3.as_ref().clone(),
            },
            AliceState::judeRefunded => Alice::Done(AliceEndState::judeRefunded),
            AliceState::CancelTimelockExpired {
                jude_wallet_restore_blockheight,
                state3,
            } => Alice::CancelTimelockExpired {
                jude_wallet_restore_blockheight: *jude_wallet_restore_blockheight,
                state3: state3.as_ref().clone(),
            },
            AliceState::BtcPunished => Alice::Done(AliceEndState::BtcPunished),
            AliceState::SafelyAborted => Alice::Done(AliceEndState::SafelyAborted),
        }
    }
}

impl From<Alice> for AliceState {
    fn from(db_state: Alice) -> Self {
        match db_state {
            Alice::Started {
                state3,
                bob_peer_id,
            } => AliceState::Started {
                bob_peer_id,
                state3: Box::new(state3),
            },
            Alice::BtcLocked {
                state3,
                bob_peer_id,
            } => AliceState::BtcLocked {
                bob_peer_id,
                state3: Box::new(state3),
            },
            Alice::judeLocked {
                jude_wallet_restore_blockheight,
                state3,
            } => AliceState::judeLocked {
                jude_wallet_restore_blockheight,
                state3: Box::new(state3),
            },
            Alice::EncSigLearned {
                jude_wallet_restore_blockheight,
                state3: state,
                encrypted_signature,
            } => AliceState::EncSigLearned {
                jude_wallet_restore_blockheight,
                state3: Box::new(state),
                encrypted_signature: Box::new(encrypted_signature),
            },
            Alice::CancelTimelockExpired {
                jude_wallet_restore_blockheight,
                state3,
            } => AliceState::CancelTimelockExpired {
                jude_wallet_restore_blockheight,
                state3: Box::new(state3),
            },
            Alice::BtcCancelled {
                jude_wallet_restore_blockheight,
                state3,
            } => {
                let tx_cancel = TxCancel::new(
                    &state3.tx_lock,
                    state3.cancel_timelock,
                    state3.a.public(),
                    state3.B,
                );

                AliceState::BtcCancelled {
                    jude_wallet_restore_blockheight,
                    state3: Box::new(state3),
                    tx_cancel: Box::new(tx_cancel),
                }
            }
            Alice::BtcPunishable {
                jude_wallet_restore_blockheight,
                state3,
            } => {
                let tx_cancel = TxCancel::new(
                    &state3.tx_lock,
                    state3.cancel_timelock,
                    state3.a.public(),
                    state3.B,
                );
                let tx_refund = TxRefund::new(&tx_cancel, &state3.refund_address);
                AliceState::BtcPunishable {
                    jude_wallet_restore_blockheight,
                    tx_refund: Box::new(tx_refund),
                    state3: Box::new(state3),
                }
            }
            Alice::BtcRefunded {
                jude_wallet_restore_blockheight,
                state3,
                spend_key,
            } => AliceState::BtcRefunded {
                jude_wallet_restore_blockheight,
                spend_key,
                state3: Box::new(state3),
            },
            Alice::Done(end_state) => match end_state {
                AliceEndState::SafelyAborted => AliceState::SafelyAborted,
                AliceEndState::BtcRedeemed => AliceState::BtcRedeemed,
                AliceEndState::judeRefunded => AliceState::judeRefunded,
                AliceEndState::BtcPunished => AliceState::BtcPunished,
            },
        }
    }
}

impl Display for Alice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Alice::Started { .. } => write!(f, "Started"),
            Alice::BtcLocked { .. } => f.write_str("Bitcoin locked"),
            Alice::judeLocked { .. } => f.write_str("jude locked"),
            Alice::CancelTimelockExpired { .. } => f.write_str("Cancel timelock is expired"),
            Alice::BtcCancelled { .. } => f.write_str("Bitcoin cancel transaction published"),
            Alice::BtcPunishable { .. } => f.write_str("Bitcoin punishable"),
            Alice::BtcRefunded { .. } => f.write_str("jude refundable"),
            Alice::Done(end_state) => write!(f, "Done: {}", end_state),
            Alice::EncSigLearned { .. } => f.write_str("Encrypted signature learned"),
        }
    }
}
