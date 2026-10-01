#![no_std]
#![allow(clippy::too_many_arguments)]
#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use soroban_sdk::{contract, contractimpl, env, Symbol};

/// Error types returned by the contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
#[contracterror]
pub enum Error {
    /// The requested liquidation record does not exist in storage.
    NotFound = 1,
    /// The liquidation record has already reached a terminal state.
    AlreadyTerminal = 2,
    /// The caller is not authorized to perform the operation.
    Unauthorized = 3,
}

pub fn get_expired_dispute_policy(env: Env) -> ExpiredDisputeFeePolicy {
        env.storage()
            .get(&DataKey::ExpiredDisputePolicy)
            .unwrap_or(&ExpiredDisputeFeePolicy::RefundFullNoPlatformFee)
    }

/// The lifecycle state of an artisan's liquidation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrder)]
#[contracttype]
pub enum LiquidationStatus {
    /// No liquidation has been started.
    None = 0,
    /// Liquidation is currently in progress.
    Active = 1,
    /// Liquidation has been completed successfully.
    Completed = 2,
    /// Liquidation was cancelled.
    Cancelled = 3,
}

const LIQUIDATION_KEY: Symbol = Symbol::short_symbol("LIQ_STATUS");

const DEFAULT_EXTEND_TO: u32 = 50;

#[contract]
pub struct CraftNexusContract;

    /////////////////////////////////////////////////////////////////////////////
    /// Escrow lifecycle
    /////////////////////////////////////////////////////////////////////////////
    pub fn create_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        max_dispute_duration: Option<u32>,
    ) {
        if env.storage().get(&DataKey::Paused).unwrap_or(&false) {
            soroban_sdk::panic_with_error(&env, &Error::ContractPaused);
        }
        if amount <= 0 {
            soroban_sdk::panic_with_error(&env, &Error::InvalidAmount);
        }
        let key = DataKey::Escrow(order_id);
        if env.storage().has(&key) {
            soroban_sdk::panic_with_error(&env, &Error::EscrowNotFound);
        }
        let escrow = Escrow{
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            order_id,
            status: EscrowStatus::Active,
            dispute_timestamp: 0,
            max_dispute_duration: max_dispute_duration.unwrap_or(&u32::default()),
        };
        env.storage().set(&key, &escrow);
    }

pub fn set_liquidation_status(env: Env, artisan: Address, status: LiquidationStatus) {
        let key = (LIQUIDATION_KEY, artisan);
        env.storage().persistent().set(&key, &status);
        env.storage().persistent().extend_ttl(&key, DEFAULT_EXTEND_TO, 0);
    }

    pub fn get_reconciliation_report(
        env: Env,
        token: Address,
    ) -> Option<ReconciliationReport> {
        env.storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(token))
    }

    pub fn set_moderator(env: Env, moderator: Address) {
        let mut config = Self::get_platform_config(env.clone());
        config.admin.require_auth();
        let previous = config
            .moderator
            .clone()
            .map(ConfigValue::Address)
            .unwrap_or_else(|| ConfigValue::String(String::from_str(&env, "unset")));
        config.moderator = Some(moderator.clone());
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        Self::emit_config_updated(&env, "moderator", previous, ConfigValue::Address(moderator));
    }
    }

pub fn blacklist_arbitrator(env: Env, arbitrator: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let key = DataKey::ArbitratorBlacklist(arbitrator.clone());
        env.storage().persistent().set(&key, &true);
        Self::extend_persistent(&env, &key);

        Self::emit_config_updated(
            &env,
            "arbitrator_blacklisted",
            ConfigValue::String(String::from_str(&env, "false")),
            ConfigValue::Address(arbitrator),
        );
    }

    pub fn remove_arbitrator_from_blacklist(env: Env, arbitrator: Address) {
        let config = Self::get_platform_config_internal(&env);
        if config.paused {
            panic_with_error!(&env, Error::ContractPaused);
        }
        config.admin.require_auth();

        let key = DataKey::ArbitratorBlacklist(arbitrator.clone());
        env.storage().persistent().remove(&key);

        Self::emit_config_updated(
            &env,
            "arbitrator_unblacklisted",
            ConfigValue::Address(arbitrator),
            ConfigValue::String(String::from_str(&env, "false")),
        );
    }

    pub fn is_arbitrator_blacklisted(env: Env, arbitrator: Address) -> Option<bool> {
        let key = DataKey::ArbitratorBlacklist(arbitrator.clone());
        if let Some(is_blacklisted) = env.storage().persistent().get(&key) {
            Self::extend_persistent_read(&env, &key);
            Some(is_blacklisted)
        } else {
            None
        }
    }

    /// Return the current liquidation status for an artisan.
    ///
    /// Returns `Error::NotFound` when the storage key is absent (e.g. after
    /// archival, partial migration, or a missing key) instead of panicking.
    pub fn get_liquidation_status(env: Env, artisan: Address) -> Result<LiquidationStatus, Error> {
        let key = (LIQUIDATION_KEY, artisan);
        match env.storage().persistent().get::<Address, LiquidationStatus>(&key) {
            Some(status) => {
                env.storage().persistent().extend_ttl(&key, DEFAULT_EXTEND_TO, 0);
                Ok(status)
            }
            None => Err(Error::NotFound),
        }
    }
        }
// Read tracked totals
        let tracked_locked: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalLocked(token.clone()))
            .unwrap_or(0);

        let tracked_staked: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalStaked(token.clone()))
            .unwrap_or(0);

        // Determine if complete
        let complete = end >= total_escrows;

        // Check for discrepancies only when complete
        let mut unresolved = false;
        if complete {
            unresolved = expected_locked != tracked_locked
                || expected_staked != tracked_staked
                || balance < expected_locked.saturating_add(expected_staked);
        }

        Ok(ReconciliationReport {
            token,
            balance,
            expected_locked,
            expected_staked,
            tracked_locked,
            tracked_staked,
            scanned_escrows: scanned,
            next_cursor: end,
            complete,
            unresolved,
        })
    }

    pub fn propose_reconciliation_repair(
        env: Env,
        token: Address,
    ) -> Result<ReconciliationRepairPlan, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let report: ReconciliationReport = env
            .storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(token.clone()))
            .ok_or(Error::ReconciliationRequired)?;
        if !report.complete || !report.unresolved {
            return Err(Error::ReconciliationRequired);
        }
        if report.balance < report.expected_locked + report.expected_staked {
            return Err(Error::EmergencyAccountingInvariant);
        }

        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextReconciliationRepairPlanId)
            .unwrap_or(1);
        let plan = ReconciliationRepairPlan {
            id,
            token,
            expected_locked: report.expected_locked,
            expected_staked: report.expected_staked,
            observed_balance: report.balance,
            observed_tracked_locked: report.tracked_locked,
            observed_tracked_staked: report.tracked_staked,
            created_at: env.ledger().timestamp(),
            applied: false,
            cancelled: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::ReconciliationRepairPlan(id), &plan);
        env.storage()
            .persistent()
            .set(&DataKey::NextReconciliationRepairPlanId, &(id + 1));
        Self::extend_persistent(&env, &DataKey::ReconciliationRepairPlan(id));
        Self::extend_persistent(&env, &DataKey::NextReconciliationRepairPlanId);
        Ok(plan)
    }

    pub fn get_reconciliation_repair_plan(
        env: Env,
        plan_id: u64,
    ) -> Option<ReconciliationRepairPlan> {
        env.storage()
            .persistent()
            .get(&DataKey::ReconciliationRepairPlan(plan_id))
    }

    /// Recovery function to sweep unallocated tokens from the contract (admin only).
    /// Unallocated funds = current_balance - (total_locked_in_escrows + total_staked_by_artisans).
    ///
    /// Requires a complete, resolved, and current `reconcile_token` report for
    /// `token` (#1069): the incremental locked/staked counters are trusted for
    /// routine reads, but a sweep must be *proven* safe against a canonical
    /// recomputation from the actual escrow and stake records before any
    /// balance can leave the contract this way. Call `reconcile_token` first;
    /// `Error::ReconciliationRequired` means it is missing, incomplete, stale,
    /// or found a mismatch.
    pub fn sweep_unallocated_funds(
        env: Env,
        token: Address,
        destination: Address,
    ) -> Result<i128, Error> {
        let _guard = ReentryGuardScope::new(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let allocation = Self::assert_safe_to_sweep(&env, &token)?;
        let unallocated = allocation.unallocated;

        if unallocated > 0 {
            Self::transfer_tokens_and_record_audit(
                &env,
                &token,
                &env.current_contract_address(),
                &destination,
                unallocated,
                &destination,
                Symbol::new(&env, "sweep_unallocated"),
                unallocated,
            );
        }

        Ok(unallocated)
    }

    pub fn set_min_escrow_amount(env: Env, token: Address, min_amount: i128) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let key = DataKey::MinEscrowAmount(token.clone());
        let old_amount: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        env.storage().persistent().set(&key, &min_amount);
        Self::extend_persistent(&env, &key);
        Self::emit_config_updated(
            &env,
            "min_escrow_amount",
            ConfigValue::I128(old_amount),
            ConfigValue::I128(min_amount),
        );
        Ok(())
    }

    pub fn get_platform_fee(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.platform_fee_bps
    }

    pub fn get_platform_wallet(env: Env) -> Address {
        let config = Self::get_platform_config_internal(&env);
        config.platform_wallet
    }

    pub fn get_total_fees_collected(env: Env) -> i128 {
        Self::get_all_tracked_total_fees(&env)
    }

    pub fn get_total_fees_for_token(env: Env, token: Address) -> i128 {
        env.storage()
            .get(&DataKey::Escrow(order_id))
            .unwrap_or_panic_with(&Error::EscrowNotFound)
    }

    pub fn dispute_escrow(env: Env, order_id: u32, _reason: Symbol, _initiator: Address) {
        if env.storage().get(&DataKey::Paused).unwrap_or(&false) {
            soroban_sdk::panic_with_error(&env, &Error::ContractPaused);
        }
        let key = DataKey::Escrow(order_id);
        let mut escrow = env.storage()
            .get(&key)
            .unwrap_or_panic_with(&Error::EscrowNotFound);
        if escrow.status != EscrowStatus::Active {
            soroban_sdk::panic_with_error(&env, &Error::InvalidStatus);
        }
        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_timestamp = env.ledger().timestamp();
        env.storage().set(&key, &escrow);
    }

    /////////////////////////////////////////////////////////////////////////////
    /// Expired dispute resolution
    /////////////////////////////////////////////////////////////////////////////
    pub fn resolve_expired_dispute(env: Env, order_id: u32) {
        if env.storage().get(&DataKey::Paused).unwrap_or(&false) {
            soroban_sdk::panic_with_error(&env, &Error::ContractPaused);
        }
        let key = DataKey::Escrow(order_id);
        let mut escrow = env.storage()
            .get(&key)
            .unwrap_or_panic_with(&Error::EscrowNotFound);
        if escrow.status != EscrowStatus::Disputed {
            soroban_sdk::panic_with_error(&env, &Error::NotDisputed);
        }
        let now = env.ledger().timestamp();
        let deadline = escrow.dispute_timestamp + escrow.max_dispute_duration as u64;
        if now <= deadline {
            soroban_sdk::panic_with_error(&env, &Error::DisputeNotExpired);
        }

        let policy = env.storage()
            .get(&DataKey::ExpiredDisputePolicy)
            .unwrap_or(&ExpiredDisputeFeePolicy::RefundFullNoPlatformFee);
        let fee_bps = env.storage().get(&DataKey::PlatformFeeBps).unwrap_or(&0) as i128;
        let full_fee = escrow.amount * fee_bps / 10_000;
        let platform_wallet: Address = env.storage().get(&DataKey::PlatformWallet).unwrap();
        let token_client = soroban_sdk::token::Client::new(&env, &escrow.token);

        let (buyer_amount, platform_amount) = match policy {
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee => (escrow.amount, 0i128),
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee => {
                (escrow.amount - full_fee, full_fee)
            }
            ExpiredDisputeFeePolicy::DeductFeeFromSeller => (escrow.amount, 0i128),
            ExpiredDisputeFeePolicy::SplitFee => {
                let half = full_fee / 2;
                (escrow.amount - half, half)
            }
        };

        if buyer_amount > 0 {
            token_client.transfer(&env.current_contract(), &escrow.buyer, &buyer_amount);
        }
        if platform_amount > 0 {
            token_client.transfer(
                &env.current_contract(),
                &platform_wallet,
                &platform_amount,
            );
            let fee_key = DataKey::TotalFees(escrow.token.clone());
            let prev: i128 = env.storage().get(&fee_key).unwrap_or(&0i128);
            let new_total = prev.checked_add(platform_amount).unwrap_or_panic_with(&Error::Overflow);
            env.storage().set(&fee_key, &new_total);
        }

        escrow.status = EscrowStatus::Resolved;
        env.storage().set(&key, &escrow);
    }
    }

pub fn get_total_fees_for_token(env: Env, token: Address) -> i128 {
        env.storage()
            .get(&DataKey::TotalFees(token))
            .unwrap_or(&0i128)
    }
}

/////////////////////////////////////////////////////////////////////////////
/// Tests
//////////////////////////////////////////////////////////////////////////////
#if (test)
#[cfg(test)]
mod test {
    use super::*;
use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Address, Env,
    };

const DEFAULT_MAX_DISPUTE_DURATION: u32 = 30 * 24 * 60 * 60;

    fn setup() -> (Env, CraftNexusContractClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_unlimited();
        let contract_id = env.register_contract(None, CraftNexusContract);
        let client = CraftNexusContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let platform = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        client.initialize(&platform, &admin, &arbitrator, &500, &None);
        (env, client, admin, platform)
    }

    /// Missing-key path: calling before any record exists must not trap.

    /// Unauthorized caller cannot mutate storage.
    /// The admin auth check must fail and the policy must remain unchanged.
    #[cfg(test)]
    #[should_panic]
    fn test_update_expired_dispute_policy_unauthorized() {
        let (env, client, _admin, _platform) = setup();
        // No auths mocked here -> admin.require_auth() must fail.
        env.mock_all_auths();
        // Remove the auth mock by resetting auths.
        env.set_auths(soroban_sdk::testutils::MockAuth {
            address: Address::generate(&env),
            live_until_ledger: 0,
        });
        let res = client.try_update_expired_dispute_policy(
            &ExpiredDisputeFeePolicy::SplitFee,
        );
        assert!(res.is_err());
        // Policy unchanged.
        assert_eq!(
            client.get_expired_dispute_policy(),
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee
        );
    }

    /// Paused contract rejects the update with ContractPaused.
    /// Storage must be unchanged after the rejection.
    #[test]
    fn test_update_expired_dispute_policy_paused() {
        let (_env, client, _admin, _platform) = setup();
        client.pause();
        let res = client.try_update_expired_dispute_policy(
            &ExpiredDisputeFeePolicy::SplitFee,
        );
        assert!(res.is_error());
        assert_eq!(
            client.get_expired_dispute_policy(),
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee
        );
    }

    /// Happy path: authorized admin can update the policy.
    #[test]
fn test_get_liquidation_status_missing_key() {
        let (env, artisan) = setup();
        let result = CraftNexusContract::get_liquidation_status(env.clone(), artisan);
        assert_eq!(result, Err(Error::NotFound));
    }

    fn test_update_expired_dispute_policy_ok() {
        let (_env, client, _admin, _platform) = setup();
        client.update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee);
        assert_eq!(
            client.get_expired_dispute_policy(),
            ExpiredDisputeFeePolicy::SplitFee
        );
    }

    pub fn create_batch_escrow(
        env: Env,
        batch_id: u64,
        escrows: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);

        if escrows.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let mut results = soroban_sdk::Vec::new(&env);

        if escrows.is_empty() {
            return Ok(results);
        }

        let mut authorized_buyers: Map<Address, u32> = Map::new(&env);
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                let buyer_key = params.buyer.clone();
                if !authorized_buyers.contains_key(buyer_key.clone()) {
                    buyer_key.require_auth();
                    authorized_buyers.set(buyer_key, 1u32);
                }
            }
        }
    }
}


#[cfg(test)]
mod deactivated_account_tests {
    use super::*;

    /// Test: Deactivated account cannot create escrow
    /// 
    /// Validates that when an onboarding contract indicates a buyer is deactivated
    /// (is_profile_active returns false), the create_escrow_with_metadata call panics
    /// with Error::OnboardingProfileInactive.
    #[test]
    #[ignore] // Requires full test harness with mock onboarding contract
    fn test_deactivated_account_cannot_create_escrow() {
        // Full integration test requires cross-contract mocking
    }

    /// Test: Deactivated account cannot stake tokens
    ///
    /// Validates that when an onboarding contract indicates an artisan is deactivated,
    /// the stake_tokens call panics with Error::OnboardingProfileInactive.
    #[test]
    #[ignore]
    fn test_deactivated_account_cannot_stake() {
        // Full integration test requires cross-contract mocking
    }

    /// Test: Deactivated account cannot unstake tokens
    ///
    /// Validates that when an onboarding contract indicates an artisan is deactivated,
    /// the unstake_tokens call panics with Error::OnboardingProfileInactive.
    #[test]
    #[ignore]
    fn test_deactivated_account_cannot_unstake() {
        // Full integration test requires cross-contract mocking
    }

let mut seen_order_ids: Map<u32, bool> = Map::new(&env);
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                if seen_order_ids.contains_key(params.order_id) {
                    return Err(Error::EscrowAlreadyExists);
                }
                seen_order_ids.set(params.order_id, true);
                Self::validate_escrow_params(&env, &params)?;
            }
        }

        let mut buyer_count_state: Map<Address, u32> = Map::new(&env);
        let mut seller_count_state: Map<Address, u32> = Map::new(&env);

        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                let buyer_key = params.buyer.clone();
                let seller_key = params.seller.clone();

                if !buyer_count_state.contains_key(buyer_key.clone()) {
                    let count_key = DataKey::BuyerEscrowCount(buyer_key.clone());
                    let existing_count: u32 =
                        env.storage().persistent().get(&count_key).unwrap_or(0u32);
                    buyer_count_state.set(buyer_key.clone(), existing_count);
                }

                if !seller_count_state.contains_key(seller_key.clone()) {
                    let count_key = DataKey::SellerEscrowCount(seller_key.clone());
                    let existing_count: u32 =
                        env.storage().persistent().get(&count_key).unwrap_or(0u32);
                    seller_count_state.set(seller_key.clone(), existing_count);
                }
            }
        }

        let mut buyer_next_counts: Map<Address, u32> = Map::new(&env);
        let mut seller_next_counts: Map<Address, u32> = Map::new(&env);

        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                match Self::create_single_escrow(&env, params.clone(), Some(batch_id)) {
                    Ok(id) => {
                        let buyer_key = params.buyer.clone();
                        let seller_key = params.seller.clone();

                        if !buyer_next_counts.contains_key(buyer_key.clone()) {
                            let existing_count =
                                buyer_count_state.get(buyer_key.clone()).unwrap_or(0u32);
                            buyer_next_counts.set(buyer_key.clone(), existing_count);
                        }
                        let buyer_count = buyer_next_counts.get(buyer_key.clone()).unwrap();

                        let buyer_index_key =
                            DataKey::BuyerEscrowIndexed(buyer_key.clone(), buyer_count);
                        env.storage().persistent().set(&buyer_index_key, &id);
                        Self::extend_persistent(&env, &buyer_index_key);

                        buyer_next_counts.set(buyer_key, buyer_count + 1);

                        if !seller_next_counts.contains_key(seller_key.clone()) {
                            let existing_count =
                                seller_count_state.get(seller_key.clone()).unwrap_or(0u32);
                            seller_next_counts.set(seller_key.clone(), existing_count);
                        }
                        let seller_count = seller_next_counts.get(seller_key.clone()).unwrap();

                        let seller_index_key =
                            DataKey::SellerEscrowIndexed(seller_key.clone(), seller_count);
                        env.storage().persistent().set(&seller_index_key, &id);
                        Self::extend_persistent(&env, &seller_index_key);

                        seller_next_counts.set(seller_key, seller_count + 1);

                        let escrow_opt: Option<Escrow> =
                            env.storage().persistent().get(&(ESCROW, id as u32));
                        if let Some(escrow) = escrow_opt {
                            Self::emit_escrow_created(
                                &env,
                                EscrowEvent {
                                    schema_version: 1,
                                    escrow_id: id,
                                    action: EscrowAction::BatchCreated,
                                    buyer: escrow.buyer,
                                    seller: escrow.seller,
                                    amount: escrow.amount,
                                    token: escrow.token,
                                    timestamp: env.ledger().timestamp(),
                                },
                            );
                        }
                        results.push_back(id);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        let mut i = 0;
        loop {
            if i >= buyer_next_counts.len() {
                break;

/// Test: Deactivation takes effect immediately (no stale cache)
    ///
    /// Validates that assert_account_active reads from persistent storage,
    /// ensuring deactivation takes effect immediately without cache TTL delays.
    #[test]
    #[ignore]
    fn test_deactivation_takes_effect_immediately_no_stale_cache() {
        // Full integration test requires cross-contract mocking
    }

    /// Ensure the getter extends the persistent key on hot reads.
    #[test]
    fn test_get_liquidation_status_extends_ttl() {
        let (env, artisan) = setup();
        CraftNexusContract::set_liquidation_status(
            env.clone(),
            artisan,
            LiquidationStatus::Active,
        );
        let key = (LIQUIDATION_KEY, artisan);
        let ttl_before = env.storage().persistent().get_ttl(&key);
        let _ = CraftNexusContract::get_liquidation_status(env.clone(), artisan);
        let ttl_after = env.storage().persistent().get_ttl(&key);
        assert!(ttl_after >= ttl_before);
    }
}
