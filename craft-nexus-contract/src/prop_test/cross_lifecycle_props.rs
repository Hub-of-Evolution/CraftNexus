//! Generated sequences that compose escrow, staking, onboarding, upgrades,
//! pause, migration, and reconciliation invariants in one ledger environment.
#![cfg(test)]

extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token,
    xdr::ToXdr,
    Address, BytesN, Env, String as SorobanString, Symbol,
};

use super::{
    invariants,
    model::{ModelEscrowStatus, ModelState},
    seed_from_env, Lcg64, DEFAULT_CASE_COUNT,
};
use crate::{
    onboarding::{OnboardingContract, OnboardingContractClient, ProfileStatus, UserRole},
    CraftNexusContract, CraftNexusContractClient, Error, EscrowStatus, Resolution,
};

const REGRESSION_SEEDS: &[u64] = &[0x1147_0000_0000_0001];
const ORDER_ID: u32 = 11_470;
const TOKEN_KEY: &str = "shared-token";
const BUYER_KEY: &str = "buyer";
const SELLER_KEY: &str = "seller";
const FAILED_BUYER_KEY: &str = "unfunded-buyer";
const START_TIME: u64 = 1_711_368_000;

#[derive(Clone, Debug)]
enum LifecycleOp {
    CreateEscrow { amount: i128 },
    DuplicateEscrow,
    FailedTokenTransfer { amount: i128 },
    Stake { amount: i128 },
    RejectEarlyUnstake,
    Pause,
    RejectCreateWhilePaused,
    Unpause,
    AdvanceStakeCooldown { seconds: u64 },
    Unstake,
    Dispute,
    RejectUnauthorizedResolve,
    RejectRecoveryDuringDispute,
    AdvanceDisputeTimeout { seconds: u64 },
    ChangeSellerRole,
    RejectTimeoutWithWrongRole,
    RestoreSellerRole,
    ResolveExpiredDispute,
    ProposeAndCancelUpgrade,
    MigrateStorageTwice,
    Reconcile,
}

fn generate_trace(rng: &mut Lcg64) -> Vec<LifecycleOp> {
    vec![
        LifecycleOp::CreateEscrow {
            amount: rng.next_i128_range(20_000, 2_000_000),
        },
        LifecycleOp::DuplicateEscrow,
        LifecycleOp::FailedTokenTransfer {
            amount: rng.next_i128_range(5_000_000, 10_000_000),
        },
        LifecycleOp::Stake {
            amount: rng.next_i128_range(10_000, 500_000),
        },
        LifecycleOp::RejectEarlyUnstake,
        LifecycleOp::Pause,
        LifecycleOp::RejectCreateWhilePaused,
        LifecycleOp::Unpause,
        LifecycleOp::AdvanceStakeCooldown {
            seconds: crate::time_policy::STAKE_COOLDOWN + rng.next_u64_range(1, 300),
        },
        LifecycleOp::Unstake,
        LifecycleOp::Dispute,
        LifecycleOp::RejectUnauthorizedResolve,
        LifecycleOp::RejectRecoveryDuringDispute,
        LifecycleOp::AdvanceDisputeTimeout {
            seconds: crate::time_policy::MAX_DISPUTE_DURATION + rng.next_u64_range(1, 3_600),
        },
        LifecycleOp::ChangeSellerRole,
        LifecycleOp::RejectTimeoutWithWrongRole,
        LifecycleOp::RestoreSellerRole,
        LifecycleOp::ResolveExpiredDispute,
        LifecycleOp::ProposeAndCancelUpgrade,
        LifecycleOp::MigrateStorageTwice,
        LifecycleOp::Reconcile,
    ]
}

struct Fixture {
    env: Env,
    escrow: CraftNexusContractClient<'static>,
    onboarding: OnboardingContractClient<'static>,
    token: Address,
    admin: Address,
    arbitrator: Address,
    buyer: Address,
    seller: Address,
    failed_buyer: Address,
    unauthorized: Address,
}

#[derive(Debug, PartialEq, Eq)]
struct TraceSnapshot {
    escrow_status: Option<EscrowStatus>,
    escrow_count: u32,
    total_locked: i128,
    total_staked: i128,
    seller_stake: i128,
    paused: bool,
    admin_revision: u32,
    upgrade_nonce: u32,
    storage_layout_version: u32,
    event_xdr: Vec<String>,
}

fn soroban_string(env: &Env, value: &str) -> SorobanString {
    SorobanString::from_str(env, value)
}

fn fixture(wasm_artifact: Option<&[u8]>) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger()
        .with_mut(|ledger| ledger.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    let onboarding_id = env.register_contract(None, OnboardingContract);
    let onboarding = OnboardingContractClient::new(&env, &onboarding_id);
    onboarding.initialize(&admin);

    let token_admin_address = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_address);
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);

    let escrow_id = Address::generate(&env);
    match wasm_artifact {
        Some(wasm) => {
            env.register_contract_wasm(Some(&escrow_id), wasm);
        }
        None => {
            env.register_contract(Some(&escrow_id), CraftNexusContract);
        }
    }
    let escrow = CraftNexusContractClient::new(&env, &escrow_id);
    escrow.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_id),
    );
    env.as_contract(&escrow_id, || {
        env.storage()
            .persistent()
            .set(&crate::DataKey::FallbackAdmin, &admin);
    });
    escrow.set_min_escrow_amount(&token, &0);
    escrow.set_min_release_window(&1);
    escrow.whitelist_token(&token);
    onboarding.set_escrow_contract(&escrow_id);

    onboarding.onboard_user(&buyer, &soroban_string(&env, "buyer"), &UserRole::Buyer);
    onboarding.onboard_user(&seller, &soroban_string(&env, "seller"), &UserRole::Artisan);
    let failed_buyer = Address::generate(&env);
    onboarding.onboard_user(
        &failed_buyer,
        &soroban_string(&env, FAILED_BUYER_KEY),
        &UserRole::Buyer,
    );
    token_admin.mint(&buyer, &10_000_000);
    token_admin.mint(&seller, &1_000_000);

    Fixture {
        env,
        escrow,
        onboarding,
        token,
        admin,
        arbitrator,
        buyer,
        seller,
        failed_buyer,
        unauthorized,
    }
}

fn escrow_status_matches(actual: EscrowStatus, model: ModelEscrowStatus) -> bool {
    matches!(
        (actual, model),
        (EscrowStatus::Active, ModelEscrowStatus::Active)
            | (EscrowStatus::Released, ModelEscrowStatus::Released)
            | (EscrowStatus::Refunded, ModelEscrowStatus::Refunded)
            | (EscrowStatus::Disputed, ModelEscrowStatus::Disputed)
            | (EscrowStatus::Resolved, ModelEscrowStatus::Resolved)
    )
}

fn onboarding_role_matches(actual: UserRole, model: super::model::ModelUserRole) -> bool {
    matches!(
        (actual, model),
        (UserRole::Buyer, super::model::ModelUserRole::Buyer)
            | (UserRole::Artisan, super::model::ModelUserRole::Artisan)
            | (UserRole::Moderator, super::model::ModelUserRole::Moderator)
    )
}

fn check_cross_invariants(
    fixture: &Fixture,
    model: &ModelState,
    expected_revision_floor: u32,
    expected_upgrade_nonce_floor: u32,
) -> Result<(), String> {
    let allocation = fixture.escrow.get_fund_allocation(&fixture.token);
    let model_locked = model.locked.get(TOKEN_KEY).copied().unwrap_or(0);
    let model_staked = model
        .stakes
        .get(SELLER_KEY)
        .map(|stake| stake.total)
        .unwrap_or(0);
    let checks = [
        invariants::assert_fund_conservation(&fixture.env, &fixture.escrow, &fixture.token),
        model.check_fund_conservation(),
        model
            .check_no_terminal_re_entry()
            .map_err(|error| error.to_string()),
        model
            .check_stake_queue_consistency()
            .map_err(|error| error.to_string()),
        invariants::assert_terminal_immutable(&fixture.escrow, ORDER_ID),
        match fixture.escrow.try_get_escrow(&ORDER_ID) {
            Ok(Ok(escrow))
                if matches!(
                    escrow.status,
                    EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
                ) =>
            {
                invariants::assert_no_double_settlement(
                    &fixture.escrow,
                    ORDER_ID,
                    &fixture.arbitrator,
                )
            }
            _ => Ok(()),
        },
        if allocation.total_locked == model_locked {
            Ok(())
        } else {
            Err(format!(
                "locked funds differ: contract={}, model={}",
                allocation.total_locked, model_locked
            ))
        },
        if allocation.total_staked == model_staked
            && fixture.escrow.get_stake(&fixture.seller) == model_staked
        {
            Ok(())
        } else {
            Err(format!(
                "stake differs: allocation={}, contract={}, model={}",
                allocation.total_staked,
                fixture.escrow.get_stake(&fixture.seller),
                model_staked
            ))
        },
        if allocation.balance >= allocation.total_locked + allocation.total_staked {
            Ok(())
        } else {
            Err("contract balance is below combined escrow and stake reserves".to_string())
        },
        match (
            fixture.onboarding.get_user(&fixture.seller),
            model.profiles.get(SELLER_KEY),
        ) {
            (profile, Some(expected))
                if profile.status == ProfileStatus::Active
                    && profile.active
                    && onboarding_role_matches(profile.role, expected.role) =>
            {
                Ok(())
            }
            (profile, Some(expected)) => Err(format!(
                "onboarding differs: contract=({:?}, {:?}), model=({:?}, {})",
                profile.role, profile.status, expected.role, expected.active
            )),
            (_, None) => Err("seller onboarding profile missing from model".to_string()),
        },
        if fixture.escrow.get_admin_revision() >= expected_revision_floor {
            Ok(())
        } else {
            Err("admin revision decreased".to_string())
        },
        if fixture.escrow.is_paused() == model.is_paused {
            Ok(())
        } else {
            Err("pause state differs between contract and model".to_string())
        },
        if fixture.escrow.get_upgrade_proposal_nonce() >= expected_upgrade_nonce_floor {
            Ok(())
        } else {
            Err("upgrade proposal nonce decreased".to_string())
        },
        if fixture.escrow.get_upgrade_proposal_nonce() == model.upgrade_nonce() {
            Ok(())
        } else {
            Err(format!(
                "upgrade nonce differs: contract={}, model={}",
                fixture.escrow.get_upgrade_proposal_nonce(),
                model.upgrade_nonce()
            ))
        },
        match fixture.escrow.try_get_escrow(&ORDER_ID) {
            Ok(Ok(escrow)) => match model.escrows.get(&ORDER_ID) {
                Some(expected) if escrow_status_matches(escrow.status, expected.status) => Ok(()),
                Some(expected) => Err(format!(
                    "escrow state differs: contract={:?}, model={:?}",
                    escrow.status, expected.status
                )),
                None => Err("contract escrow exists without model record".to_string()),
            },
            _ if !model.escrows.contains_key(&ORDER_ID) => Ok(()),
            _ => Err("model escrow exists but contract query failed".to_string()),
        },
        match fixture.escrow.reconcile_token(&fixture.token, &0, &1) {
            Ok(report)
                if report.complete
                    && !report.unresolved
                    && report.expected_locked == model_locked
                    && report.expected_staked == model_staked
                    && report.tracked_locked == model_locked
                    && report.tracked_staked == model_staked =>
            {
                Ok(())
            }
            Ok(report) => Err(format!("reconciliation mismatch: {:?}", report)),
            Err(error) => Err(format!("reconciliation failed: {:?}", error)),
        },
    ];
    invariants::run_invariants(&checks)
}

fn execute_trace(
    seed: u64,
    operations: &[LifecycleOp],
    wasm_artifact: Option<&[u8]>,
) -> Result<TraceSnapshot, String> {
    let fixture = fixture(wasm_artifact);
    let mut model = ModelState::new();
    model
        .onboard_user(BUYER_KEY.to_string(), super::model::ModelUserRole::Buyer)
        .map_err(|error| format!("model buyer onboarding failed: {:?}", error))?;
    model
        .onboard_user(SELLER_KEY.to_string(), super::model::ModelUserRole::Artisan)
        .map_err(|error| format!("model seller onboarding failed: {:?}", error))?;
    model
        .onboard_user(
            FAILED_BUYER_KEY.to_string(),
            super::model::ModelUserRole::Buyer,
        )
        .map_err(|error| format!("model failed-buyer onboarding failed: {:?}", error))?;
    let mut last_revision = fixture.escrow.get_admin_revision();
    let mut last_upgrade_nonce = fixture.escrow.get_upgrade_proposal_nonce();

    for (step, operation) in operations.iter().enumerate() {
        match operation {
            LifecycleOp::CreateEscrow { amount } => {
                model
                    .create_escrow(
                        BUYER_KEY.to_string(),
                        SELLER_KEY.to_string(),
                        TOKEN_KEY.to_string(),
                        *amount,
                        ORDER_ID,
                        1,
                        fixture.env.ledger().timestamp(),
                    )
                    .map_err(|error| format!("model create failed: {:?}", error))?;
                fixture.escrow.create_escrow(
                    &fixture.buyer,
                    &fixture.seller,
                    &fixture.token,
                    amount,
                    &ORDER_ID,
                    &Some(1),
                );
            }
            LifecycleOp::DuplicateEscrow => {
                let before_count = fixture.escrow.get_escrow_count();
                let duplicate_result = fixture.escrow.try_create_escrow(
                    &fixture.buyer,
                    &fixture.seller,
                    &fixture.token,
                    &1_000,
                    &ORDER_ID,
                    &Some(1),
                );
                if model
                    .create_escrow(
                        BUYER_KEY.to_string(),
                        SELLER_KEY.to_string(),
                        TOKEN_KEY.to_string(),
                        1_000,
                        ORDER_ID,
                        1,
                        fixture.env.ledger().timestamp(),
                    )
                    .is_ok()
                    || !matches!(duplicate_result, Err(Ok(Error::EscrowAlreadyExists)))
                    || fixture.escrow.get_escrow_count() != before_count
                {
                    return Err("duplicate escrow identifier was accepted".to_string());
                }
            }
            LifecycleOp::FailedTokenTransfer { amount } => {
                let before_count = fixture.escrow.get_escrow_count();
                let result = fixture.escrow.try_create_escrow(
                    &fixture.failed_buyer,
                    &fixture.seller,
                    &fixture.token,
                    amount,
                    &(ORDER_ID + 1),
                    &Some(1),
                );
                if !matches!(result, Err(Ok(Error::TokenTransferFailed)))
                    || fixture.escrow.get_escrow_count() != before_count
                {
                    return Err("failed token transfer mutated escrow state".to_string());
                }
            }
            LifecycleOp::Stake { amount } => {
                model
                    .stake(
                        SELLER_KEY.to_string(),
                        TOKEN_KEY.to_string(),
                        *amount,
                        fixture.env.ledger().timestamp(),
                    )
                    .map_err(|error| format!("model stake failed: {:?}", error))?;
                fixture
                    .escrow
                    .stake_tokens(&fixture.seller, &fixture.token, amount);
            }
            LifecycleOp::RejectEarlyUnstake => {
                if !matches!(
                    fixture
                        .escrow
                        .try_unstake_tokens(&fixture.seller, &fixture.token),
                    Err(Ok(Error::StakeCooldownActive))
                ) {
                    return Err("unstake succeeded before cooldown".to_string());
                }
            }
            LifecycleOp::Pause => {
                fixture.escrow.set_paused(&true);
                model.set_paused(true);
            }
            LifecycleOp::RejectCreateWhilePaused => {
                let model_result = model.create_escrow(
                    BUYER_KEY.to_string(),
                    SELLER_KEY.to_string(),
                    TOKEN_KEY.to_string(),
                    1_000,
                    ORDER_ID + 2,
                    1,
                    fixture.env.ledger().timestamp(),
                );
                let contract_result = fixture.escrow.try_create_escrow(
                    &fixture.buyer,
                    &fixture.seller,
                    &fixture.token,
                    &1_000,
                    &(ORDER_ID + 2),
                    &Some(1),
                );
                if model_result.is_ok()
                    || !matches!(contract_result, Err(Ok(Error::ContractPaused)))
                {
                    return Err("paused platform accepted escrow creation".to_string());
                }
            }
            LifecycleOp::Unpause => {
                fixture.escrow.set_paused(&false);
                model.set_paused(false);
            }
            LifecycleOp::AdvanceStakeCooldown { seconds } => {
                fixture.env.ledger().with_mut(|ledger| {
                    ledger.timestamp = ledger.timestamp.saturating_add(*seconds)
                });
            }
            LifecycleOp::Unstake => {
                let now = fixture.env.ledger().timestamp();
                let amount = model
                    .stakes
                    .get(SELLER_KEY)
                    .map(|stake| stake.total)
                    .unwrap_or(0);
                model
                    .unstake(SELLER_KEY, TOKEN_KEY, amount, now)
                    .map_err(|error| format!("model unstake failed: {:?}", error))?;
                fixture
                    .escrow
                    .unstake_tokens(&fixture.seller, &fixture.token);
            }
            LifecycleOp::Dispute => {
                model
                    .dispute_escrow(ORDER_ID, BUYER_KEY, fixture.env.ledger().timestamp())
                    .map_err(|error| format!("model dispute failed: {:?}", error))?;
                fixture.escrow.dispute_escrow(
                    &ORDER_ID,
                    &Symbol::new(&fixture.env, "invariant"),
                    &fixture.buyer,
                );
            }
            LifecycleOp::RejectUnauthorizedResolve => {
                let resolve_result = fixture.escrow.try_resolve_dispute(
                    &ORDER_ID,
                    &Resolution::ReleaseToSeller,
                    &fixture.unauthorized,
                );
                if model
                    .resolve_dispute(
                        ORDER_ID,
                        "unauthorized",
                        "arbitrator",
                        true,
                        fixture.env.ledger().timestamp(),
                    )
                    .is_ok()
                    || !matches!(resolve_result, Err(Ok(Error::Unauthorized)))
                {
                    return Err("unauthorized dispute resolution succeeded".to_string());
                }
            }
            LifecycleOp::RejectRecoveryDuringDispute => {
                let recovered_admin = Address::generate(&fixture.env);
                if !matches!(
                    fixture.escrow.try_recover_admin_access(&recovered_admin),
                    Err(Ok(Error::EmergencyConflictActive))
                ) || fixture.escrow.get_emergency_operation().is_some()
                {
                    return Err(
                        "admin recovery was not rejected cleanly during an active dispute"
                            .to_string(),
                    );
                }
            }
            LifecycleOp::AdvanceDisputeTimeout { seconds } => {
                fixture.env.ledger().with_mut(|ledger| {
                    ledger.timestamp = ledger.timestamp.saturating_add(*seconds)
                });
            }
            LifecycleOp::ChangeSellerRole => {
                fixture
                    .onboarding
                    .update_user_role(&fixture.seller, &UserRole::Buyer);
                model
                    .profiles
                    .get_mut(SELLER_KEY)
                    .ok_or_else(|| "seller model profile missing".to_string())?
                    .role = super::model::ModelUserRole::Buyer;
                if fixture.onboarding.get_user(&fixture.seller).role != UserRole::Buyer {
                    return Err("onboarding role change was not persisted".to_string());
                }
            }
            LifecycleOp::RejectTimeoutWithWrongRole => {
                let now = fixture.env.ledger().timestamp();
                let model_allows_timeout = model.escrows.get(&ORDER_ID).is_some_and(|escrow| {
                    escrow.status == ModelEscrowStatus::Disputed
                        && escrow.dispute_expired(now, model.max_dispute_duration)
                });
                if !matches!(
                    fixture.escrow.try_resolve_expired_dispute(&ORDER_ID),
                    Err(Ok(Error::OnboardingAuthorizationFailed))
                ) || !model_allows_timeout
                {
                    return Err(
                        "timeout settlement ignored the current onboarding role".to_string()
                    );
                }
            }
            LifecycleOp::RestoreSellerRole => {
                fixture
                    .onboarding
                    .update_user_role(&fixture.seller, &UserRole::Artisan);
                model
                    .profiles
                    .get_mut(SELLER_KEY)
                    .ok_or_else(|| "seller model profile missing".to_string())?
                    .role = super::model::ModelUserRole::Artisan;
            }
            LifecycleOp::ResolveExpiredDispute => {
                let now = fixture.env.ledger().timestamp();
                model
                    .resolve_expired_dispute(ORDER_ID, now)
                    .map_err(|error| format!("model timeout settlement failed: {:?}", error))?;
                fixture.escrow.resolve_expired_dispute(&ORDER_ID);
            }
            LifecycleOp::ProposeAndCancelUpgrade => {
                let hash = BytesN::from_array(&fixture.env, &[0x47; 32]);
                let now = fixture.env.ledger().timestamp();
                model
                    .propose_upgrade(now)
                    .map_err(|error| format!("model upgrade proposal failed: {:?}", error))?;
                let before = fixture.escrow.get_upgrade_proposal_nonce();
                fixture.escrow.propose_upgrade_wasm(&fixture.admin, &hash);
                fixture.escrow.cancel_upgrade_wasm();
                model
                    .cancel_upgrade(now)
                    .map_err(|error| format!("model upgrade cancellation failed: {:?}", error))?;
                let after = fixture.escrow.get_upgrade_proposal_nonce();
                invariants::assert_upgrade_nonce_increased(before, after)?;
            }
            LifecycleOp::MigrateStorageTwice => {
                let version = fixture.escrow.get_storage_layout_version();
                fixture.escrow.migrate_storage_layout();
                let second_migration = fixture.escrow.migrate_storage_layout();
                if fixture.escrow.get_storage_layout_version() < version || second_migration != 0 {
                    return Err("storage layout version regressed".to_string());
                }
            }
            LifecycleOp::Reconcile => {
                let report = fixture
                    .escrow
                    .reconcile_token(&fixture.token, &0, &1)
                    .map_err(|error| format!("reconciliation failed: {:?}", error))?;
                if !report.complete || report.unresolved {
                    return Err(format!("reconciliation unresolved: {:?}", report));
                }
            }
        }

        last_revision = core::cmp::max(last_revision, fixture.escrow.get_admin_revision());
        last_upgrade_nonce = core::cmp::max(
            last_upgrade_nonce,
            fixture.escrow.get_upgrade_proposal_nonce(),
        );
        check_cross_invariants(&fixture, &model, last_revision, last_upgrade_nonce).map_err(
            |error| {
                format!(
                    "seed=0x{:016X}, step={}, operation={:?}: {}",
                    seed, step, operation, error
                )
            },
        )?;
    }

    let escrow_status = fixture
        .escrow
        .try_get_escrow(&ORDER_ID)
        .ok()
        .and_then(Result::ok)
        .map(|escrow| escrow.status);
    let allocation = fixture.escrow.get_fund_allocation(&fixture.token);
    let event_xdr = fixture
        .env
        .events()
        .all()
        .iter()
        .map(|(contract, topics, data)| {
            let mut bytes = contract.to_xdr(&fixture.env).to_alloc_vec();
            bytes.extend(topics.to_xdr(&fixture.env).to_alloc_vec());
            bytes.extend(data.to_xdr(&fixture.env).to_alloc_vec());
            bytes.iter().map(|byte| format!("{byte:02x}")).collect()
        })
        .collect();
    Ok(TraceSnapshot {
        escrow_status,
        escrow_count: fixture.escrow.get_escrow_count(),
        total_locked: allocation.total_locked,
        total_staked: allocation.total_staked,
        seller_stake: fixture.escrow.get_stake(&fixture.seller),
        paused: fixture.escrow.is_paused(),
        admin_revision: fixture.escrow.get_admin_revision(),
        upgrade_nonce: fixture.escrow.get_upgrade_proposal_nonce(),
        storage_layout_version: fixture.escrow.get_storage_layout_version(),
        event_xdr,
    })
}

fn runtime_seed() -> u64 {
    std::env::var("PROP_SEED")
        .ok()
        .and_then(|seed| u64::from_str_radix(seed.trim_start_matches("0x"), 16).ok())
        .unwrap_or_else(seed_from_env)
}

fn preserve_failure_fixture(seed: u64, operations: &[LifecycleOp], details: &str) {
    let path = std::env::var_os("CROSS_LIFECYCLE_FAILURE_REPORT")
        .unwrap_or_else(|| "target/cross-lifecycle-failure.txt".into());
    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create cross-lifecycle report directory");
    }
    std::fs::write(
        path,
        format!("seed=0x{seed:016X}\n\n{details}\n\nregression_trace={operations:#?}\n"),
    )
    .expect("preserve cross-lifecycle failure fixture");
}

#[test]
fn prop_cross_lifecycle_invariants() {
    let root_seed = runtime_seed();
    let wasm_artifact = std::env::var_os("CROSS_LIFECYCLE_WASM_ARTIFACT")
        .map(std::fs::read)
        .transpose()
        .expect("read cross-lifecycle WASM artifact");
    let mut rng = Lcg64::new(root_seed);
    let mut seeds = REGRESSION_SEEDS.to_vec();
    seeds.extend((0..DEFAULT_CASE_COUNT).map(|_| rng.next_u64()));

    for seed in seeds {
        let operations = generate_trace(&mut Lcg64::new(seed));
        let native = match execute_trace(seed, &operations, None) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                preserve_failure_fixture(seed, &operations, &error);
                panic!(
                    "cross-lifecycle invariant failure: {}\nseed=0x{:016X}\ntrace={:#?}\n\
                     add this seed to REGRESSION_SEEDS; reproduce with \
                     PROP_SEED=0x{:016X} cargo test --lib prop_cross_lifecycle_invariants -- --nocapture",
                    error, seed, operations, seed
                );
            }
        };
        if let Some(wasm) = wasm_artifact.as_deref() {
            let wasm_result = execute_trace(seed, &operations, Some(wasm));
            match wasm_result {
                Ok(wasm) if native == wasm => {}
                wasm => {
                    let details = format!("native={native:#?}\nwasm={wasm:#?}");
                    preserve_failure_fixture(seed, &operations, &details);
                    panic!(
                        "native/WASM cross-lifecycle mismatch\nseed=0x{:016X}\ntrace={:#?}\n{}",
                        seed, operations, details
                    );
                }
            }
        }
    }
}
