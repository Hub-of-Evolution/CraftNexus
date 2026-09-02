//! Bounded, overflow-safe oracle-price conversion for CraftNexus (Issue #1088).
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

/// Economic guardrails applied to every oracle-driven conversion.
///
/// All fields are caller-supplied so each integration point (escrow
/// settlement, staking valuation, etc.) can configure bounds appropriate to
/// its own asset pair; nothing here is read from contract storage.
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
}

/// A single oracle price observation, already validated for freshness by the
/// caller (this module does not know about ledger timestamps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceQuote {
    /// Price mantissa scaled by `10^decimals`.
    pub price: i128,
    /// Number of fractional decimal digits `price` is scaled by.
    pub decimals: u32,
    /// Reported liquidity backing this quote, in the oracle's own units.
    pub liquidity: i128,
}

/// Errors returned by this module. Callers map these onto the contract's own
/// `Error` enum at the integration boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionError {
    /// `amount`, `price`, or `liquidity` was negative.
    NegativeInput,
    /// `decimals` (on either side of a two-price conversion) exceeded the
    /// supported range, or was large enough that `10^decimals` would not fit
    /// in an `i128`.
    UnsupportedDecimals,
    /// Intermediate or final arithmetic overflowed `i128`.
    Overflow,
    /// `price.liquidity` was below `bounds.min_liquidity`.
    InsufficientLiquidity,
    /// `price` moved further from `reference_price` than
    /// `bounds.max_movement_bps` permits.
    ExcessiveMovement,
    /// The converted output was zero for a strictly positive input, which
    /// would silently destroy value; reject instead of settling for zero.
    OutputUnderflow,
}

/// Largest `decimals` value for which `10^decimals` fits in an `i128`.
/// `i128::MAX` is approximately `1.7e38`, so 38 is the hard ceiling; this
/// module caps at 30 to leave headroom for the amount factor itself.
const MAX_SUPPORTED_DECIMALS: u32 = 30;

fn pow10(decimals: u32) -> Result<i128, ConversionError> {
    if decimals > MAX_SUPPORTED_DECIMALS {
        return Err(ConversionError::UnsupportedDecimals);
    }
    10i128
        .checked_pow(decimals)
        .ok_or(ConversionError::UnsupportedDecimals)
}

/// Rejects a price quote that fails the configured liquidity or maximum
/// movement bounds. Shared by both rounding directions so the bound checks
/// can never drift out of sync with each other.
fn check_bounds(
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
) -> Result<(), ConversionError> {
    if quote.price < 0 || quote.liquidity < 0 || reference_price < 0 {
        return Err(ConversionError::NegativeInput);
    }
    if quote.liquidity < bounds.min_liquidity {
        return Err(ConversionError::InsufficientLiquidity);
    }
    if reference_price == 0 {
        // No reference to compare against (e.g. first-ever quote): movement
        // bound is vacuous, only liquidity is checked.
        return Ok(());
    }

    let diff = (quote.price - reference_price).abs();
    let scaled_diff = diff
        .checked_mul(10_000)
        .ok_or(ConversionError::Overflow)?;
    let movement_bps = scaled_diff / reference_price;

    if movement_bps > bounds.max_movement_bps as i128 {
        return Err(ConversionError::ExcessiveMovement);
    }
    Ok(())
}

/// Converts `amount` (scaled by `amount_decimals`) into the quoted asset
/// using `quote.price` (scaled by `quote.decimals`), rounding the result
/// **down** (truncated toward zero).
///
/// Use this whenever the converted amount is what the contract will pay out
/// to a counterparty (e.g. seller proceeds, buyer refund priced in a
/// different asset): the receiving party gets the floor of the true value,
/// so the contract never pays out more value than it took in.
///
/// `reference_price` is the last trusted price for this asset pair (e.g. the
/// price recorded at escrow creation, or the previous accepted settlement
/// price); pass `0` to skip the movement check for a pair with no prior
/// reference.
///
/// Returns [`ConversionError::OutputUnderflow`] if a strictly positive
/// `amount` converts to zero, since settling for zero would silently forfeit
/// the counterparty's value instead of surfacing the precision problem.
pub fn convert_amount(
    amount: i128,
    amount_decimals: u32,
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
) -> Result<i128, ConversionError> {
    if amount < 0 {
        return Err(ConversionError::NegativeInput);
    }
    check_bounds(quote, reference_price, bounds)?;

    // amount is scaled by 10^amount_decimals and price is scaled by
    // 10^price_decimals, both representing "1.0" of their respective units.
    // converted = amount * price / 10^amount_decimals, which yields a result
    // scaled by 10^price_decimals (the oracle's own output scale) — the same
    // convention the Stellar/Soroban reflector oracle interface uses.
    // `quote.decimals` does not appear in the arithmetic (the output is
    // naturally expressed in that scale) but is still bounds-checked so an
    // oracle report with an out-of-range decimals field is rejected rather
    // than silently accepted.
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

/// Same conversion as [`convert_amount`] but rounds the result **up**
/// (ceiling).
///
/// Use this only when the contract is computing an amount it must collect or
/// reserve to cover an obligation denominated in the other asset (e.g. "how
/// much of token A must be escrowed to guarantee at least X of token B at
/// this price") — rounding up here means the contract, not the counterparty,
/// absorbs the fractional remainder, so it is never left short.
pub fn convert_amount_ceiling(
    amount: i128,
    amount_decimals: u32,
    quote: &PriceQuote,
    reference_price: i128,
    bounds: &ConversionBounds,
) -> Result<i128, ConversionError> {
    if amount < 0 {
        return Err(ConversionError::NegativeInput);
    }
    check_bounds(quote, reference_price, bounds)?;

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

    fn quote(price: i128, decimals: u32, liquidity: i128) -> PriceQuote {
        PriceQuote {
            price,
            decimals,
            liquidity,
        }
    }

    fn loose_bounds() -> ConversionBounds {
        ConversionBounds {
            max_movement_bps: 10_000, // 100%, effectively no cap
            min_liquidity: 0,
        }
    }

    #[test]
    fn converts_equal_decimals_one_to_one_price() {
        let q = quote(1_000_0000, 7, 1_000_000); // price = 1.0 at 7 decimals
        let out = convert_amount(500_0000, 7, &q, 0, &loose_bounds()).unwrap();
        assert_eq!(out, 500_0000);
    }

    #[test]
    fn floor_rounds_down_for_receiving_party() {
        // price = 0.3333333 (7 decimals), amount = 10 (7 decimals) => 3.3333333 -> floor 3
        let q = quote(3_333_333, 7, 1_000_000);
        let out = convert_amount(10_0000000, 7, &q, 0, &loose_bounds()).unwrap();
        assert_eq!(out, 3_333_3330);
    }

    #[test]
    fn ceiling_rounds_up_for_payer_obligation() {
        let q = quote(3_333_333, 7, 1_000_000);
        let out = convert_amount_ceiling(1, 7, &q, 0, &loose_bounds()).unwrap();
        // A tiny amount that floors to 0 must ceiling to at least 1.
        assert!(out >= 1);
    }

    #[test]
    fn rejects_negative_amount() {
        let q = quote(1_0000000, 7, 1_000_000);
        let err = convert_amount(-1, 7, &q, 0, &loose_bounds()).unwrap_err();
        assert_eq!(err, ConversionError::NegativeInput);
    }

    #[test]
    fn rejects_negative_price() {
        let q = quote(-1, 7, 1_000_000);
        let err = convert_amount(100, 7, &q, 0, &loose_bounds()).unwrap_err();
        assert_eq!(err, ConversionError::NegativeInput);
    }

    #[test]
    fn rejects_below_minimum_liquidity() {
        let q = quote(1_0000000, 7, 5);
        let bounds = ConversionBounds {
            max_movement_bps: 10_000,
            min_liquidity: 1_000,
        };
        let err = convert_amount(100, 7, &q, 0, &bounds).unwrap_err();
        assert_eq!(err, ConversionError::InsufficientLiquidity);
    }

    #[test]
    fn rejects_excessive_movement_above_reference() {
        let reference_price = 1_000_0000; // 1.0
        let q = quote(1_200_0000, 7, 1_000_000); // 1.2, +20%
        let bounds = ConversionBounds {
            max_movement_bps: 500, // 5% max
            min_liquidity: 0,
        };
        let err = convert_amount(100, 7, &q, reference_price, &bounds).unwrap_err();
        assert_eq!(err, ConversionError::ExcessiveMovement);
    }

    #[test]
    fn rejects_excessive_movement_below_reference() {
        let reference_price = 1_000_0000;
        let q = quote(700_0000, 7, 1_000_000); // -30%
        let bounds = ConversionBounds {
            max_movement_bps: 500,
            min_liquidity: 0,
        };
        let err = convert_amount(100, 7, &q, reference_price, &bounds).unwrap_err();
        assert_eq!(err, ConversionError::ExcessiveMovement);
    }

    #[test]
    fn accepts_movement_within_bound() {
        let reference_price = 1_000_0000;
        let q = quote(1_040_0000, 7, 1_000_000); // +4%
        let bounds = ConversionBounds {
            max_movement_bps: 500,
            min_liquidity: 0,
        };
        assert!(convert_amount(1_0000000, 7, &q, reference_price, &bounds).is_ok());
    }

    #[test]
    fn zero_reference_price_skips_movement_check() {
        let q = quote(1_000_0000, 7, 1_000_000);
        let bounds = ConversionBounds {
            max_movement_bps: 1,
            min_liquidity: 0,
        };
        assert!(convert_amount(100, 7, &q, 0, &bounds).is_ok());
    }

    #[test]
    fn rejects_output_underflow_for_dust_amount() {
        // price so small that a tiny amount floors to zero.
        let q = quote(1, 7, 1_000_000); // price = 0.0000001
        let err = convert_amount(1, 7, &q, 0, &loose_bounds()).unwrap_err();
        assert_eq!(err, ConversionError::OutputUnderflow);
    }

    #[test]
    fn zero_amount_converts_to_zero_without_error() {
        let q = quote(1_0000000, 7, 1_000_000);
        let out = convert_amount(0, 7, &q, 0, &loose_bounds()).unwrap();
        assert_eq!(out, 0);
    }

    #[test]
    fn rejects_unsupported_decimals() {
        let q = quote(1_0000000, 40, 1_000_000);
        let err = convert_amount(100, 7, &q, 0, &loose_bounds()).unwrap_err();
        assert_eq!(err, ConversionError::UnsupportedDecimals);
    }

    #[test]
    fn overflow_is_rejected_not_wrapped() {
        let q = quote(i128::MAX, 0, 1_000_000);
        let err = convert_amount(i128::MAX, 0, &q, 0, &loose_bounds()).unwrap_err();
        assert_eq!(err, ConversionError::Overflow);
    }

    #[test]
    fn cross_decimals_conversion_normalizes_correctly() {
        // amount has 2 decimals (e.g. cents), price has 7 decimals.
        // amount = 100 units at 2 decimals = 1.00 "dollars"
        // price = 2.5000000 at 7 decimals ("1.00 dollars" = 2.5 of the quoted asset)
        // expected output, scaled by the price's 7 decimals, is 2.5000000.
        let q = quote(2_500_0000, 7, 1_000_000);
        let out = convert_amount(100, 2, &q, 0, &loose_bounds()).unwrap();
        assert_eq!(out, 2_500_0000);
    }

    #[test]
    fn floor_and_ceiling_agree_on_exact_division() {
        let q = quote(2_0000000, 7, 1_000_000); // price = 2.0 exactly
        let floor = convert_amount(10_0000000, 7, &q, 0, &loose_bounds()).unwrap();
        let ceil = convert_amount_ceiling(10_0000000, 7, &q, 0, &loose_bounds()).unwrap();
        assert_eq!(floor, ceil);
    }

    #[test]
    fn ceiling_never_less_than_floor() {
        let q = quote(3_333_333, 7, 1_000_000);
        let floor = convert_amount(10_0000001, 7, &q, 0, &loose_bounds()).unwrap();
        let ceil = convert_amount_ceiling(10_0000001, 7, &q, 0, &loose_bounds()).unwrap();
        assert!(ceil >= floor);
    }
}
