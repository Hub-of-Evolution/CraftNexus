//! Price-oracle validation and currency-conversion guardrails for CraftNexus.
//!
//! # Purpose
//!
//! Fee and settlement calculations that depend on external pricing must never
//! silently consume bad data. This module is the single source of truth for
//! the three guardrails enforced by the contract:
//!
//! 1. **Malformed-data rejection** — a price feed must be strictly positive and
//!    use a supported decimal scale ([`validate_price_feed`]).
//! 2. **Stale-data rejection** — a feed whose timestamp is in the future
//!    (malformed) or older than the configured `max_staleness` window is
//!    unusable ([`is_price_stale`]).
//! 3. **Explicit conversion bounds** — conversions are deterministic
//!    ([`convert_amount`]) and any observed/executed rate must stay within a
//!    configured deviation band of the oracle reference ([`within_deviation`]).
//!
//! All arithmetic is pure, `no_std`, overflow-checked, and rounds toward zero,
//! so the same inputs always produce the same output (deterministic fee and
//! settlement math for the same contract state).
//!
//! # Price model
//!
//! A `PriceFeed` stores `price` as the value of **one whole token** in
//! reference units (e.g. USD), scaled by `decimals`, where `decimals` also
//! denotes the token's own decimal scale (the standard oracle convention):
//!
//! ```text
//! one_whole_token_in_base = price / 10^decimals
//! ```
//!
//! So `price = 100_000_000, decimals = 8` means one whole token is worth
//! `1.00000000` base units. Because both scale factors are `10^decimals`, the
//! conversion of `amount` smallest units cancels them algebraically and
//! reduces to a single integer division (see [`convert_amount`]).
//!
//! The contract stamps `timestamp` itself at publish time, so a feed cannot
//! claim a freshness it never actually had.

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum supported price-feed decimal scale (matches token decimal limits).
pub const MAX_PRICE_DECIMALS: u32 = 18;

/// Default staleness window (1 hour). Feeds older than this are rejected.
pub const DEFAULT_MAX_STALENESS: u64 = 3600;

/// Default maximum deviation between an observed conversion and the oracle
/// reference (500 bps = 5%).
pub const DEFAULT_MAX_DEVIATION_BPS: u32 = 500;

/// Absolute ceiling for the configurable deviation band (10_000 bps = 100%).
/// Anything wider would make the guardrail meaningless.
pub const MAX_DEVIATION_BPS_CEILING: u32 = 10_000;

/// Reason a price feed failed validation. Mapped to `Error::InvalidPriceData`
/// at the contract boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FeedValidationError {
    /// Price must be strictly positive.
    ZeroOrNegativePrice,
    /// Decimal scale outside `0..=MAX_PRICE_DECIMALS`.
    DecimalsOutOfRange,
}

// ── Feed validation (malformed-data rejection) ───────────────────────────────

/// Validates the static shape of a price feed.
///
/// Rejects zero/negative prices (a real asset can never be worth zero or less)
/// and decimal scales outside the supported range. A malformed feed must never
/// enter storage — callers reject it at the boundary.
#[inline]
pub fn validate_price_feed(price: i128, decimals: u32) -> Result<(), FeedValidationError> {
    if price <= 0 {
        return Err(FeedValidationError::ZeroOrNegativePrice);
    }
    if decimals > MAX_PRICE_DECIMALS {
        return Err(FeedValidationError::DecimalsOutOfRange);
    }
    Ok(())
}

// ── Staleness (stale-data rejection) ─────────────────────────────────────────

/// Returns `true` if a feed stamped at `timestamp` is unusable at ledger time
/// `now` given a `max_staleness` window (in seconds).
///
/// A feed is stale when:
/// * its timestamp is in the future (`timestamp > now`) — malformed clock data,
///   or
/// * it is older than `max_staleness` seconds (`now - timestamp > max_staleness`).
///
/// The boundary `now == timestamp + max_staleness` is still **fresh**
/// (inclusive-end convention, matching `time_policy`).
#[inline]
pub fn is_price_stale(now: u64, timestamp: u64, max_staleness: u64) -> bool {
    timestamp > now || now.saturating_sub(timestamp) > max_staleness
}

// ── Conversion (deterministic, bounded) ──────────────────────────────────────

/// `10^n`. Internal conversions may need up to `2 * MAX_PRICE_DECIMALS`
/// (10^36), which still fits comfortably in `i128` (~1.7e38).
#[inline]
pub fn pow10(n: u32) -> i128 {
    let mut result: i128 = 1;
    for _ in 0..n {
        result *= 10;
    }
    result
}

/// Convert `amount` (in the `from` token's smallest units) into the `to`
/// token's smallest units using two price feeds.
///
/// # Price model
///
/// `price` is the value of **one whole token** in reference units, scaled by
/// `decimals`, and `decimals` also denotes the token's own decimal scale
/// (the standard oracle convention: a token priced `1.0` at `decimals = 18`
/// has `price = 10^18`). Under that convention the two `10^decimals` scale
/// factors cancel algebraically, so the conversion reduces to a **single
/// integer division**:
///
/// ```text
/// output = amount * from_price / to_price              (equal decimals)
/// output = amount * from_price * 10^(2*d) / to_price   (to has d more decimals)
/// output = amount * from_price / (to_price * 10^(2*d)) (from has d more decimals)
/// ```
///
/// Performing the division once, at the end, preserves precision for small
/// values (e.g. sub-unit fees) instead of truncating an intermediate result.
///
/// # Determinism
///
/// The function is pure: identical inputs (same amount, same feeds) always
/// produce the identical output. Rounding is toward zero (floor for the
/// positive values enforced here), never banker's rounding, so the result is
/// stable across invocations and callers.
///
/// # Errors
///
/// Returns `None` (never panics, never wraps) when:
/// * `amount` is negative,
/// * either feed fails [`validate_price_feed`], or
/// * the intermediate arithmetic overflows `i128` — the caller must reject
///   (`Error::ConversionOutOfBounds`) rather than use a truncated value.
#[inline]
pub fn convert_amount(
    amount: i128,
    from_price: i128,
    from_decimals: u32,
    to_price: i128,
    to_decimals: u32,
) -> Option<i128> {
    if amount < 0 {
        return None;
    }
    if validate_price_feed(from_price, from_decimals).is_err() {
        return None;
    }
    if validate_price_feed(to_price, to_decimals).is_err() {
        return None;
    }

    let from_scale2 = 2 * from_decimals;
    let to_scale2 = 2 * to_decimals;

    // Multiply through first so the only rounding happens in the final
    // division; the decimal-scale powers cancel algebraically for equal
    // decimals and adjust the exponent otherwise.
    let numerator = amount.checked_mul(from_price)?;
    if to_scale2 >= from_scale2 {
        numerator
            .checked_mul(pow10(to_scale2 - from_scale2))?
            .checked_div(to_price)
    } else {
        let denominator = to_price.checked_mul(pow10(from_scale2 - to_scale2))?;
        numerator.checked_div(denominator)
    }
}

/// Convert `amount` (smallest units of a token with `from_token_decimals`)
/// using an externally observed rate: **one whole from-token equals
/// `rate / 10^rate_decimals` whole to-tokens**. Returns smallest units of the
/// to-token (which has `to_token_decimals` decimals).
///
/// ```text
/// output = amount * rate * 10^(to_dec - from_dec - rate_dec)
/// ```
///
/// The single trailing division keeps rounding deterministic (toward zero).
/// Returns `None` on negative input, malformed rate, or `i128` overflow.
#[inline]
pub fn convert_with_rate(
    amount: i128,
    from_token_decimals: u32,
    rate: i128,
    rate_decimals: u32,
    to_token_decimals: u32,
) -> Option<i128> {
    if amount < 0 {
        return None;
    }
    if validate_price_feed(rate, rate_decimals).is_err() {
        return None;
    }
    let exponent = to_token_decimals as i32 - from_token_decimals as i32 - rate_decimals as i32;
    let numerator = amount.checked_mul(rate)?;
    if exponent >= 0 {
        numerator.checked_mul(pow10(exponent as u32))
    } else {
        numerator.checked_div(pow10((-exponent) as u32))
    }
}

// ── Deviation band (explicit conversion bounds) ──────────────────────────────

/// Absolute deviation of `observed` from `reference`, in basis points relative
/// to `reference`. `None` when `reference <= 0` or the arithmetic overflows.
#[inline]
pub fn deviation_bps(observed: i128, reference: i128) -> Option<u32> {
    if reference <= 0 {
        return None;
    }
    let diff = observed.checked_sub(reference)?.unsigned_abs();
    let dev = diff
        .checked_mul(10_000u128)?
        .checked_div(reference.unsigned_abs())?;
    u32::try_from(dev).ok()
}

/// Returns `true` if `observed` deviates from the `reference` conversion by at
/// most `max_deviation_bps` basis points (e.g. 500 = 5%).
///
/// This is the "conversion outputs remain within configured bounds" guardrail:
/// any externally observed rate (DEX execution, market quote) used in a fee or
/// settlement calculation must stay inside the configured band around the
/// oracle reference, otherwise the calculation must be rejected.
#[inline]
pub fn within_deviation(observed: i128, reference: i128, max_deviation_bps: u32) -> bool {
    match deviation_bps(observed, reference) {
        Some(deviation) => deviation <= max_deviation_bps,
        None => false,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_price_feed ───────────────────────────────────────────────

    #[test]
    fn valid_feed_accepted() {
        assert_eq!(validate_price_feed(1, 0), Ok(()));
        assert_eq!(validate_price_feed(1_000_000, 6), Ok(()));
        assert_eq!(validate_price_feed(i128::MAX, 18), Ok(()));
    }

    #[test]
    fn zero_or_negative_price_rejected() {
        assert_eq!(
            validate_price_feed(0, 18),
            Err(FeedValidationError::ZeroOrNegativePrice)
        );
        assert_eq!(
            validate_price_feed(-1, 18),
            Err(FeedValidationError::ZeroOrNegativePrice)
        );
        assert_eq!(
            validate_price_feed(i128::MIN, 0),
            Err(FeedValidationError::ZeroOrNegativePrice)
        );
    }

    #[test]
    fn decimals_out_of_range_rejected() {
        assert_eq!(
            validate_price_feed(100, 19),
            Err(FeedValidationError::DecimalsOutOfRange)
        );
        assert_eq!(
            validate_price_feed(100, u32::MAX),
            Err(FeedValidationError::DecimalsOutOfRange)
        );
    }

    // ── is_price_stale ────────────────────────────────────────────────────

    #[test]
    fn fresh_feed_is_not_stale() {
        let now = 1_000_000u64;
        let window = 3600u64;
        // stamped exactly `window` seconds ago → still fresh (inclusive end)
        assert!(!is_price_stale(now, now - window, window));
        assert!(!is_price_stale(now, now, window));
        assert!(!is_price_stale(now, now - 1, window));
    }

    #[test]
    fn old_feed_is_stale() {
        let now = 1_000_000u64;
        let window = 3600u64;
        // stamped `window + 1` seconds ago → stale
        assert!(is_price_stale(now, now - window - 1, window));
        assert!(is_price_stale(now, 0, window));
    }

    #[test]
    fn future_timestamp_is_stale() {
        // A feed stamped in the future is malformed clock data → stale.
        assert!(is_price_stale(1_000_000, 1_000_001, 3600));
        assert!(is_price_stale(1_000_000, u64::MAX, 3600));
    }

    #[test]
    fn zero_staleness_rejects_everything_but_now() {
        let now = 123u64;
        assert!(!is_price_stale(now, now, 0));
        assert!(is_price_stale(now, now - 1, 0));
        assert!(is_price_stale(now, now + 1, 0));
    }

    #[test]
    fn staleness_never_overflows() {
        // now near u64::MAX, timestamp near 0 → saturating math must not panic
        assert!(is_price_stale(u64::MAX, 0, 3600)); // very old → stale
        // stamped one second ago → fresh (no wrap-around on the subtraction)
        assert!(!is_price_stale(u64::MAX, u64::MAX - 1, 3600));
    }

    // ── convert_amount ────────────────────────────────────────────────────

    #[test]
    fn converts_same_decimals() {
        // both feeds priced at 1.0 → output == input
        let out = convert_amount(1_000_000, 10i128.pow(7), 7, 10i128.pow(7), 7);
        assert_eq!(out, Some(1_000_000));
    }

    #[test]
    fn converts_across_decimal_scales() {
        // 6-dec token priced 1.0 (10^6), 18-dec token priced 1.0 (10^18)
        // 1.0 of the 6-dec token == 10^12 smallest units of the 18-dec token
        let out = convert_amount(1_000_000, 10i128.pow(6), 6, 10i128.pow(18), 18);
        assert_eq!(out, Some(1_000_000_000_000_000_000)); // 10^18
    }

    #[test]
    fn converts_with_price_ratio() {
        // from priced at 2.0 (2 * 10^6), to priced at 1.0 (10^18)
        // 1.0 from-token == 2.0 base == 2 * 10^18 to-units
        let out = convert_amount(1_000_000, 2 * 10i128.pow(6), 6, 10i128.pow(18), 18);
        assert_eq!(out, Some(2_000_000_000_000_000_000));
    }

    #[test]
    fn converts_reverse_decimal_scales() {
        // 18-dec token priced 1.0 (10^18), 6-dec token priced 1.0 (10^6)
        // 1.0 of the 18-dec token == 10^6 smallest units of the 6-dec token
        let out = convert_amount(10i128.pow(18), 10i128.pow(18), 18, 10i128.pow(6), 6);
        assert_eq!(out, Some(1_000_000));
        // A single wei is far below one 6-dec unit → rounds to zero
        assert_eq!(convert_amount(1, 10i128.pow(18), 18, 10i128.pow(6), 6), Some(0));
    }

    #[test]
    fn rounds_toward_zero_deterministically() {
        // 1 wei of from-token priced 10^-9 base → value below 1 minor unit of
        // an 18-dec token priced 1.0 → rounds to 0 (floor), never negative.
        let out = convert_amount(1, 10i128.pow(9), 18, 10i128.pow(18), 18);
        assert_eq!(out, Some(0));

        // Same inputs → same output, every time.
        for _ in 0..5 {
            assert_eq!(
                convert_amount(1, 10i128.pow(9), 18, 10i128.pow(18), 18),
                Some(0)
            );
        }
    }

    #[test]
    fn negative_amount_rejected() {
        assert_eq!(convert_amount(-5, 10i128.pow(6), 6, 10i128.pow(18), 18), None);
    }

    #[test]
    fn invalid_feeds_rejected() {
        assert_eq!(convert_amount(100, 0, 6, 10i128.pow(18), 18), None);
        assert_eq!(convert_amount(100, -10, 6, 10i128.pow(18), 18), None);
        assert_eq!(convert_amount(100, 10, 19, 10i128.pow(18), 18), None);
        assert_eq!(convert_amount(100, 10i128.pow(6), 6, 0, 18), None);
        assert_eq!(convert_amount(100, 10i128.pow(6), 6, 10, 19), None);
    }

    #[test]
    fn overflow_rejected_not_wrapped() {
        // i128::MAX * price overflows → None (caller must reject)
        assert_eq!(
            convert_amount(i128::MAX, 10i128.pow(18), 18, 10i128.pow(18), 18),
            None
        );
    }

    #[test]
    fn pow10_matches_expectations() {
        assert_eq!(pow10(0), 1);
        assert_eq!(pow10(6), 1_000_000);
        assert_eq!(pow10(18), 1_000_000_000_000_000_000);
    }

    // ── convert_with_rate ──────────────────────────────────────────────────

    #[test]
    fn rate_conversion_same_decimals() {
        // 0.1 whole from-token at a 0.5 rate → 0.05 whole to-token.
        let out = convert_with_rate(1_000_000, 7, 5_000_000, 7, 7);
        assert_eq!(out, Some(500_000));
    }

    #[test]
    fn rate_conversion_cross_decimals() {
        // 1.0 (10^18 smallest units) of an 18-dec token at rate 1.0 → 10^6
        // smallest units of a 6-dec token.
        let out = convert_with_rate(10i128.pow(18), 18, 10i128.pow(7), 7, 6);
        assert_eq!(out, Some(1_000_000));
    }

    #[test]
    fn rate_conversion_rejects_invalid_input() {
        assert_eq!(convert_with_rate(-1, 7, 5_000_000, 7, 7), None);
        assert_eq!(convert_with_rate(1_000_000, 7, 0, 7, 7), None);
        assert_eq!(convert_with_rate(1_000_000, 7, 5_000_000, 19, 7), None);
        assert_eq!(
            convert_with_rate(i128::MAX, 7, 5_000_000, 7, 7),
            None // overflow
        );
    }

    // ── deviation_bps / within_deviation ──────────────────────────────────

    #[test]
    fn identical_values_have_zero_deviation() {
        assert_eq!(deviation_bps(100, 100), Some(0));
        assert!(within_deviation(100, 100, 0));
    }

    #[test]
    fn deviation_computed_relative_to_reference() {
        // 105 vs 100 → 5% = 500 bps
        assert_eq!(deviation_bps(105, 100), Some(500));
        // 95 vs 100 → 5% = 500 bps (symmetric)
        assert_eq!(deviation_bps(95, 100), Some(500));
        // 1_000_000 vs 100 → 10_000x = 999_900 bps
        assert!(deviation_bps(1_000_000, 100).unwrap() > 10_000);
    }

    #[test]
    fn within_deviation_bounds() {
        let max = 500u32; // 5%
        assert!(within_deviation(105, 100, max));
        assert!(within_deviation(95, 100, max));
        assert!(!within_deviation(106, 100, max));
        assert!(!within_deviation(94, 100, max));
        // exact boundary is allowed (500 bps)
        assert!(within_deviation(105, 100, 500));
        assert!(!within_deviation(105, 100, 499));
    }

    #[test]
    fn zero_reference_is_never_within_bounds() {
        assert_eq!(deviation_bps(0, 0), None);
        assert_eq!(deviation_bps(1, 0), None);
        assert!(!within_deviation(1, 0, 10_000));
    }

    #[test]
    fn deviation_overflow_safely_rejected() {
        // |i128::MIN - i128::MAX| overflows i128 subtraction → None
        assert_eq!(deviation_bps(i128::MAX, i128::MIN), None);
        assert!(!within_deviation(i128::MAX, i128::MIN, u32::MAX));
    }
}
