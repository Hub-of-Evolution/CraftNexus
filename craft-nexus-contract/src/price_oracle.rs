//! Price oracle input validation for fee and settlement calculations (Issue #1087).

use soroban_sdk::{Env, Symbol};

/// Max allowed confidence interval (in basis points).
const MAX_CONFIDENCE_BPS: u32 = 100;

/// Validates a price oracle update before it is used in conversions.
///
/// Rejects stale timestamps, malformed asset pairs, excessive precision,
/// and confidence bounds outside the configured range. The function is
/// pure and deterministic: identical valid inputs always succeed.
///
/// # Errors
/// Returns [`crate::Error::PriceOracleStale`] if the update is too old,
/// or [`crate::Error::PriceOracleInvalid`] when any field is malformed.
pub fn validate_oracle_price(
    env: &Env,
    timestamp: u64,
    max_age_secs: u64,
    asset_pair: &Symbol,
    price_precision: u32,
    confidence_bps: u32,
) -> Result<(), crate::Error> {
    let now = env.ledger().timestamp();
    if timestamp > now || now.saturating_sub(timestamp) > max_age_secs {
        return Err(crate::Error::PriceOracleStale);
    }
    if asset_pair.to_string().is_empty()
        || price_precision > 18
        || confidence_bps == 0
        || confidence_bps > MAX_CONFIDENCE_BPS
    {
        return Err(crate::Error::PriceOracleInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger;

    struct Ctx {
        env: Env,
    }
    impl Ctx {
        fn new(ts: u64) -> Self {
            let env = Env::default();
            env.ledger().set_timestamp(ts);
            Self { env }
        }
        fn pair(&self, s: &str) -> Symbol {
            Symbol::new(&self.env, s)
        }
        fn ok(&self, timestamp: u64, max_age_secs: u64, pair: &str, precision: u32, conf: u32) -> bool {
            validate_oracle_price(&self.env, timestamp, max_age_secs, &self.pair(pair), precision, conf).is_ok()
        }
        fn error(&self, timestamp: u64, max_age_secs: u64, pair: &str, precision: u32, conf: u32) -> Result<(), crate::Error> {
            validate_oracle_price(&self.env, timestamp, max_age_secs, &self.pair(pair), precision, conf)
        }
    }

    #[test]
    fn accepts_fresh_valid_price() {
        let c = Ctx::new(1_000_000);
        assert!(c.ok(999_900, 300, "XLMUSD", 7, 50));
    }

    #[test]
    fn rejects_stale_timestamp() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(999_000, 300, "XLMUSD", 7, 50), Err(crate::Error::PriceOracleStale));
    }

    #[test]
    fn rejects_future_timestamp() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(1_000_100, 300, "XLMUSD", 7, 50), Err(crate::Error::PriceOracleStale));
    }

    #[test]
    fn rejects_empty_asset_pair() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(999_900, 300, "", 7, 50), Err(crate::Error::PriceOracleInvalid));
    }

    #[test]
    fn rejects_excessive_precision() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(999_900, 300, "XLMUSD", 19, 50), Err(crate::Error::PriceOracleInvalid));
    }

    #[test]
    fn rejects_zero_confidence() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(999_900, 300, "XLMUSD", 7, 0), Err(crate::Error::PriceOracleInvalid));
    }

    #[test]
    fn rejects_excessive_confidence() {
        let c = Ctx::new(1_000_000);
        assert_eq!(c.error(999_900, 300, "XLMUSD", 7, 101), Err(crate::Error::PriceOracleInvalid));
    }
}