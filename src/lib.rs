#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol, Vec};

const MAX_FEE_BPS: i128 = 10000;

/// Platform configuration stored in instance storage.
#[contracttype]
pub struct Config {
    pub fee_bps: i128,
}

/// Compute the platform fee for a given amount.
///
/// Reads [`Config`] from instance storage under the key `"config"`.
/// Panics with a descriptive message on invalid inputs or missing config.
pub fn platform_fee(env: &Env, amount: i128) -> i128 {
    assert!(amount >= 0, "amount must be non-negative");

    let config: Config = env
        .storage()
        .instance()
        .get(&Symbol::new(env, "config"))
        .expect("config not found");

    let fee_bps = config.fee_bps;
    assert!(
        fee_bps >= 0 && fee_bps <= MAX_FEE_BPS,
        "fee_bps must be between 0 and MAX_FEE_BPS"
    );

    amount
        .checked_mul(fee_bps)
        .expect("overflow during fee calculation")
        / MAX_FEE_BPS
}

/// Retrieve a `Vec<i32>` from instance storage by key name.
///
/// Panics with `"storage error"` if the key is missing or the stored value
/// cannot be decoded as `Vec<i32>`.
pub fn safe_get_vec_i32(env: &Env, key: &str) -> Vec<i32> {
    env.storage()
        .instance()
        .get::<Symbol, Vec<i32>>(&Symbol::new(env, key))
        .expect("storage error")
}

#[contract]
pub struct PlatformFeeContract;

#[contractimpl]
impl PlatformFeeContract {
    pub fn get_fee(env: Env, amount: i128) -> i128 {
        platform_fee(&env, amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{vec, Env, Symbol};

    const DEFAULT_FEE_BPS: i128 = 500;

    fn setup_config(env: &Env, fee_bps: i128) {
        env.storage()
            .instance()
            .set(&Symbol::new(env, "config"), &Config { fee_bps });
    }

    fn setup_referral_chain(env: &Env, chain: Vec<i32>) {
        env.storage()
            .instance()
            .set(&Symbol::new(env, "referral_chain"), &chain);
    }

    #[test]
    fn test_platform_fee_happy_path() {
        let env = Env::default();
        setup_config(&env, DEFAULT_FEE_BPS);
        assert_eq!(platform_fee(&env, 1000), 50);
    }

    #[test]
    fn test_platform_fee_zero_amount() {
        let env = Env::default();
        setup_config(&env, DEFAULT_FEE_BPS);
        assert_eq!(platform_fee(&env, 0), 0);
    }

    #[test]
    fn test_platform_fee_max_bps() {
        let env = Env::default();
        setup_config(&env, MAX_FEE_BPS);
        assert_eq!(platform_fee(&env, 100), 100);
    }

    #[test]
    #[should_panic(expected = "amount must be non-negative")]
    fn test_platform_fee_negative_amount() {
        let env = Env::default();
        setup_config(&env, DEFAULT_FEE_BPS);
        platform_fee(&env, -1);
    }

    #[test]
    #[should_panic(expected = "fee_bps must be between 0 and MAX_FEE_BPS")]
    fn test_platform_fee_invalid_bps_negative() {
        let env = Env::default();
        setup_config(&env, -1);
        platform_fee(&env, 100);
    }

    #[test]
    #[should_panic(expected = "fee_bps must be between 0 and MAX_FEE_BPS")]
    fn test_platform_fee_invalid_bps_exceeds_max() {
        let env = Env::default();
        setup_config(&env, MAX_FEE_BPS + 1);
        platform_fee(&env, 100);
    }

    #[test]
    #[should_panic(expected = "config not found")]
    fn test_platform_fee_missing_config() {
        let env = Env::default();
        platform_fee(&env, 100);
    }

    #[test]
    #[should_panic(expected = "overflow during fee calculation")]
    fn test_platform_fee_overflows() {
        let env = Env::default();
        setup_config(&env, DEFAULT_FEE_BPS);
        platform_fee(&env, i128::MAX);
    }

    #[test]
    fn test_safe_get_vec_i32_existing() {
        let env = Env::default();
        let expected: Vec<i32> = vec![&env, 1, 2, 3];
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "referral_list"), &expected);
        let result = safe_get_vec_i32(&env, "referral_list");
        assert_eq!(result, expected);
    }

    #[test]
    #[should_panic(expected = "storage error")]
    fn test_safe_get_vec_i32_missing() {
        let env = Env::default();
        safe_get_vec_i32(&env, "non_existent_key");
    }

    #[test]
    fn test_safe_get_vec_i32_empty() {
        let env = Env::default();
        let expected: Vec<i32> = vec![&env];
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "empty_list"), &expected);
        let result = safe_get_vec_i32(&env, "empty_list");
        assert!(result.is_empty());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_referral_fee_distribution() {
        let env = Env::default();
        let referral_chain: Vec<i32> = vec![&env, 1001, 1002, 1003];
        setup_referral_chain(&env, referral_chain);
        let chain = safe_get_vec_i32(&env, "referral_chain");
        assert_eq!(chain.len(), 3);
        assert_eq!(chain.get(0).unwrap(), 1001);
        assert_eq!(chain.get(1).unwrap(), 1002);
        assert_eq!(chain.get(2).unwrap(), 1003);
    }

    #[test]
    fn test_referral_single_level() {
        let env = Env::default();
        let single: Vec<i32> = vec![&env, 9999];
        setup_referral_chain(&env, single);
        let chain = safe_get_vec_i32(&env, "referral_chain");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain.get(0).unwrap(), 9999);
    }

    #[test]
    fn test_referral_empty_chain() {
        let env = Env::default();
        let empty: Vec<i32> = vec![&env];
        setup_referral_chain(&env, empty);
        let chain = safe_get_vec_i32(&env, "referral_chain");
        assert!(chain.is_empty());
    }
}
