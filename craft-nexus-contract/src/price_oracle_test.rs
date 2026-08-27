#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, Env, IntoVal, Symbol, TryIntoVal,
};

fn setup_test(
    env: &Env,
    mock_auth: bool,
) -> (
    CraftNexusContractClient<'static>,
    Address, // token_a
    Address, // token_b
    Address, // admin
) {
    env.budget().reset_unlimited();
    if mock_auth {
        env.mock_all_auths();
    }
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_a = env.register_stellar_asset_contract_v2(token_admin.clone());

    let token_b_admin = Address::generate(env);
    let token_b = env.register_stellar_asset_contract_v2(token_b_admin.clone());

    // Set a non-zero timestamp for freshness tests.
    env.ledger().with_mut(|li| {
        li.timestamp = 1711368000; // 2024-03-25
    });

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    (client, token_a.address(), token_b.address(), admin)
}

/// Publish feeds for both tokens priced at 1.0 (decimals 7).
fn publish_default_feeds(client: &CraftNexusContractClient<'static>, token_a: &Address, token_b: &Address) {
    client.set_price_feed(token_a, &10i128.pow(7), &7);
    client.set_price_feed(token_b, &10i128.pow(7), &7);
}

// ── Feed management (malformed-data rejection) ───────────────────────────────

#[test]
fn test_set_and_get_price_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, admin) = setup_test(&env, true);

    client.set_price_feed(&token_a, &1_000_000, &6);

    let feed = client.get_price_feed(&token_a).expect("feed published");
    assert_eq!(feed.price, 1_000_000);
    assert_eq!(feed.decimals, 6);
    // The contract stamps the ledger timestamp — a feed cannot claim a
    // freshness it never actually had.
    assert_eq!(feed.timestamp, env.ledger().timestamp());
    assert_eq!(feed.source, admin);

    // Updating an existing feed overwrites it.
    client.set_price_feed(&token_a, &2_000_000, &6);
    let feed = client.get_price_feed(&token_a).unwrap();
    assert_eq!(feed.price, 2_000_000);
}

#[test]
fn test_set_price_feed_rejects_malformed_data() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, _) = setup_test(&env, true);

    // zero price
    let result = client.try_set_price_feed(&token_a, &0, &6);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // negative price
    let result = client.try_set_price_feed(&token_a, &-100, &6);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // decimals outside supported range
    let result = client.try_set_price_feed(&token_a, &1_000_000, &19);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // Nothing was persisted.
    assert!(client.get_price_feed(&token_a).is_none());
}

#[test]
fn test_set_price_feed_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, _) = setup_test(&env, true);

    // Clear auths: admin.require_auth() must reject the publish.
    env.set_auths(&[]);
    assert!(client.try_set_price_feed(&token_a, &1_000_000, &6).is_err());
}

#[test]
fn test_remove_price_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, _) = setup_test(&env, true);

    client.set_price_feed(&token_a, &1_000_000, &6);
    assert!(client.get_price_feed(&token_a).is_some());

    client.remove_price_feed(&token_a);
    assert!(client.get_price_feed(&token_a).is_none());

    // Removing a missing feed fails with PriceFeedNotFound.
    let result = client.try_remove_price_feed(&token_a);
    assert_eq!(result.unwrap_err(), Ok(Error::PriceFeedNotFound));
}

#[test]
fn test_remove_price_feed_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, _) = setup_test(&env, true);
    client.set_price_feed(&token_a, &1_000_000, &6);

    env.set_auths(&[]);
    assert!(client.try_remove_price_feed(&token_a).is_err());
}

// ── Oracle config ────────────────────────────────────────────────────────────

#[test]
fn test_oracle_config_defaults() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup_test(&env, true);

    let config = client.get_oracle_config();
    assert_eq!(config.max_staleness, price_oracle::DEFAULT_MAX_STALENESS);
    assert_eq!(
        config.max_deviation_bps,
        price_oracle::DEFAULT_MAX_DEVIATION_BPS
    );
}

#[test]
fn test_set_oracle_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup_test(&env, true);

    client.set_oracle_config(&7_200, &250);
    let config = client.get_oracle_config();
    assert_eq!(config.max_staleness, 7_200);
    assert_eq!(config.max_deviation_bps, 250);

    // Zero staleness would reject every feed immediately → invalid.
    let result = client.try_set_oracle_config(&0, &250);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // Deviation band wider than 100% is meaningless → invalid.
    let result = client.try_set_oracle_config(&3_600, &10_001);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // Previous config is preserved after failed updates.
    let config = client.get_oracle_config();
    assert_eq!(config.max_staleness, 7_200);
}

#[test]
fn test_set_oracle_config_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup_test(&env, true);

    env.set_auths(&[]);
    assert!(client.try_set_oracle_config(&7_200, &250).is_err());
}

// ── Guarded conversion (stale-data rejection, determinism) ──────────────────

#[test]
fn test_convert_amount_missing_feed_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // No feeds at all.
    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::PriceFeedNotFound));

    // Only one feed published → still missing the other.
    client.set_price_feed(&token_a, &10i128.pow(7), &7);
    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::PriceFeedNotFound));
}

#[test]
fn test_convert_amount_rejects_stale_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    // Fresh: conversion succeeds.
    let converted = client.convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(converted, 1_000_000);

    // Advance past the default 1-hour staleness window.
    env.ledger().with_mut(|li| {
        li.timestamp += price_oracle::DEFAULT_MAX_STALENESS + 1;
    });

    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::StalePriceData));
}

#[test]
fn test_convert_amount_rejects_stale_feed_at_exact_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    // Exactly max_staleness later → still fresh (inclusive-end convention).
    env.ledger().with_mut(|li| {
        li.timestamp += price_oracle::DEFAULT_MAX_STALENESS;
    });
    let _ = client.convert_amount(&token_a, &token_b, &1_000_000);

    // One second more → stale.
    env.ledger().with_mut(|li| {
        li.timestamp += 1;
    });
    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::StalePriceData));
}

#[test]
fn test_convert_amount_rejects_malformed_stored_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, admin) = setup_test(&env, true);
    client.set_price_feed(&token_b, &10i128.pow(7), &7);

    // Bypass the admin API and write a malformed feed directly (defense in
    // depth: the read path must reject it even if it ever reaches storage).
    let now = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        env.storage().persistent().set(
            &DataKey::PriceFeed(token_a.clone()),
            &PriceFeed {
                price: 0,
                decimals: 18,
                timestamp: now,
                source: admin.clone(),
            },
        );
    });

    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));
}

#[test]
fn test_convert_amount_rejects_future_timestamp_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, admin) = setup_test(&env, true);
    client.set_price_feed(&token_b, &10i128.pow(7), &7);

    // A feed stamped in the future is malformed clock data → stale.
    let now = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        env.storage().persistent().set(
            &DataKey::PriceFeed(token_a.clone()),
            &PriceFeed {
                price: 10i128.pow(7),
                decimals: 7,
                timestamp: now + 1,
                source: admin.clone(),
            },
        );
    });

    let result = client.try_convert_amount(&token_a, &token_b, &1_000_000);
    assert_eq!(result.unwrap_err(), Ok(Error::StalePriceData));
}

#[test]
fn test_convert_amount_success_and_determinism() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // A: 1.0 (dec 7), B: 2.0 (dec 7) → 1 A-unit == 0.5 B-units.
    client.set_price_feed(&token_a, &10i128.pow(7), &7);
    client.set_price_feed(&token_b, &(2 * 10i128.pow(7)), &7);

    let amount = 1_000_000i128;
    let first = client.convert_amount(&token_a, &token_b, &amount);
    assert_eq!(first, 500_000);

    // Deterministic for the same contract state: identical inputs → identical
    // output on every invocation.
    for _ in 0..5 {
        assert_eq!(client.convert_amount(&token_a, &token_b, &amount), first);
    }

    // Round trip returns the original amount (1.0 ↔ 2.0 is exact).
    let back = client.convert_amount(&token_b, &token_a, &first);
    assert_eq!(back, amount);
}

#[test]
fn test_convert_amount_rejects_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    let result = client.try_convert_amount(&token_a, &token_b, &-100);
    assert_eq!(result.unwrap_err(), Ok(Error::ConversionOutOfBounds));
}

// ── Observed-rate guardrail (explicit conversion bounds) ─────────────────────

#[test]
fn test_observed_rate_within_bounds_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // Oracle: A priced 1.0, B priced 2.0 → 1 A == 0.5 B.
    client.set_price_feed(&token_a, &10i128.pow(7), &7);
    client.set_price_feed(&token_b, &(2 * 10i128.pow(7)), &7);

    // Observed market rate matches the oracle reference (0.5 B per A).
    let observed =
        client.convert_with_observed_rate(&token_a, &token_b, &1_000_000, &5_000_000, &7);
    assert_eq!(observed, 500_000);
}

#[test]
fn test_observed_rate_within_bounds_accepted_after_refresh() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    // Feeds at 1.0/1.0 → reference output equals the input amount.
    let observed = client.convert_with_observed_rate(&token_a, &token_b, &2_000_000, &10i128.pow(7), &7);
    assert_eq!(observed, 2_000_000);
}

#[test]
fn test_observed_rate_out_of_bounds_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // Oracle: A priced 1.0, B priced 2.0 → reference 0.5 B per A.
    client.set_price_feed(&token_a, &10i128.pow(7), &7);
    client.set_price_feed(&token_b, &(2 * 10i128.pow(7)), &7);

    // Observed rate 0.55 (10% above reference) → outside the 5% band.
    let result =
        client.try_convert_with_observed_rate(&token_a, &token_b, &1_000_000, &5_500_000, &7);
    assert_eq!(result.unwrap_err(), Ok(Error::ConversionOutOfBounds));

    // Observed rate 0.45 (10% below reference) → also outside the band.
    let result =
        client.try_convert_with_observed_rate(&token_a, &token_b, &1_000_000, &4_500_000, &7);
    assert_eq!(result.unwrap_err(), Ok(Error::ConversionOutOfBounds));

    // Exactly 5% off (0.525) is still within the configured band.
    let within = client.convert_with_observed_rate(&token_a, &token_b, &1_000_000, &5_250_000, &7);
    assert_eq!(within, 525_000);
}

#[test]
fn test_observed_rate_malformed_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    // Zero observed price is malformed.
    let result = client.try_convert_with_observed_rate(&token_a, &token_b, &1_000_000, &0, &7);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));

    // Unsupported observed decimals are malformed.
    let result =
        client.try_convert_with_observed_rate(&token_a, &token_b, &1_000_000, &10i128.pow(7), &19);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidPriceData));
}

// ── Fee quote (deterministic, bounded) ───────────────────────────────────────

#[test]
fn test_quote_fee_for_amount_deterministic_and_bounded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // A priced 1.0 (dec 7), B priced 2.0 (dec 7).
    client.set_price_feed(&token_a, &10i128.pow(7), &7);
    client.set_price_feed(&token_b, &(2 * 10i128.pow(7)), &7);

    let amount = 1_000_000i128;
    let fee_bps = 500u32; // 5%
    let quote = client.quote_fee_for_amount(&token_a, &amount, &fee_bps, &token_b);

    // Nominal fee: 1_000_000 * 500 / 10_000 = 50_000 (deterministic bps math).
    assert_eq!(quote.fee_in_token, 50_000);
    // Converted at 2.0: 25_000 in B units.
    assert_eq!(quote.fee_in_quote_token, 25_000);

    // Deterministic for the same contract state.
    for _ in 0..5 {
        assert_eq!(
            client.quote_fee_for_amount(&token_a, &amount, &fee_bps, &token_b),
            quote
        );
    }

    // The quote equals a direct guarded conversion of the nominal fee.
    let direct = client.convert_amount(&token_a, &token_b, &quote.fee_in_token);
    assert_eq!(direct, quote.fee_in_quote_token);
}

#[test]
fn test_quote_fee_invalid_bps_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    let result = client.try_quote_fee_for_amount(&token_a, &1_000_000, &10_001, &token_b);
    assert_eq!(result.unwrap_err(), Ok(Error::InvalidFee));
}

#[test]
fn test_quote_fee_rejects_stale_feed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    env.ledger().with_mut(|li| {
        li.timestamp += price_oracle::DEFAULT_MAX_STALENESS + 1;
    });

    let result = client.try_quote_fee_for_amount(&token_a, &1_000_000, &500, &token_b);
    assert_eq!(result.unwrap_err(), Ok(Error::StalePriceData));
}

#[test]
fn test_quote_fee_round_trip_out_of_bounds_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);

    // Narrow the band to zero deviation: any rounding drift must fail.
    client.set_oracle_config(&3_600, &0);
    // A priced 3.0 (dec 0), B priced 10.0 (dec 0) — asymmetric division.
    client.set_price_feed(&token_a, &3, &0);
    client.set_price_feed(&token_b, &10, &0);

    // fee_in_token = 10_000 * 1000 / 10_000 = 1_000 → quote = 1_000*3/10 = 300,
    // round trip = 300*10/3 = 1_000 → exact, within the zero band.
    let quote = client.quote_fee_for_amount(&token_a, &10_000, &1000, &token_b);
    assert_eq!(quote.fee_in_token, 1_000);
    assert_eq!(quote.fee_in_quote_token, 300);

    // fee_in_token = 10_010 * 1000 / 10_000 = 1_001 → quote = 1_001*3/10 = 300,
    // round trip = 300*10/3 = 1_000 → 1 unit drift → outside the zero band.
    let result = client.try_quote_fee_for_amount(&token_a, &10_010, &1000, &token_b);
    assert_eq!(result.unwrap_err(), Ok(Error::ConversionOutOfBounds));
}

// ── Determinism of fee math for the same contract state ──────────────────────

#[test]
fn test_fee_calculation_deterministic_same_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, token_b, _) = setup_test(&env, true);
    publish_default_feeds(&client, &token_a, &token_b);

    // Same contract state → identical nominal fee every time.
    let first = client.calculate_fee_for_amount(&777_777);
    for _ in 0..5 {
        assert_eq!(client.calculate_fee_for_amount(&777_777), first);
    }

    // Same contract state → identical oracle-backed quote every time.
    let quote_first = client.quote_fee_for_amount(&token_a, &777_777, &500, &token_b);
    for _ in 0..5 {
        assert_eq!(
            client.quote_fee_for_amount(&token_a, &777_777, &500, &token_b),
            quote_first
        );
    }
}

// ── Event emission ───────────────────────────────────────────────────────────

#[test]
fn test_set_price_feed_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token_a, _, admin) = setup_test(&env, true);

    client.set_price_feed(&token_a, &1_000_000, &6);

    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, client.address);
    assert_eq!(
        last.1,
        vec![
            &env,
            Symbol::new(&env, "price_feed_updated").into_val(&env),
            token_a.clone().into_val(&env),
        ]
    );

    let event: PriceFeedUpdatedEvent = last.2.try_into_val(&env).unwrap();
    assert_eq!(event.token, token_a);
    assert_eq!(event.price, 1_000_000);
    assert_eq!(event.decimals, 6);
    assert_eq!(event.timestamp, env.ledger().timestamp());
    assert_eq!(event.source, admin);
}

#[test]
fn test_set_oracle_config_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup_test(&env, true);

    client.set_oracle_config(&7_200, &250);

    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, client.address);
    let event: OracleConfigUpdatedEvent = last.2.try_into_val(&env).unwrap();
    assert_eq!(event.max_staleness, 7_200);
    assert_eq!(event.max_deviation_bps, 250);
}
