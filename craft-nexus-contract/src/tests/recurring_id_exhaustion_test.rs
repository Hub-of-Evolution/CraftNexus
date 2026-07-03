// Test that allocating a recurring escrow ID beyond the maximum returns an error.
#[test]
fn test_recurring_escrow_id_exhaustion() {
    let env = Env::default();
    // Set up the contract and token using the existing helper.
    let (escrow, _, buyer, artisan, token_id, token_admin, _, _) =
        setup_enhanced_test(&env);

    // Force the next recurring escrow ID to be the maximum value.
    env.storage()
        .persistent()
        .set(&DataKey::NextRecurringEscrowId, &MAX_RECURRING_ESCROW_ID);

    let total_amount: i128 = 1000;
    token_admin.mint(&buyer, &total_amount);

    // Attempt to create a recurring escrow – should error.
    let result = escrow.create_recurring_escrow(
        &buyer,
        &artisan,
        &token_id,
        &total_amount,
        3600,
        2,
    );
    assert_eq!(result, Err(crate::Error::RecurringEscrowIdExhausted));
}
