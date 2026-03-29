#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Env, IntoVal, Symbol, TryIntoVal,
};

/// Shared setup: registers contract, initializes platform, mints tokens for buyer.
fn setup_recurring_test(
    env: &Env,
) -> (
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let artisan = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_asset = token::StellarAssetClient::new(env, &token_contract.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
    });

    client.initialize(&platform_wallet, &admin, &arbitrator, &500);
    client.set_min_escrow_amount(&token_contract.address(), &0);

    (
        client,
        buyer,
        artisan,
        token_contract.address(),
        token_asset,
        platform_wallet,
    )
}

// ── Creation Tests ───────────────────────────────────────────────────────

#[test]
fn test_create_recurring_escrow_success() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);

    let escrow = client.create_recurring_escrow(
        &buyer, &artisan, &token_id, &10_000, &100, // frequency: 100 seconds
        &10,  // duration: 10 periods
    );

    assert_eq!(escrow.id, 1);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.artisan, artisan);
    assert_eq!(escrow.token, token_id);
    assert_eq!(escrow.total_amount, 10_000);
    assert_eq!(escrow.frequency, 100);
    assert_eq!(escrow.duration, 10);
    assert_eq!(escrow.released_amount, 0);
    assert_eq!(escrow.status, RecurringEscrowStatus::Active);
    assert_eq!(escrow.start_time, 1_000_000);
    assert_eq!(escrow.next_release_time, 1_000_100);

    // Verify funds transferred to contract
    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&client.address), 10_000);
    assert_eq!(token_client.balance(&buyer), 0);

    // Verify stored escrow matches
    let stored = client.get_recurring_escrow(&1);
    assert_eq!(stored, escrow);

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "recurring_escrow_created").into_val(&env),
            1u64.into_val(&env)
        ]
    );
    let event: RecurringEscrowCreatedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.artisan, artisan);
    assert_eq!(event.total_amount, 10_000);
}

#[test]
fn test_create_multiple_recurring_escrows_increment_ids() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &100_000);

    let e1 = client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &5);
    let e2 = client.create_recurring_escrow(&buyer, &artisan, &token_id, &20_000, &200, &3);

    assert_eq!(e1.id, 1);
    assert_eq!(e2.id, 2);
}

#[test]
#[should_panic]
fn test_create_recurring_escrow_same_buyer_artisan() {
    let env = Env::default();
    let (client, buyer, _, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &buyer, &token_id, &10_000, &100, &10);
}

#[test]
#[should_panic]
fn test_create_recurring_escrow_zero_amount() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &0, &100, &10);
}

#[test]
#[should_panic]
fn test_create_recurring_escrow_negative_amount() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &-100, &100, &10);
}

#[test]
#[should_panic]
fn test_create_recurring_escrow_zero_frequency() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &0, &10);
}

#[test]
#[should_panic]
fn test_create_recurring_escrow_zero_duration() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &0);
}

// ── Sequential Release Tests ─────────────────────────────────────────────

#[test]
fn test_release_next_recurring_single_period() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Advance time past first period
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_100; // exactly at next_release_time
    });

    client.release_next_recurring(&1);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.released_amount, 1_000); // 10_000 / 10 = 1_000 per period
    assert_eq!(escrow.status, RecurringEscrowStatus::Active);
    assert_eq!(escrow.next_release_time, 1_000_200); // advanced by frequency

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 1_000);
    assert_eq!(token_client.balance(&client.address), 9_000);

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    let event: RecurringFundsReleasedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.released_amount, 1_000);
    assert_eq!(event.total_released, 1_000);
    assert_eq!(event.period, 1);
}

#[test]
fn test_release_sequential_periods() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    let token_client = token::Client::new(&env, &token_id);

    // Release period 1
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_100;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 1_000);

    // Release period 2
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_200;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 2_000);

    // Release period 3
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_300;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 3_000);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.released_amount, 3_000);
    assert_eq!(escrow.status, RecurringEscrowStatus::Active);
}

#[test]
fn test_release_multiple_periods_at_once() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Skip ahead 3 periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_300; // 3 periods elapsed
    });

    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    // Should release 3 periods * 1000 each = 3000
    assert_eq!(token_client.balance(&artisan), 3_000);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.released_amount, 3_000);
    assert_eq!(escrow.next_release_time, 1_000_400);
}

#[test]
fn test_release_all_periods_completes_escrow() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Skip all 10 periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_001_000; // 10 periods
    });

    client.release_next_recurring(&1);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
    assert_eq!(escrow.released_amount, 10_000);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 10_000);
    assert_eq!(token_client.balance(&client.address), 0);

    // Verify completion event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    let event: RecurringEscrowCompletedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.total_released, 10_000);
}

#[test]
fn test_release_handles_remainder_on_final_period() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    // 100 / 3 = 33 per period, remainder 1. Last period should be 34.
    token_admin.mint(&buyer, &100);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &100, &100, &3);

    let token_client = token::Client::new(&env, &token_id);

    // Period 1
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_100;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 33);

    // Period 2
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_200;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 66);

    // Period 3 (final - includes remainder)
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_300;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 100); // 66 + 34 = 100

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.released_amount, 100);
    assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
}

// ── Edge Cases: Too Early ────────────────────────────────────────────────

#[test]
fn test_release_before_due_returns_error() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Time hasn't reached next_release_time yet
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_050; // only 50s elapsed, need 100
    });

    let result = client.try_release_next_recurring(&1);
    assert!(result.is_err());
}

#[test]
fn test_release_at_boundary_succeeds() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_100; // exactly at next_release_time
    });

    let result = client.try_release_next_recurring(&1);
    assert!(result.is_ok());
}

#[test]
fn test_release_one_second_before_due_fails() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_099; // 1 second before due
    });

    let result = client.try_release_next_recurring(&1);
    assert!(result.is_err());
}

// ── Edge Cases: After Completion ─────────────────────────────────────────

#[test]
fn test_release_after_completion_returns_error() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Complete all periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_001_000;
    });
    client.release_next_recurring(&1);

    // Try releasing again - should fail with InvalidEscrowState since status is Completed
    let result = client.try_release_next_recurring(&1);
    assert!(result.is_err());
}

// ── Cancellation Tests ───────────────────────────────────────────────────

#[test]
fn test_cancel_recurring_escrow_refunds_unreleased() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Release 2 periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_200;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 2_000);

    // Cancel
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_250;
    });
    let result = client.try_cancel_recurring_escrow(&1);
    assert!(result.is_ok());

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.status, RecurringEscrowStatus::Cancelled);

    // Buyer gets remaining 8_000 back
    assert_eq!(token_client.balance(&buyer), 8_000);
    assert_eq!(token_client.balance(&artisan), 2_000);
    assert_eq!(token_client.balance(&client.address), 0);

    // Verify cancellation event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    let event: RecurringEscrowCancelledEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.refund_amount, 8_000);
}

#[test]
fn test_cancel_without_any_releases_refunds_full() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    let result = client.try_cancel_recurring_escrow(&1);
    assert!(result.is_ok());

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 10_000);
    assert_eq!(token_client.balance(&artisan), 0);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_cancel_completed_escrow_fails() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Release all periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_001_000;
    });
    client.release_next_recurring(&1);

    // Cancel should fail because status is Completed
    let result = client.try_cancel_recurring_escrow(&1);
    assert!(result.is_err());
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    client.cancel_recurring_escrow(&1);

    let result = client.try_cancel_recurring_escrow(&1);
    assert!(result.is_err());
}

#[test]
fn test_release_after_cancel_fails() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    client.cancel_recurring_escrow(&1);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_001_000;
    });

    let result = client.try_release_next_recurring(&1);
    assert!(result.is_err());
}

// ── Query Function Tests ─────────────────────────────────────────────────

#[test]
fn test_get_escrow_status_active() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    assert_eq!(
        client.get_recurring_escrow_status(&1),
        RecurringEscrowStatus::Active
    );
}

#[test]
fn test_get_remaining_balance() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    assert_eq!(client.get_recurring_remaining_balance(&1), 10_000);

    // Release 3 periods
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_300;
    });
    client.release_next_recurring(&1);

    assert_eq!(client.get_recurring_remaining_balance(&1), 7_000);
}

#[test]
#[should_panic]
fn test_get_escrow_not_found() {
    let env = Env::default();
    let (client, _, _, _, _, _) = setup_recurring_test(&env);
    client.get_recurring_escrow(&999);
}

#[test]
#[should_panic]
fn test_get_status_not_found() {
    let env = Env::default();
    let (client, _, _, _, _, _) = setup_recurring_test(&env);
    client.get_recurring_escrow_status(&999);
}

#[test]
#[should_panic]
fn test_get_remaining_balance_not_found() {
    let env = Env::default();
    let (client, _, _, _, _, _) = setup_recurring_test(&env);
    client.get_recurring_remaining_balance(&999);
}

// ── Pagination Tests ─────────────────────────────────────────────────────

#[test]
fn test_get_recurring_escrows_by_buyer() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &100_000);

    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &5);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &20_000, &200, &3);

    let all = client.get_recurring_escrows_by_buyer(&buyer, &0, &10);
    assert_eq!(all.len(), 2);
    assert_eq!(all.get_unchecked(0), 1);
    assert_eq!(all.get_unchecked(1), 2);

    // Pagination
    let page1 = client.get_recurring_escrows_by_buyer(&buyer, &0, &1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get_unchecked(0), 1);

    let page2 = client.get_recurring_escrows_by_buyer(&buyer, &1, &1);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get_unchecked(0), 2);

    let empty = client.get_recurring_escrows_by_buyer(&buyer, &5, &10);
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_get_recurring_escrows_by_artisan() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);
    let buyer2 = Address::generate(&env);

    token_admin.mint(&buyer, &100_000);
    token_admin.mint(&buyer2, &100_000);

    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &5);
    client.create_recurring_escrow(&buyer2, &artisan, &token_id, &20_000, &200, &3);

    let all = client.get_recurring_escrows_by_artisan(&artisan, &0, &10);
    assert_eq!(all.len(), 2);

    let empty_artisan = Address::generate(&env);
    let none = client.get_recurring_escrows_by_artisan(&empty_artisan, &0, &10);
    assert_eq!(none.len(), 0);
}

// ── Large Amount / Overflow Safety Tests ─────────────────────────────────

#[test]
fn test_large_escrow_amount() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    // Use a large amount near i128 safe range
    let large_amount: i128 = 1_000_000_000_000_000_000; // 1 quintillion
    token_admin.mint(&buyer, &large_amount);

    let escrow = client.create_recurring_escrow(
        &buyer,
        &artisan,
        &token_id,
        &large_amount,
        &86400, // daily
        &12,    // 12 months
    );

    assert_eq!(escrow.total_amount, large_amount);

    // Release first month
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + 86400;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    let per_period = large_amount / 12;
    assert_eq!(token_client.balance(&artisan), per_period);

    // Release remaining
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + 86400 * 12;
    });
    client.release_next_recurring(&1);

    let stored = client.get_recurring_escrow(&1);
    assert_eq!(stored.status, RecurringEscrowStatus::Completed);
    assert_eq!(stored.released_amount, large_amount);
    assert_eq!(token_client.balance(&artisan), large_amount);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_single_period_escrow() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &5_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &5_000, &100, &1);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_100;
    });
    client.release_next_recurring(&1);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
    assert_eq!(escrow.released_amount, 5_000);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 5_000);
}

#[test]
fn test_amount_one_with_duration_ten() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &1);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &1, &100, &10);

    let token_client = token::Client::new(&env, &token_id);

    // 1 / 10 = 0 per period. First 9 releases give 0, last gives 1.
    for i in 1..=9 {
        env.ledger().with_mut(|li| {
            li.timestamp = 1_000_000 + (i * 100);
        });
        client.release_next_recurring(&1);
        assert_eq!(token_client.balance(&artisan), 0);
    }

    // Final period
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + 1000;
    });
    client.release_next_recurring(&1);
    assert_eq!(token_client.balance(&artisan), 1);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
}

// ── Determinism Tests ────────────────────────────────────────────────────

#[test]
fn test_deterministic_release_sequence() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    let token_client = token::Client::new(&env, &token_id);
    let mut expected_released: i128 = 0;

    for i in 1..=10 {
        env.ledger().with_mut(|li| {
            li.timestamp = 1_000_000 + (i * 100);
        });
        client.release_next_recurring(&1);
        expected_released += 1_000;
        assert_eq!(token_client.balance(&artisan), expected_released);
    }

    assert_eq!(expected_released, 10_000);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
}

#[test]
fn test_skip_periods_then_release() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    // Skip to period 5
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_500;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 5_000);

    let escrow = client.get_recurring_escrow(&1);
    assert_eq!(escrow.released_amount, 5_000);
    assert_eq!(escrow.next_release_time, 1_000_600);
}

#[test]
fn test_remaining_balance_never_negative() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);

    env.ledger().with_mut(|li| {
        li.timestamp = 1_001_000;
    });
    client.release_next_recurring(&1);

    let remaining = client.get_recurring_remaining_balance(&1);
    assert_eq!(remaining, 0);
}

// ── Token Whitelisting Integration ───────────────────────────────────────

#[test]
fn test_recurring_escrow_respects_token_whitelist() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let artisan = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_asset = token::StellarAssetClient::new(&env, &token_contract.address());

    let other_token_admin = Address::generate(&env);
    let other_token = env.register_stellar_asset_contract_v2(other_token_admin.clone());
    let other_token_asset = token::StellarAssetClient::new(&env, &other_token.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
    });

    client.initialize(&platform_wallet, &admin, &arbitrator, &500);
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_escrow_amount(&other_token.address(), &0);

    // Whitelist only token_contract
    client.whitelist_token(&token_contract.address());

    token_asset.mint(&buyer, &10_000);
    other_token_asset.mint(&buyer, &10_000);

    // Whitelisted token should succeed
    let escrow = client.create_recurring_escrow(
        &buyer,
        &artisan,
        &token_contract.address(),
        &10_000,
        &100,
        &10,
    );
    assert_eq!(escrow.id, 1);

    // Non-whitelisted token should fail
    let result = client.try_create_recurring_escrow(
        &buyer,
        &artisan,
        &other_token.address(),
        &10_000,
        &100,
        &10,
    );
    assert!(result.is_err());
}

// ── Circuit Breaker Integration ──────────────────────────────────────────

#[test]
fn test_create_recurring_blocked_when_paused() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &10_000);

    client.set_paused(&true);

    let result =
        client.try_create_recurring_escrow(&buyer, &artisan, &token_id, &10_000, &100, &10);
    assert!(result.is_err());
}

// ── Frequency Variations ─────────────────────────────────────────────────

#[test]
fn test_hourly_frequency() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &24_000);
    // 24 hourly releases of 1000 each
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &24_000, &3600, &24);

    // Advance 1 hour
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + 3600;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 1_000);
}

#[test]
fn test_daily_frequency() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &30_000);
    // 30 daily releases of 1000 each
    let seconds_per_day: u64 = 86400;
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &30_000, &seconds_per_day, &30);

    // Advance 1 day
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + seconds_per_day;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 1_000);
}

#[test]
fn test_weekly_frequency() {
    let env = Env::default();
    let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

    token_admin.mint(&buyer, &40_000);
    // 4 weekly releases of 10000 each
    let seconds_per_week: u64 = 604800;
    client.create_recurring_escrow(&buyer, &artisan, &token_id, &40_000, &seconds_per_week, &4);

    // Advance 1 week
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + seconds_per_week;
    });
    client.release_next_recurring(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&artisan), 10_000);
}

// ── Arithmetic Safety ────────────────────────────────────────────────────

#[test]
fn test_fuzz_various_amounts_and_durations() {
    // Test a sweep of amounts and durations to verify arithmetic invariants.
    // Each case gets its own env to avoid timestamp accumulation issues.
    let test_cases: alloc::vec::Vec<(i128, u64)> = alloc::vec![
        (7, 3),         // 7/3 = 2 per period, remainder 1
        (100, 7),       // 100/7 = 14 per period, remainder 2
        (1000, 13),     // 1000/13 = 76 per period, remainder 12
        (1_000_000, 7), // large amount, prime duration
        (97, 97),       // equal amount and duration -> 1 per period
        (100, 1),       // single period
    ];

    for (amount, duration) in test_cases {
        let env = Env::default();
        let (client, buyer, artisan, token_id, token_admin, _) = setup_recurring_test(&env);

        token_admin.mint(&buyer, &amount);

        client.create_recurring_escrow(&buyer, &artisan, &token_id, &amount, &100, &duration);

        // Release all periods at once
        env.ledger().with_mut(|li| {
            li.timestamp = 1_000_000 + (duration * 100);
        });
        client.release_next_recurring(&1);

        let escrow = client.get_recurring_escrow(&1);
        assert_eq!(escrow.status, RecurringEscrowStatus::Completed);
        assert_eq!(escrow.released_amount, amount);

        let remaining = client.get_recurring_remaining_balance(&1);
        assert_eq!(remaining, 0);
    }
}
