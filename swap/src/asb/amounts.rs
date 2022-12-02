use crate::{bitcoin, jude};
use anyhow::{anyhow, Result};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::fmt::{Debug, Display, Formatter};

/// Prices at which 1 jude will be traded, in BTC (jude/BTC pair)
/// The `ask` represents the minimum price in BTC for which we are willing to
/// sell 1 jude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    pub ask: bitcoin::Amount,
}

impl Rate {
    pub const ZERO: Rate = Rate {
        ask: bitcoin::Amount::ZERO,
    };

    // This function takes the quote amount as it is what Bob sends to Alice in the
    // swap request
    pub fn sell_quote(&self, quote: bitcoin::Amount) -> Result<jude::Amount> {
        Self::quote(self.ask, quote)
    }

    fn quote(rate: bitcoin::Amount, quote: bitcoin::Amount) -> Result<jude::Amount> {
        // quote (btc) = rate * base (jude)
        // base = quote / rate

        let quote_in_sats = quote.as_sat();
        let quote_in_btc = Decimal::from(quote_in_sats)
            .checked_div(Decimal::from(bitcoin::Amount::ONE_BTC.as_sat()))
            .ok_or_else(|| anyhow!("division overflow"))?;

        let rate_in_btc = Decimal::from(rate.as_sat())
            .checked_div(Decimal::from(bitcoin::Amount::ONE_BTC.as_sat()))
            .ok_or_else(|| anyhow!("division overflow"))?;

        let base_in_jude = quote_in_btc
            .checked_div(rate_in_btc)
            .ok_or_else(|| anyhow!("division overflow"))?;
        let base_in_piconero = base_in_jude * Decimal::from(jude::Amount::ONE_jude.as_piconero());

        let base_in_piconero = base_in_piconero
            .to_u64()
            .ok_or_else(|| anyhow!("decimal cannot be represented as u64"))?;

        Ok(jude::Amount::from_piconero(base_in_piconero))
    }
}

impl Display for Rate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sell_quote() {
        let rate = Rate {
            ask: bitcoin::Amount::from_btc(0.002_500).unwrap(),
        };

        let btc_amount = bitcoin::Amount::from_btc(2.5).unwrap();

        let jude_amount = rate.sell_quote(btc_amount).unwrap();

        assert_eq!(jude_amount, jude::Amount::from_jude(1000.0).unwrap())
    }
}
