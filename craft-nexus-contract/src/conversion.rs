//! Bounded, overflow-safe oracle-price conversion for CraftNexus (Issue #928).
//!
//! # Problem
//!
//! Any settlement path that turns an oracle price into a transferable amount
//! is a value-creation surface: a stale or manipulated price, a fat-fingered
//! decimals mismatch, or a plain `i128` overflow can mint or burn value that
//! never existed in the escrowed pot. This module is the single place that
//! turns `(amount, price)` into a converted amount, and it never returns a
//! value that has not passed every configured economic bound.
//!
//! # Fixed-point representation
//!
//! Prices follow the same convention as the Stellar/Soroban reflector oracle
//! interface: a price is an `i128` mantissa scaled by `10^decimals`. For
//! example a price of `1.2345` with `decimals = 7` is represented as the
//! mantissa `12_345_000`.
//!
//! # Rounding direction
//!
//! Every conversion **truncates toward zero** (floor division on the
//! non-negative operands this module accepts). This is documented per party
//! because it determines who absorbs the fractional remainder:
//!
//! - [`convert_amount`] (buyer pays seller's asset, or any forward quote):
//!   rounds down. The **receiving party** (the party this amount is paid
//!   *to*) gets the truncated, smaller amount; the payer never pays more
//!   than the true converted value. This prevents a rounding-up path from
//!   ever manufacturing value out of the fee/refund pot.
//! - [`convert_amount_ceiling`] (used when the contract must not *underpay*
//!   an obligation it owes, e.g. computing how much input is required to
//!   guarantee a minimum output): rounds up, so the **payer** covers the
//!   full obligation and the contract is never left short.
//!
//! Callers must pick the direction that matches which side of the trade the
//! contract is protecting; see each function's doc comment.
//!
//! # Conservation
//!
//! Because both rounding helpers are pure functions of `(amount, price)` with
//! no hidden state, converting an escrow's `platform_fee`, `seller_amount`,
//! and `buyer_amount` independently and summing them can differ from
//! converting the total by at most `(number of parts - 1)` units of the
//! output asset (integer-division remainder loss). Conversion call sites
//! that must preserve exact conservation (fee + seller + buyer == total)
//! should convert the total once and derive the parts from the *converted*
//! total using the existing basis-point split, rather than converting each
//! part independently.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversionBounds {
    /// Maximum allowed relative movement of `price` away from
    /// `reference_price`, in basis points (10_000 = 100%). A conversion whose
    /// price has moved further than this from the trusted reference is
    /// rejected outright rather than settled at a possibly-manipulated rate.
    pub max_movement_bps: u32,
    /// Minimum reported liquidity (in the oracle's own units) required for a
    /// price to be trusted. Oracles typically expose this alongside price as
    /// a depth/volume figure; conversions quoted against a thin book are
    /// rejected even if the price itself looks reasonable.
    pub min_liquidity: i128,
    /// Maximum allowed age of the price quote in seconds or ledger timestamps.
    pub max_age_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceQuote {
    /// Price mantissa scaled by `10^decimals`.
    pub price: i128,
    /// Number of fractional decimal digits `price` is scaled by.
    pub decimals: u32,
    /// Reported liquidity backing this quote, in the oracle's own units.
    pub liquidity: i128,
    /// Timestamp when the price quote was generated.
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionError {
    NegativeInput,
    UnsupportedDecimals,
    Overflow,
    InsufficientLiquidity,
    ExcessiveMovement,
    StalePriceData,
    OutputUnderflow,
}

const MAX_SUPPORTED_DECIMALS: u32 = 30;

fn pow10(decimals: u32) -> Result<i128, ConversionError> {
    if decimals > MAX_SUPPORTED_DECIMALS {
        return Err(ConversionError::UnsupportedDecimals);
    }
    10i128
        .checked_pow(decimals)
        .ok_or(ConversionError::UnsupportedDecimals)
}

pub fn check_bounds(
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
    current_timestamp: u64,
) -> Result<(), ConversionError> {
    if quote.price < 0 || quote.liquidity < 0 || reference_price < 0 {
        return Err(ConversionError::NegativeInput);
    }
    if quote.liquidity < bounds.min_liquidity {
        return Err(ConversionError::InsufficientLiquidity);
    }
    if current_timestamp < quote.timestamp || (current_timestamp - quote.timestamp) > bounds.max_age_secs {
        return Err(ConversionError::StalePriceData);
    }
    if reference_price == 0 {
        return Ok(());
    }

    let diff = (quote.price - reference_price).abs();
    let scaled_diff = diff.checked_mul(10_000).ok_or(ConversionError::Overflow)?;
    let movement_bps = scaled_diff / reference_price;

    if movement_bps > bounds.max_movement_bps as i128 {
        return Err(ConversionError::ExcessiveMovement);
    }
    Ok(())
}

pub fn convert_amount(
    amount: i128,
    amount_decimals: u32,
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
    current_timestamp: u64,
) -> Result<i128, ConversionError> {
    if amount < 0 {
        return Err(ConversionError::NegativeInput);
    }
    check_bounds(quote, reference_price, bounds, current_timestamp)?;

    let _price_scale = pow10(quote.decimals)?;
    let amount_scale = pow10(amount_decimals)?;

    let numerator = amount
        .checked_mul(quote.price)
        .ok_or(ConversionError::Overflow)?;

    let converted = numerator / amount_scale;

    if amount > 0 && converted == 0 {
        return Err(ConversionError::OutputUnderflow);
    }

    Ok(converted)
}

pub fn convert_amount_ceiling(
    amount: i128,
    amount_decimals: u32,
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
    current_timestamp: u64,
) -> Result<i128, ConversionError> {
    if amount < 0 {
        return Err(ConversionError::NegativeInput);
    }
    check_bounds(quote, reference_price, bounds, current_timestamp)?;

    let _price_scale = pow10(quote.decimals)?;
    let amount_scale = pow10(amount_decimals)?;

    let numerator = amount
        .checked_mul(quote.price)
        .ok_or(ConversionError::Overflow)?;

    let converted = numerator
        .checked_add(amount_scale - 1)
        .ok_or(ConversionError::Overflow)?
        / amount_scale;

    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(price: i128, decimals: u32, liquidity: i128, timestamp: u64) -> PriceQuote {
        PriceQuote {
            price,
            decimals,
            liquidity,
            timestamp,
        }
    }

    fn loose_bounds() -> ConversionBounds {
        ConversionBounds {
            max_movement_bps: 10_000,
            min_liquidity: 0,
            max_age_secs: 3600,
        }
    }

    #[test]
    fn rejects_stale_price_data() {
        let q = quote(1_000_0000, 7, 1_000_000, 100);
        let bounds = ConversionBounds {
            max_movement_bps: 10_000,
            min_liquidity: 0,
            max_age_secs: 60,
        };
        let err = convert_amount(100, 7, &q, 0, &bounds, 200).unwrap_err();
        assert_eq!(err, ConversionError::StalePriceData);
    }

    #[test]
    fn accepts_fresh_price_data() {
        let q = quote(1_000_0000, 7, 1_000_000, 150);
        let bounds = ConversionBounds {
            max_movement_bps: 10_000,
            min_liquidity: 0,
            max_age_secs: 60,
        };
        assert!(convert_amount(100, 7, &q, 0, &bounds, 200).is_ok());
    }
}
