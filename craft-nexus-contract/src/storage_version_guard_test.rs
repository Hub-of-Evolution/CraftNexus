//! Storage Layout Version Guard Tests (#1116)
//!
//! Verifies that contract methods reject records whose serialized layout
//! version is not understood by the running code.
//!
//! Acceptance criteria:
//! - Unsupported layouts fail before interpretation or mutation.
//! - Version checks cover escrow, stake, onboarding, and governance records.
//! - Tests include malformed (future-version) and legacy (no-version) fixtures.

#[cfg(test)]
mod storage_version_guard_tests {
    use crate::{
        ArtisanStakeData, DataKey, Error, EscrowContract, EscrowContractClient,
        CURRENT_ESCROW_VERSION, CURRENT_STAKE_VERSION,
    };
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Bytes, BytesN, Env, Map, Symbol, Val,
    };

    // ── helpers ─────────────────────────────────────────────────────────────

    fn setup() -> (Env, Address, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        let platform_wallet = Address::generate(&env);

        // Register and initialise the escrow contract.
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(
            &admin,
            &250u32,          // fee bps
            &platform_wallet,
            &None::<Address>, // no onboarding contract
        );

        (env, contract_id, admin, buyer, seller, platform_wallet)
    }

    /// Register a minimal token contract and whitelist it, returning the token address.
    fn setup_token(
        env: &Env,
        client: &EscrowContractClient,
        admin: &Address,
        holder: &Address,
        amount: i128,
    ) -> Address {
        use soroban_sdk::testutils::Address as _;
        let token_admin = Address::generate(env);
        let token_contract =
            env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = token_contract.address();
        let token_client =
            soroban_sdk::token::StellarAssetClient::new(env, &token_id);
        token_client.mint(holder, &amount);
        // Whitelist: 7 decimals
        client.whitelist_token(admin, &token_id, &7u32);
        token_id
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 1. ESCROW VERSION GUARDS
    // ═══════════════════════════════════════════════════════════════════════

    /// Reading an escrow whose version field is exactly `CURRENT_ESCROW_VERSION`
    /// succeeds normally (baseline sanity check).
    #[test]
    fn test_escrow_current_version_accepted() {
        let (env, contract_id, admin, buyer, seller, platform_wallet) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 10_000);

        client.create_escrow(&buyer, &seller, &token, &1_000i128, &86_400u32, &None);
        // get_escrow triggers the read path; should not panic.
        let escrow = client.get_escrow(&1u32);
        assert_eq!(escrow.version, CURRENT_ESCROW_VERSION);
    }

    /// Injecting a future escrow version (CURRENT + 1) causes `get_escrow` to
    /// panic with `UnsupportedEscrowVersion` before any field is interpreted.
    #[test]
    #[should_panic]
    fn test_escrow_future_version_rejected() {
        let (env, contract_id, admin, buyer, seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 10_000);
        client.create_escrow(&buyer, &seller, &token, &1_000i128, &86_400u32, &None);

        // Manually overwrite the stored record with an unsupported version.
        env.as_contract(&contract_id, || {
            let key = (Symbol::short("ESCROW"), 1u32);
            let mut raw: soroban_sdk::Map<Symbol, Val> = env
                .storage()
                .persistent()
                .get(&key)
                .expect("escrow exists");
            let future_version: u32 = CURRENT_ESCROW_VERSION + 1;
            raw.set(Symbol::new(&env, "version"), future_version.into_val(&env));
            env.storage().persistent().set(&key, &raw);
        });

        // This must panic with UnsupportedEscrowVersion.
        let _ = client.get_escrow(&1u32);
    }

    /// Attempting to release funds from a future-version escrow is rejected
    /// before any state mutation occurs.
    #[test]
    fn test_escrow_future_version_blocks_release() {
        let (env, contract_id, admin, buyer, seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 10_000);
        client.create_escrow(&buyer, &seller, &token, &1_000i128, &86_400u32, &None);

        env.as_contract(&contract_id, || {
            let key = (Symbol::short("ESCROW"), 1u32);
            let mut raw: soroban_sdk::Map<Symbol, Val> = env
                .storage()
                .persistent()
                .get(&key)
                .expect("escrow exists");
            let future: u32 = CURRENT_ESCROW_VERSION + 99;
            raw.set(Symbol::new(&env, "version"), future.into_val(&env));
            env.storage().persistent().set(&key, &raw);
        });

        let result = client.try_release_funds(&buyer, &1u32);
        assert!(
            result.is_err(),
            "release_funds must fail on a future-version escrow"
        );
        let err = result.unwrap_err().unwrap();
        assert_eq!(err, Error::UnsupportedEscrowVersion);
    }

    /// Legacy escrow records (no `version` key in the map) are transparently
    /// upgraded to the current version on read — they are not rejected.
    #[test]
    fn test_escrow_legacy_no_version_key_migrated() {
        let (env, contract_id, admin, buyer, seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 10_000);
        client.create_escrow(&buyer, &seller, &token, &1_000i128, &86_400u32, &None);

        // Strip the `version` key to simulate a legacy pre-versioned record.
        env.as_contract(&contract_id, || {
            let key = (Symbol::short("ESCROW"), 1u32);
            let mut raw: soroban_sdk::Map<Symbol, Val> = env
                .storage()
                .persistent()
                .get(&key)
                .expect("escrow exists");
            raw.remove(Symbol::new(&env, "version"));
            env.storage().persistent().set(&key, &raw);
        });

        // Must succeed and return the current version after migration.
        let escrow = client.get_escrow(&1u32);
        assert_eq!(escrow.version, CURRENT_ESCROW_VERSION);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 2. STAKE VERSION GUARDS
    // ═══════════════════════════════════════════════════════════════════════

    /// `get_artisan_stake_data` on a current-version record returns the record.
    #[test]
    fn test_stake_current_version_accepted() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);

        client.stake_tokens(&buyer, &token, &10_000i128);
        let data = client.get_artisan_stake_data(&buyer).expect("stake exists");
        assert_eq!(data.version, CURRENT_STAKE_VERSION);
        assert_eq!(data.amount, 10_000);
    }

    /// Injecting a future stake version causes `get_artisan_stake_data` to
    /// panic with `UnsupportedStakeVersion`.
    #[test]
    #[should_panic]
    fn test_stake_future_version_rejected_on_read() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &10_000i128);

        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            let future = ArtisanStakeData {
                version: CURRENT_STAKE_VERSION + 1,
                amount: 10_000,
                token: token.clone(),
            };
            env.storage().persistent().set(&key, &future);
        });

        let _ = client.get_artisan_stake_data(&buyer);
    }

    /// A future-version stake record also blocks `stake_tokens` mutation.
    #[test]
    fn test_stake_future_version_blocks_stake_tokens() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &10_000i128);

        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            let future = ArtisanStakeData {
                version: CURRENT_STAKE_VERSION + 5,
                amount: 10_000,
                token: token.clone(),
            };
            env.storage().persistent().set(&key, &future);
        });

        let result = client.try_stake_tokens(&buyer, &token, &1_000i128);
        assert!(result.is_err(), "stake_tokens must fail on future-version stake");
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedStakeVersion
        );
    }

    /// A future-version stake record blocks `unstake_tokens` mutation.
    #[test]
    fn test_stake_future_version_blocks_unstake_tokens() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &10_000i128);

        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            let future = ArtisanStakeData {
                version: CURRENT_STAKE_VERSION + 99,
                amount: 10_000,
                token: token.clone(),
            };
            env.storage().persistent().set(&key, &future);
        });

        let result = client.try_unstake_tokens(&buyer, &token);
        assert!(result.is_err(), "unstake_tokens must fail on future-version stake");
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedStakeVersion
        );
    }

    /// Legacy stake records (no `version` field) are lazily migrated to
    /// `CURRENT_STAKE_VERSION` on first read — not rejected.
    #[test]
    fn test_stake_legacy_no_version_migrated() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &10_000i128);

        // Write a legacy record with no version field.
        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            // LegacyArtisanStakeData is private; build via Map to mimic the layout.
            let mut m: Map<Symbol, Val> = Map::new(&env);
            m.set(Symbol::new(&env, "amount"), 10_000i128.into_val(&env));
            m.set(Symbol::new(&env, "token"), token.to_val());
            env.storage().persistent().set(&key, &m);
        });

        // Must succeed; lazy migration sets version = CURRENT_STAKE_VERSION.
        let data = client.get_artisan_stake_data(&buyer).expect("stake exists");
        assert_eq!(data.version, CURRENT_STAKE_VERSION);
        assert_eq!(data.amount, 10_000);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3. ONBOARDING VERSION GUARDS
    // ═══════════════════════════════════════════════════════════════════════
    //
    // The onboarding contract already has `assert_profile_version_supported`
    // (introduced in #1056). These tests exercise the guard via the onboarding
    // client to confirm it rejects future-version profiles.

    #[cfg(not(target_family = "wasm"))]
    mod onboarding_version_guards {
        use crate::onboarding::{
            OnboardingContract, OnboardingContractClient, UserRole,
        };
        use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

        fn setup_onboarding() -> (Env, Address, OnboardingContractClient<'static>) {
            let env = Env::default();
            env.mock_all_auths();
            let admin = Address::generate(&env);
            let contract_id = env.register_contract(None, OnboardingContract);
            let client = OnboardingContractClient::new(&env, &contract_id);
            client.initialize(&admin, &None, &None, &None);
            (env, admin, client)
        }

        /// A profile at the current version is read normally.
        #[test]
        fn test_profile_current_version_accepted() {
            let (env, _admin, client) = setup_onboarding();
            let user = Address::generate(&env);
            client.onboard_user(&user, &Symbol::new(&env, "artisan_01"), &UserRole::Artisan);
            let p = client.get_user(&user);
            // version should be at current level, not 0 or future
            assert!(p.version > 0);
        }

        /// Injecting a future profile version causes `get_user` to panic.
        #[test]
        #[should_panic]
        fn test_profile_future_version_rejected() {
            use crate::onboarding::DataKey as OnboardingDataKey;
            let (env, _admin, client) = setup_onboarding();
            let user = Address::generate(&env);
            client.onboard_user(&user, &Symbol::new(&env, "artisan_02"), &UserRole::Artisan);

            // Overwrite the stored profile with a future version number.
            env.as_contract(&client.address, || {
                let key = OnboardingDataKey::UserProfile(user.clone());
                // Read current stored bytes, bump version field to a large future value.
                let mut profile: soroban_sdk::Map<Symbol, soroban_sdk::Val> = env
                    .storage()
                    .persistent()
                    .get(&key)
                    .expect("profile exists");
                profile.set(Symbol::new(&env, "version"), 9999u32.into_val(&env));
                env.storage().persistent().set(&key, &profile);
            });

            // Must panic with UnsupportedProfileVersion.
            let _ = client.get_user(&user);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 4. GOVERNANCE VERSION GUARDS
    // ═══════════════════════════════════════════════════════════════════════

    /// Injecting a future `version` key into a stored `WasmUpgradeProposal`
    /// causes `execute_upgrade` to panic with `UnsupportedGovernanceVersion`.
    #[test]
    fn test_upgrade_proposal_future_version_rejected() {
        let (env, contract_id, admin, _buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        // Set up storage layout so execute_upgrade can reach the proposal check.
        client.migrate_storage_layout();

        // Propose an upgrade (dummy hash).
        let wasm_hash = BytesN::from_array(&env, &[0xabu8; 32]);
        client.propose_upgrade_wasm(&admin, &wasm_hash);

        // Inject a future version key into the stored proposal.
        env.as_contract(&contract_id, || {
            let raw: soroban_sdk::Val = env
                .storage()
                .persistent()
                .get_unchecked(&DataKey::WasmUpgradeProposal)
                .expect("proposal exists");
            let mut map =
                soroban_sdk::Map::<Symbol, soroban_sdk::Val>::try_from_val(&env, &raw)
                    .expect("proposal map");
            map.set(Symbol::new(&env, "version"), 999u32.into_val(&env));
            env.storage()
                .persistent()
                .set(&DataKey::WasmUpgradeProposal, &map);
        });

        // Advance time past the upgrade cooldown.
        env.ledger().set_timestamp(1_000_000 + 60 * 60 * 25);

        let result = client.try_execute_upgrade(&wasm_hash);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedGovernanceVersion
        );
    }

    /// Injecting a future version into a stored `AdminActionProposal` causes
    /// execution to be rejected.
    #[test]
    fn test_admin_action_future_version_rejected() {
        let (env, contract_id, admin, _buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        // Propose a simple admin action (pause platform).
        client.propose_admin_action(
            &admin,
            &crate::AdminActionKind::PausePlatform(true),
        );

        // Overwrite the stored action with a future version.
        env.as_contract(&contract_id, || {
            use crate::AdminActionDataKey;
            let raw: soroban_sdk::Val = env
                .storage()
                .persistent()
                .get_unchecked(&AdminActionDataKey::AdminAction(1u64))
                .expect("action exists");
            let mut map =
                soroban_sdk::Map::<Symbol, soroban_sdk::Val>::try_from_val(&env, &raw)
                    .expect("action map");
            map.set(Symbol::new(&env, "version"), 888u32.into_val(&env));
            env.storage()
                .persistent()
                .set(&AdminActionDataKey::AdminAction(1u64), &map);
        });

        // Advance past timelock.
        env.ledger().set_timestamp(1_000_000 + 60 * 60 * 25);

        let result = client.try_execute_admin_action(&admin, &1u64);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedGovernanceVersion
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 5. BOUNDARY / EDGE CASES
    // ═══════════════════════════════════════════════════════════════════════

    /// Version exactly equal to the current constant is always accepted (not
    /// treated as "future").
    #[test]
    fn test_version_at_current_boundary_accepted() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &5_000i128);

        // Overwrite with version == CURRENT_STAKE_VERSION (boundary, must pass).
        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            let exact = ArtisanStakeData {
                version: CURRENT_STAKE_VERSION,
                amount: 5_000,
                token: token.clone(),
            };
            env.storage().persistent().set(&key, &exact);
        });

        let data = client.get_artisan_stake_data(&buyer).expect("stake exists");
        assert_eq!(data.version, CURRENT_STAKE_VERSION);
    }

    /// Version == `CURRENT + 1` (one above boundary) is always rejected.
    #[test]
    fn test_version_one_above_boundary_rejected_for_stake() {
        let (env, contract_id, admin, buyer, _seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 50_000);
        client.stake_tokens(&buyer, &token, &5_000i128);

        env.as_contract(&contract_id, || {
            let key = DataKey::ArtisanStake(buyer.clone());
            let future = ArtisanStakeData {
                version: CURRENT_STAKE_VERSION + 1,
                amount: 5_000,
                token: token.clone(),
            };
            env.storage().persistent().set(&key, &future);
        });

        let result = client.try_get_artisan_stake_data(&buyer);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedStakeVersion
        );
    }

    /// `u32::MAX` version is rejected with the same error — no integer overflow.
    #[test]
    fn test_version_u32_max_rejected_for_escrow() {
        let (env, contract_id, admin, buyer, seller, _pw) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        let token = setup_token(&env, &client, &admin, &buyer, 10_000);
        client.create_escrow(&buyer, &seller, &token, &1_000i128, &86_400u32, &None);

        env.as_contract(&contract_id, || {
            let key = (Symbol::short("ESCROW"), 1u32);
            let mut raw: soroban_sdk::Map<Symbol, Val> = env
                .storage()
                .persistent()
                .get(&key)
                .expect("escrow exists");
            raw.set(Symbol::new(&env, "version"), u32::MAX.into_val(&env));
            env.storage().persistent().set(&key, &raw);
        });

        let result = client.try_get_escrow(&1u32);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            Error::UnsupportedEscrowVersion
        );
    }
}
