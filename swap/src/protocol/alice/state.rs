use crate::{
    bitcoin,
    bitcoin::{
        current_epoch, wait_for_cancel_timelock_to_expire, CancelTimelock, ExpiredTimelocks,
        GetBlockHeight, PunishTimelock, TransactionBlockHeight, TxCancel, TxRefund,
        WatchForRawTransaction,
    },
    execution_params::ExecutionParams,
    jude,
    protocol::{
        alice::{Message1, Message3},
        bob::{Message0, Message2, Message4},
        CROSS_CURVE_PROOF_SYSTEM,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use libp2p::PeerId;
use jude_rpc::wallet::BlockHeight;
use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sigma_fun::ext::dl_secp256k1_ed25519_eq::CrossCurveDLEQProof;
use std::fmt;

#[derive(Debug)]
pub enum AliceState {
    Started {
        bob_peer_id: PeerId,
        state3: Box<State3>,
    },
    BtcLocked {
        bob_peer_id: PeerId,
        state3: Box<State3>,
    },
    judeLocked {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: Box<State3>,
    },
    EncSigLearned {
        jude_wallet_restore_blockheight: BlockHeight,
        encrypted_signature: Box<bitcoin::EncryptedSignature>,
        state3: Box<State3>,
    },
    BtcRedeemed,
    BtcCancelled {
        jude_wallet_restore_blockheight: BlockHeight,
        tx_cancel: Box<TxCancel>,
        state3: Box<State3>,
    },
    BtcRefunded {
        jude_wallet_restore_blockheight: BlockHeight,
        spend_key: jude::PrivateKey,
        state3: Box<State3>,
    },
    BtcPunishable {
        jude_wallet_restore_blockheight: BlockHeight,
        tx_refund: Box<TxRefund>,
        state3: Box<State3>,
    },
    judeRefunded,
    CancelTimelockExpired {
        jude_wallet_restore_blockheight: BlockHeight,
        state3: Box<State3>,
    },
    BtcPunished,
    SafelyAborted,
}

impl fmt::Display for AliceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AliceState::Started { .. } => write!(f, "started"),
            AliceState::BtcLocked { .. } => write!(f, "btc is locked"),
            AliceState::judeLocked { .. } => write!(f, "jude is locked"),
            AliceState::EncSigLearned { .. } => write!(f, "encrypted signature is learned"),
            AliceState::BtcRedeemed => write!(f, "btc is redeemed"),
            AliceState::BtcCancelled { .. } => write!(f, "btc is cancelled"),
            AliceState::BtcRefunded { .. } => write!(f, "btc is refunded"),
            AliceState::BtcPunished => write!(f, "btc is punished"),
            AliceState::SafelyAborted => write!(f, "safely aborted"),
            AliceState::BtcPunishable { .. } => write!(f, "btc is punishable"),
            AliceState::judeRefunded => write!(f, "jude is refunded"),
            AliceState::CancelTimelockExpired { .. } => write!(f, "cancel timelock is expired"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct State0 {
    pub a: bitcoin::SecretKey,
    pub s_a: jude::Scalar,
    pub v_a: jude::PrivateViewKey,
    pub(crate) S_a_jude: jude::PublicKey,
    pub(crate) S_a_bitcoin: bitcoin::PublicKey,
    pub dleq_proof_s_a: CrossCurveDLEQProof,
    #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
    pub btc: bitcoin::Amount,
    pub jude: jude::Amount,
    pub cancel_timelock: CancelTimelock,
    pub punish_timelock: PunishTimelock,
    pub redeem_address: bitcoin::Address,
    pub punish_address: bitcoin::Address,
}

impl State0 {
    pub async fn new<R>(
        btc: bitcoin::Amount,
        jude: jude::Amount,
        execution_params: ExecutionParams,
        bitcoin_wallet: &bitcoin::Wallet,
        rng: &mut R,
    ) -> Result<Self>
    where
        R: RngCore + CryptoRng,
    {
        let a = bitcoin::SecretKey::new_random(rng);
        let v_a = jude::PrivateViewKey::new_random(rng);
        let redeem_address = bitcoin_wallet.new_address().await?;
        let punish_address = redeem_address.clone();

        let s_a = jude::Scalar::random(rng);
        let (dleq_proof_s_a, (S_a_bitcoin, S_a_jude)) = CROSS_CURVE_PROOF_SYSTEM.prove(&s_a, rng);

        Ok(Self {
            a,
            s_a,
            v_a,
            S_a_bitcoin: S_a_bitcoin.into(),
            S_a_jude: jude::PublicKey {
                point: S_a_jude.compress(),
            },
            dleq_proof_s_a,
            redeem_address,
            punish_address,
            btc,
            jude,
            cancel_timelock: execution_params.bitcoin_cancel_timelock,
            punish_timelock: execution_params.bitcoin_punish_timelock,
        })
    }

    pub fn receive(self, msg: Message0) -> Result<State1> {
        let valid = CROSS_CURVE_PROOF_SYSTEM.verify(
            &msg.dleq_proof_s_b,
            (
                msg.S_b_bitcoin.into(),
                msg.S_b_jude
                    .point
                    .decompress()
                    .ok_or_else(|| anyhow!("S_b is not a jude curve point"))?,
            ),
        );

        if !valid {
            bail!("Bob's dleq proof doesn't verify")
        }

        let v = self.v_a + msg.v_b;

        Ok(State1 {
            a: self.a,
            B: msg.B,
            s_a: self.s_a,
            S_a_jude: self.S_a_jude,
            S_a_bitcoin: self.S_a_bitcoin,
            S_b_jude: msg.S_b_jude,
            S_b_bitcoin: msg.S_b_bitcoin,
            v,
            v_a: self.v_a,
            dleq_proof_s_a: self.dleq_proof_s_a,
            btc: self.btc,
            jude: self.jude,
            cancel_timelock: self.cancel_timelock,
            punish_timelock: self.punish_timelock,
            refund_address: msg.refund_address,
            redeem_address: self.redeem_address,
            punish_address: self.punish_address,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct State1 {
    a: bitcoin::SecretKey,
    B: bitcoin::PublicKey,
    s_a: jude::Scalar,
    S_a_jude: jude::PublicKey,
    S_a_bitcoin: bitcoin::PublicKey,
    S_b_jude: jude::PublicKey,
    S_b_bitcoin: bitcoin::PublicKey,
    v: jude::PrivateViewKey,
    v_a: jude::PrivateViewKey,
    dleq_proof_s_a: CrossCurveDLEQProof,
    #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
    btc: bitcoin::Amount,
    jude: jude::Amount,
    cancel_timelock: CancelTimelock,
    punish_timelock: PunishTimelock,
    refund_address: bitcoin::Address,
    redeem_address: bitcoin::Address,
    punish_address: bitcoin::Address,
}

impl State1 {
    pub fn next_message(&self) -> Message1 {
        Message1 {
            A: self.a.public(),
            S_a_jude: self.S_a_jude,
            S_a_bitcoin: self.S_a_bitcoin,
            dleq_proof_s_a: self.dleq_proof_s_a.clone(),
            v_a: self.v_a,
            redeem_address: self.redeem_address.clone(),
            punish_address: self.punish_address.clone(),
        }
    }

    pub fn receive(self, msg: Message2) -> State2 {
        State2 {
            a: self.a,
            B: self.B,
            s_a: self.s_a,
            S_b_jude: self.S_b_jude,
            S_b_bitcoin: self.S_b_bitcoin,
            v: self.v,
            btc: self.btc,
            jude: self.jude,
            cancel_timelock: self.cancel_timelock,
            punish_timelock: self.punish_timelock,
            refund_address: self.refund_address,
            redeem_address: self.redeem_address,
            punish_address: self.punish_address,
            tx_lock: msg.tx_lock,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct State2 {
    a: bitcoin::SecretKey,
    B: bitcoin::PublicKey,
    s_a: jude::Scalar,
    S_b_jude: jude::PublicKey,
    S_b_bitcoin: bitcoin::PublicKey,
    v: jude::PrivateViewKey,
    #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
    btc: bitcoin::Amount,
    jude: jude::Amount,
    cancel_timelock: CancelTimelock,
    punish_timelock: PunishTimelock,
    refund_address: bitcoin::Address,
    redeem_address: bitcoin::Address,
    punish_address: bitcoin::Address,
    tx_lock: bitcoin::TxLock,
}

impl State2 {
    pub fn next_message(&self) -> Message3 {
        let tx_cancel =
            bitcoin::TxCancel::new(&self.tx_lock, self.cancel_timelock, self.a.public(), self.B);

        let tx_refund = bitcoin::TxRefund::new(&tx_cancel, &self.refund_address);
        // Alice encsigns the refund transaction(bitcoin) digest with Bob's jude
        // pubkey(S_b). The refund transaction spends the output of
        // tx_lock_bitcoin to Bob's refund address.
        // recover(encsign(a, S_b, d), sign(a, d), S_b) = s_b where d is a digest, (a,
        // A) is alice's keypair and (s_b, S_b) is bob's keypair.
        let tx_refund_encsig = self.a.encsign(self.S_b_bitcoin, tx_refund.digest());

        let tx_cancel_sig = self.a.sign(tx_cancel.digest());
        Message3 {
            tx_refund_encsig,
            tx_cancel_sig,
        }
    }

    pub fn receive(self, msg: Message4) -> Result<State3> {
        let tx_cancel =
            bitcoin::TxCancel::new(&self.tx_lock, self.cancel_timelock, self.a.public(), self.B);
        bitcoin::verify_sig(&self.B, &tx_cancel.digest(), &msg.tx_cancel_sig)
            .context("Failed to verify cancel transaction")?;
        let tx_punish =
            bitcoin::TxPunish::new(&tx_cancel, &self.punish_address, self.punish_timelock);
        bitcoin::verify_sig(&self.B, &tx_punish.digest(), &msg.tx_punish_sig)
            .context("Failed to verify punish transaction")?;

        Ok(State3 {
            a: self.a,
            B: self.B,
            s_a: self.s_a,
            S_b_jude: self.S_b_jude,
            S_b_bitcoin: self.S_b_bitcoin,
            v: self.v,
            btc: self.btc,
            jude: self.jude,
            cancel_timelock: self.cancel_timelock,
            punish_timelock: self.punish_timelock,
            refund_address: self.refund_address,
            redeem_address: self.redeem_address,
            punish_address: self.punish_address,
            tx_lock: self.tx_lock,
            tx_punish_sig_bob: msg.tx_punish_sig,
            tx_cancel_sig_bob: msg.tx_cancel_sig,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct State3 {
    pub a: bitcoin::SecretKey,
    pub B: bitcoin::PublicKey,
    pub s_a: jude::Scalar,
    pub S_b_jude: jude::PublicKey,
    pub S_b_bitcoin: bitcoin::PublicKey,
    pub v: jude::PrivateViewKey,
    #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
    pub btc: bitcoin::Amount,
    pub jude: jude::Amount,
    pub cancel_timelock: CancelTimelock,
    pub punish_timelock: PunishTimelock,
    pub refund_address: bitcoin::Address,
    pub redeem_address: bitcoin::Address,
    pub punish_address: bitcoin::Address,
    pub tx_lock: bitcoin::TxLock,
    pub tx_punish_sig_bob: bitcoin::Signature,
    pub tx_cancel_sig_bob: bitcoin::Signature,
}

impl State3 {
    pub async fn wait_for_cancel_timelock_to_expire<W>(&self, bitcoin_wallet: &W) -> Result<()>
    where
        W: WatchForRawTransaction + TransactionBlockHeight + GetBlockHeight,
    {
        wait_for_cancel_timelock_to_expire(
            bitcoin_wallet,
            self.cancel_timelock,
            self.tx_lock.txid(),
        )
        .await
    }

    pub async fn expired_timelocks<W>(&self, bitcoin_wallet: &W) -> Result<ExpiredTimelocks>
    where
        W: WatchForRawTransaction + TransactionBlockHeight + GetBlockHeight,
    {
        current_epoch(
            bitcoin_wallet,
            self.cancel_timelock,
            self.punish_timelock,
            self.tx_lock.txid(),
        )
        .await
    }
}
