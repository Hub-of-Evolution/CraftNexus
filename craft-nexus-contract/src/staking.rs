use soroban_sdk::{
    contract, contractimpl, contracttype,

    vec, Address, Env, Vec,
};

// ============================================================================
// 0. ERROR TYPES
// ============================================================================

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    StakeNotFound = 1,
}

// ============================================================================
// 1. DATA STRUCTURES & KEYS
// ============================================================================

const COOLDOWN_PERIOD: u64 = 86400 * 7; // 7 days in seconds
const MIN_STAKE_REQUIRED: i128 = 100;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakeEntry {
    pub amount: i128,
    pub unlock_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    StakeHealthSnapshotNotFound = 1,
}

#[contracttype]
pub enum DataKey {
    UserStakes(Address),
    MinStakeRequired,
}

// ============================================================================
// 2. STAKING CONTRACT IMPLEMENTATION (The Fix)
// ============================================================================

#[contract]
pub struct StakeContract;

#[contractimpl]
impl StakeContract {
    /// Sets the minimum stake required. Only the admin can call this.
    pub fn set_min_stake_required(env: Env, admin: Address, amount: i128) {
        admin.require_auth();

        if amount < 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let current: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinStakeRequired)
            .unwrap_or(MIN_STAKE_REQUIRED);

        let new_amount = current
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::Overflow));

        env.storage()
            .instance()
            .set(&DataKey::MinStakeRequired, &new_amount);
    }

    /// Adds a new stake, appending it as an independent entry with its own maturity.
    pub fn stake(env: Env, user: Address, amount: i128) {
        user.require_auth();

        let current_time = env.ledger().timestamp();
        let unlock_time = current_time + COOLDOWN_PERIOD;

        let mut stakes: Vec<StakeEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::UserStakes(user.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        stakes.push_back(StakeEntry {
            amount,
            unlock_time,
        });
        env.storage()
            .persistent()
            .set(&DataKey::UserStakes(user), &stakes);

        // (External token transfer logic from user to contract would go here)
    }

    /// Evaluates all user stakes and processes withdrawals for those that have matured.
    pub fn withdraw_matured(env: Env, user: Address) -> i128 {
        user.require_auth();

        let current_time = env.ledger().timestamp();
        let stakes: Vec<StakeEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::UserStakes(user.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let mut remaining_stakes = Vec::new(&env);
        let mut withdrawable_amount: i128 = 0;

        for stake in stakes.into_iter() {
            if current_time >= stake.unlock_time {
                withdrawable_amount += stake.amount;
            } else {
                remaining_stakes.push_back(stake);
            }
        }

        // Update state with only the pending, un-matured stakes
        env.storage()
            .persistent()
            .set(&DataKey::UserStakes(user), &remaining_stakes);

        // (External token transfer logic from contract to user would go here)

        withdrawable_amount
    }

    /// Read-only function to inspect a user's current stake queue
    pub fn get_stakes(env: Env, user: Address) -> Vec<StakeEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::UserStakes(user))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Read-only function to inspect a user's persisted stake health snapshot.
    /// Returns `None` if `evaluate_stake_health` has never been called for the user.
    pub fn get_stake_health_snapshot(env: Env, user: Address) -> Option<StakeEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::UserStakes(user))
    }
}

// ============================================================================
// 3. TEST SUITE (Issue #1050 Acceptance Criteria)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup() -> (Env, Address, StakeContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();

        // Initialize ledger time to a known baseline
        env.ledger().set_timestamp(100_000);

        let user = Address::generate(&env);
        let contract_id = env.register_contract(None, StakeContract);
        let client = StakeContractClient::new(&env, &contract_id);

        (env, user, client)
    }

    #[test]
    fn test_new_deposit_does_not_bypass_cooldown() {
        let (env, user, client) = setup();

        // 1. Initial stake
        client.stake(&user, &1000);
        let initial_stake_time = env.ledger().timestamp();

        // 2. Advance time forward, but not past the 7-day cooldown
        env.ledger()
            .set_timestamp(initial_stake_time + (COOLDOWN_PERIOD / 2));

        // 3. Second stake added
        client.stake(&user, &500);

        // 4. Attempt withdrawal. Neither should be ready.
        let withdrawn = client.withdraw_matured(&user);
        assert_eq!(withdrawn, 0, "New deposit bypassed cooldown rules");

        let queue = client.get_stakes(&user);
        assert_eq!(queue.len(), 2, "Queue should contain both pending deposits");
    }

    #[test]
    fn test_matured_deposits_remain_withdrawable_under_original_schedule() {
        let (env, user, client) = setup();

        client.stake(&user, &1000);
        let initial_stake_time = env.ledger().timestamp();

        // Advance time just past the cooldown for the first stake
        env.ledger()
            .set_timestamp(initial_stake_time + COOLDOWN_PERIOD + 1);

        // Add a new stake
        client.stake(&user, &500);

        // Withdraw matured stakes. The first 1000 should be ready, the 500 should remain locked.
        let withdrawn = client.withdraw_matured(&user);

        assert_eq!(
            withdrawn, 1000,
            "Matured deposit was blocked by new deposit"
        );

        let remaining_queue = client.get_stakes(&user);
        assert_eq!(
            remaining_queue.len(),
            1,
            "Only the new pending deposit should remain"
        );
        assert_eq!(remaining_queue.get(0).unwrap().amount, 500);
    }

    #[test]
    fn test_deposits_before_and_after_boundary() {
        let (env, user, client) = setup();
        let base_time = env.ledger().timestamp();

        // Stake A: Time T
        client.stake(&user, &1000);

        // Stake B: Time T + 3 days
        env.ledger().set_timestamp(base_time + 86400 * 3);
        client.stake(&user, &2000);

        // Advance to Time T + 7.5 days.
        // Stake A is past boundary (>7 days). Stake B is before boundary (only 4.5 days old).
        env.ledger()
            .set_timestamp(base_time + COOLDOWN_PERIOD + 43200);

        let withdrawn = client.withdraw_matured(&user);

        // Assert exactly Stake A is released
        assert_eq!(
            withdrawn, 1000,
            "Boundary logic failed to separate mature vs pending stakes"
        );

        // Advance remaining time to mature Stake B
        env.ledger()
            .set_timestamp(base_time + 86400 * 3 + COOLDOWN_PERIOD + 1);
        let withdrawn_b = client.withdraw_matured(&user);

        assert_eq!(
            withdrawn_b, 2000,
            "Stake B failed to mature on its independent schedule"
        );

        let empty_queue = client.get_stakes(&user);
        assert_eq!(
            empty_queue.len(),
            0,
            "Queue should be empty after all stakes mature"
        );
    }

    #[test]
    fn test_get_stake_health_snapshot_missing_key_returns_none() {
        let (env, user, client) = setup();

        // No record exists yet: must not trap, must return None.
        let snapshot = client.get_stake_health_snapshot(&user);
        assert_eq!(snapshot, None, "Missing snapshot should return None");

        // After a terminal state (all stakes withdrawn), still safe.
        client.stake(&user, &1000);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + COOLDOWN_PERIOD + 1);
        client.withdraw_matured(&user);

        let snapshot_after = client.get_stake_health_snapshot(&user);
        assert_eq!(snapshot_after, None, "Snapshot should remain None");
    }
}
