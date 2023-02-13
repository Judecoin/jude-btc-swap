pub mod wallet;
mod wallet_rpc;

pub use ::jude::network::Network;
pub use ::jude::{Address, PrivateKey, PublicKey};
pub use curve25519_dalek::scalar::Scalar;
pub use wallet::Wallet;
pub use wallet_rpc::{WalletRpc, WalletRpcProcess};

use crate::bitcoin;
use anyhow::Result;
use rand::{CryptoRng, RngCore};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::ops::{Add, Mul, Sub};
use std::str::FromStr;

pub const PICONERO_OFFSET: u64 = 1_000_000_000_000;

#[derive(Serialize, Deserialize)]
#[serde(remote = "Network")]
#[allow(non_camel_case_types)]
pub enum network {
    Mainnet,
    Stagenet,
    Testnet,
}

pub fn private_key_from_secp256k1_scalar(scalar: bitcoin::Scalar) -> PrivateKey {
    let mut bytes = scalar.to_bytes();

    // we must reverse the bytes because a secp256k1 scalar is big endian, whereas a
    // ed25519 scalar is little endian
    bytes.reverse();

    PrivateKey::from_scalar(Scalar::from_bytes_mod_order(bytes))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrivateViewKey(#[serde(with = "jude_private_key")] PrivateKey);

impl PrivateViewKey {
    pub fn new_random<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let scalar = Scalar::random(rng);
        let private_key = PrivateKey::from_scalar(scalar);

        Self(private_key)
    }

    pub fn public(&self) -> PublicViewKey {
        PublicViewKey(PublicKey::from_private_key(&self.0))
    }
}

impl Add for PrivateViewKey {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl From<PrivateViewKey> for PrivateKey {
    fn from(from: PrivateViewKey) -> Self {
        from.0
    }
}

impl From<PublicViewKey> for PublicKey {
    fn from(from: PublicViewKey) -> Self {
        from.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PublicViewKey(PublicKey);

#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq, PartialOrd)]
pub struct Amount(u64);

// Median tx fees on Jude as found here: https://www.jude.how/jude-transaction-fees, JUDE 0.000_008 * 2 (to be on the safe side)
pub const JUDE_FEE: Amount = Amount::from_piconero(16_000_000);

impl Amount {
    pub const ZERO: Self = Self(0);
    pub const ONE_JUDE: Self = Self(PICONERO_OFFSET);
    /// Create an [Amount] with piconero precision and the given number of
    /// piconeros.
    ///
    /// A piconero (a.k.a atomic unit) is equal to 1e-12 JUDE.
    pub const fn from_piconero(amount: u64) -> Self {
        Amount(amount)
    }

    /// Return Jude Amount as Piconero.
    pub fn as_piconero(&self) -> u64 {
        self.0
    }

    /// Calculate the maximum amount of Bitcoin that can be bought at a given
    /// asking price for this amount of Jude including the median fee.
    pub fn max_bitcoin_for_price(&self, ask_price: bitcoin::Amount) -> Option<bitcoin::Amount> {
        let pico_minus_fee = self.as_piconero().saturating_sub(JUDE_FEE.as_piconero());

        if pico_minus_fee == 0 {
            return Some(bitcoin::Amount::ZERO);
        }

        // safely convert the BTC/JUDE rate to sat/pico
        let ask_sats = Decimal::from(ask_price.to_sat());
        let pico_per_jude = Decimal::from(PICONERO_OFFSET);
        let ask_sats_per_pico = ask_sats / pico_per_jude;

        let pico = Decimal::from(pico_minus_fee);
        let max_sats = pico.checked_mul(ask_sats_per_pico)?;
        let satoshi = max_sats.to_u64()?;

        Some(bitcoin::Amount::from_sat(satoshi))
    }

    pub fn from_jude(amount: f64) -> Result<Self> {
        let decimal = Decimal::try_from(amount)?;
        Self::from_decimal(decimal)
    }

    pub fn parse_jude(amount: &str) -> Result<Self> {
        let decimal = Decimal::from_str(amount)?;
        Self::from_decimal(decimal)
    }

    pub fn as_piconero_decimal(&self) -> Decimal {
        Decimal::from(self.as_piconero())
    }

    fn from_decimal(amount: Decimal) -> Result<Self> {
        let piconeros_dec =
            amount.mul(Decimal::from_u64(PICONERO_OFFSET).expect("constant to fit into u64"));
        let piconeros = piconeros_dec
            .to_u64()
            .ok_or_else(|| OverflowError(amount.to_string()))?;
        Ok(Amount(piconeros))
    }
}

impl Add for Amount {
    type Output = Amount;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Amount {
    type Output = Amount;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul<u64> for Amount {
    type Output = Amount;

    fn mul(self, rhs: u64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl From<Amount> for u64 {
    fn from(from: Amount) -> u64 {
        from.0
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut decimal = Decimal::from(self.0);
        decimal
            .set_scale(12)
            .expect("12 is smaller than max precision of 28");
        write!(f, "{} JUDE", decimal)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransferProof {
    tx_hash: TxHash,
    #[serde(with = "jude_private_key")]
    tx_key: PrivateKey,
}

impl TransferProof {
    pub fn new(tx_hash: TxHash, tx_key: PrivateKey) -> Self {
        Self { tx_hash, tx_key }
    }
    pub fn tx_hash(&self) -> TxHash {
        self.tx_hash.clone()
    }
    pub fn tx_key(&self) -> PrivateKey {
        self.tx_key
    }
}

// TODO: add constructor/ change String to fixed length byte array
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TxHash(pub String);

impl From<TxHash> for String {
    fn from(from: TxHash) -> Self {
        from.0
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("expected {expected}, got {actual}")]
pub struct InsufficientFunds {
    pub expected: Amount,
    pub actual: Amount,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq)]
#[error("Overflow, cannot convert {0} to u64")]
pub struct OverflowError(pub String);

pub mod jude_private_key {
    use jude::consensus::{Decodable, Encodable};
    use jude::PrivateKey;
    use serde::de::Visitor;
    use serde::ser::Error;
    use serde::{de, Deserializer, Serializer};
    use std::fmt;
    use std::io::Cursor;

    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = PrivateKey;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a byte array representing a Jude private key")
        }

        fn visit_bytes<E>(self, s: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut s = s;
            PrivateKey::consensus_decode(&mut s).map_err(|err| E::custom(format!("{:?}", err)))
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let bytes = hex::decode(s).map_err(|err| E::custom(format!("{:?}", err)))?;
            PrivateKey::consensus_decode(&mut bytes.as_slice())
                .map_err(|err| E::custom(format!("{:?}", err)))
        }
    }

    pub fn serialize<S>(x: &PrivateKey, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = Cursor::new(vec![]);
        x.consensus_encode(&mut bytes)
            .map_err(|err| S::Error::custom(format!("{:?}", err)))?;
        if s.is_human_readable() {
            s.serialize_str(&hex::encode(bytes.into_inner()))
        } else {
            s.serialize_bytes(bytes.into_inner().as_ref())
        }
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<PrivateKey, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let key = {
            if deserializer.is_human_readable() {
                deserializer.deserialize_string(BytesVisitor)?
            } else {
                deserializer.deserialize_bytes(BytesVisitor)?
            }
        };
        Ok(key)
    }
}

pub mod jude_amount {
    use crate::jude::Amount;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(x: &Amount, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_u64(x.as_piconero())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Amount, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let picos = u64::deserialize(deserializer)?;
        let amount = Amount::from_piconero(picos);

        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_jude_min() {
        let min_pics = 1;
        let amount = Amount::from_piconero(min_pics);
        let jude = amount.to_string();
        assert_eq!("0.000000000001 JUDE", jude);
    }

    #[test]
    fn display_jude_one() {
        let min_pics = 1000000000000;
        let amount = Amount::from_piconero(min_pics);
        let jude = amount.to_string();
        assert_eq!("1.000000000000 JUDE", jude);
    }

    #[test]
    fn display_jude_max() {
        let max_pics = 18_446_744_073_709_551_615;
        let amount = Amount::from_piconero(max_pics);
        let jude = amount.to_string();
        assert_eq!("18446744.073709551615 JUDE", jude);
    }

    #[test]
    fn parse_jude_min() {
        let jude_min = "0.000000000001";
        let amount = Amount::parse_jude(jude_min).unwrap();
        let pics = amount.0;
        assert_eq!(1, pics);
    }

    #[test]
    fn parse_jude() {
        let jude = "123";
        let amount = Amount::parse_jude(jude).unwrap();
        let pics = amount.0;
        assert_eq!(123000000000000, pics);
    }

    #[test]
    fn parse_jude_max() {
        let jude = "18446744.073709551615";
        let amount = Amount::parse_jude(jude).unwrap();
        let pics = amount.0;
        assert_eq!(18446744073709551615, pics);
    }

    #[test]
    fn parse_jude_overflows() {
        let overflow_pics = "18446744.073709551616";
        let error = Amount::parse_jude(overflow_pics).unwrap_err();
        assert_eq!(
            error.downcast_ref::<OverflowError>().unwrap(),
            &OverflowError(overflow_pics.to_owned())
        );
    }

    #[test]
    fn max_bitcoin_to_trade() {
        // sanity check: if the asking price is 1 BTC / 1 JUDE
        // and we have μ JUDE + fee
        // then max BTC we can buy is μ
        let ask = bitcoin::Amount::from_btc(1.0).unwrap();

        let jude = Amount::parse_jude("1.0").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(1.0).unwrap());

        let jude = Amount::parse_jude("0.5").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(0.5).unwrap());

        let jude = Amount::parse_jude("2.5").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(2.5).unwrap());

        let jude = Amount::parse_jude("420").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(420.0).unwrap());

        let jude = Amount::parse_jude("0.00001").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(0.00001).unwrap());

        // other ask prices

        let ask = bitcoin::Amount::from_btc(0.5).unwrap();
        let jude = Amount::parse_jude("2").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(1.0).unwrap());

        let ask = bitcoin::Amount::from_btc(2.0).unwrap();
        let jude = Amount::parse_jude("1").unwrap() + JUDE_FEE;
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_btc(2.0).unwrap());

        let ask = bitcoin::Amount::from_sat(382_900);
        let jude = Amount::parse_jude("10").unwrap();
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_sat(3_828_993));

        // example from https://github.com/comit-network/jude-btc-swap/issues/1084
        // with rate from kraken at that time
        let ask = bitcoin::Amount::from_sat(685_800);
        let jude = Amount::parse_jude("0.826286435921").unwrap();
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(btc, bitcoin::Amount::from_sat(566_656));
    }

    #[test]
    fn max_bitcoin_to_trade_overflow() {
        let jude = Amount::from_jude(30.0).unwrap();
        let ask = bitcoin::Amount::from_sat(728_688);
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(bitcoin::Amount::from_sat(21_860_628), btc);

        let jude = Amount::from_piconero(u64::MAX);
        let ask = bitcoin::Amount::from_sat(u64::MAX);
        let btc = jude.max_bitcoin_for_price(ask);

        assert!(btc.is_none());
    }

    #[test]
    fn geting_max_bitcoin_to_trade_with_balance_smaller_than_locking_fee() {
        let ask = bitcoin::Amount::from_sat(382_900);
        let jude = Amount::parse_jude("0.00001").unwrap();
        let btc = jude.max_bitcoin_for_price(ask).unwrap();

        assert_eq!(bitcoin::Amount::ZERO, btc);
    }

    use rand::rngs::OsRng;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct JudePrivateKey(#[serde(with = "jude_private_key")] crate::jude::PrivateKey);

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct JudeAmount(#[serde(with = "jude_amount")] crate::jude::Amount);

    #[test]
    fn serde_jude_private_key_json() {
        let key = JudePrivateKey(jude::PrivateKey::from_scalar(
            crate::jude::Scalar::random(&mut OsRng),
        ));
        let encoded = serde_json::to_vec(&key).unwrap();
        let decoded: JudePrivateKey = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn serde_jude_private_key_cbor() {
        let key = JudePrivateKey(jude::PrivateKey::from_scalar(
            crate::jude::Scalar::random(&mut OsRng),
        ));
        let encoded = serde_cbor::to_vec(&key).unwrap();
        let decoded: JudePrivateKey = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn serde_jude_amount() {
        let amount = JudeAmount(crate::jude::Amount::from_piconero(1000));
        let encoded = serde_cbor::to_vec(&amount).unwrap();
        let decoded: JudeAmount = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(amount, decoded);
    }
}
