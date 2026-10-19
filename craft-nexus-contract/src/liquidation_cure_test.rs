//! Regression tests for #1359: use real host authorization and token balances.
extern crate std;
use super::*;
use soroban_sdk::{
    testutils::{
        storage::{Instance as _, Persistent as _},
        Address as _, Events, Ledger, MockAuth, MockAuthInvoke,
    },
    token,
    xdr::ToXdr,
    Address, Bytes, Env, IntoVal, Symbol,
};

struct Fixture {
    env: Env,
    contract: Address,
    artisan: Address,
    other: Address,
    token: Address,
}

#[derive(Debug, PartialEq)]
struct State {
    persistent: std::vec::Vec<Option<Bytes>>,
    config: Option<Bytes>,
    persistent_ttls: std::vec::Vec<Option<u32>>,
    instance_ttl: u32,
    artisan_balance: i128,
    contract_balance: i128,
    events: u32,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|ledger| ledger.timestamp = 123_456);
        let contract = env.register_contract(None, CraftNexusContract);
        let artisan = Address::generate(&env);
        let other = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(other.clone())
            .address();
        let token_admin = token::StellarAssetClient::new(&env, &token);
        token_admin.mint(&artisan, &1_000);
        token_admin.mint(&contract, &200);
        env.as_contract(&contract, || {
            let config = PlatformConfig {
                platform_fee_bps: 500,
                platform_wallet: other.clone(),
                admin: other.clone(),
                arbitrator: other.clone(),
                moderator: None,
                is_paused: false,
                min_stake_required: 100,
                pending_admin: None,
                wasm_upgrade_cooldown: 604_800,
                max_dispute_duration: 2_592_000,
                stake_cooldown: 604_800,
                expired_dispute_fee_policy: ExpiredDisputeFeePolicy::RefundFullNoPlatformFee,
                min_release_window: 86_400,
                dispute_escalation_window: 259_200,
                evidence_challenge_window: 86_400,
            };
            env.storage()
                .instance()
                .set(&DataKey::PlatformConfig, &config);
            env.storage().persistent().set(
                &DataKey::ArtisanStake(artisan.clone()),
                &ArtisanStakeData {
                    amount: 100,
                    token: token.clone(),
                },
            );
            env.storage()
                .persistent()
                .set(&DataKey::ActiveObligations(artisan.clone()), &1u32);
            env.storage().persistent().set(
                &DataKey::LiquidationStatus(artisan.clone()),
                &LiquidationStatus::Liquidated,
            );
            env.storage()
                .persistent()
                .set(&DataKey::LiquidationRecordCount, &4u32);
            for (index, owner, cured) in [
                (0u32, artisan.clone(), false),
                (1, other.clone(), false),
                (3, artisan.clone(), true),
            ] {
                let id = u64::from(index);
                env.storage()
                    .persistent()
                    .set(&DataKey::LiquidationRecordIndexed(index), &id);
                env.storage().persistent().set(
                    &DataKey::LiquidationRecord(id),
                    &LiquidationRecord {
                        id,
                        artisan: owner,
                        liquidator: other.clone(),
                        seized_amount: 25,
                        executed_at: 100,
                        health_ratio_bps: 5_000,
                        cured,
                        cured_at: if cured { 110 } else { 0 },
                    },
                );
            }
        });
        Self {
            env,
            contract,
            artisan,
            other,
            token,
        }
    }

    fn client(&self) -> CraftNexusContractClient<'_> {
        CraftNexusContractClient::new(&self.env, &self.contract)
    }

    fn config(&self, paused: bool, minimum: i128) {
        self.env.as_contract(&self.contract, || {
            let mut config: PlatformConfig = self
                .env
                .storage()
                .instance()
                .get(&DataKey::PlatformConfig)
                .unwrap();
            config.is_paused = paused;
            config.min_stake_required = minimum;
            self.env
                .storage()
                .instance()
                .set(&DataKey::PlatformConfig, &config);
        });
    }

    fn stake(&self, amount: i128, legacy: bool) {
        self.env.as_contract(&self.contract, || {
            let key = DataKey::ArtisanStake(self.artisan.clone());
            if legacy {
                self.env.storage().persistent().set(&key, &amount);
                self.env.storage().persistent().set(
                    &DataKey::ArtisanStakeToken(self.artisan.clone()),
                    &self.token,
                );
            } else {
                self.env.storage().persistent().set(
                    &key,
                    &ArtisanStakeData {
                        amount,
                        token: self.token.clone(),
                    },
                );
            }
        });
    }

    fn state(&self) -> State {
        let (persistent, config, persistent_ttls, instance_ttl) =
            self.env.as_contract(&self.contract, || {
                let mut keys = std::vec![
                    DataKey::ArtisanStake(self.artisan.clone()),
                    DataKey::ArtisanStakeToken(self.artisan.clone()),
                    DataKey::ActiveObligations(self.artisan.clone()),
                    DataKey::LiquidationStatus(self.artisan.clone()),
                    DataKey::StakeHealthSnapshot(self.artisan.clone()),
                    DataKey::LiquidationRecordCount,
                ];
                for index in 0..4 {
                    keys.push(DataKey::LiquidationRecordIndexed(index));
                    keys.push(DataKey::LiquidationRecord(u64::from(index)));
                }
                let values = keys
                    .iter()
                    .map(|key| {
                        self.env
                            .storage()
                            .persistent()
                            .get::<_, soroban_sdk::Val>(key)
                            .map(|value| value.to_xdr(&self.env))
                    })
                    .collect();
                let config = self
                    .env
                    .storage()
                    .instance()
                    .get::<_, soroban_sdk::Val>(&DataKey::PlatformConfig)
                    .map(|value| value.to_xdr(&self.env));
                let ttls = keys
                    .iter()
                    .map(|key| {
                        self.env
                            .storage()
                            .persistent()
                            .has(key)
                            .then(|| self.env.storage().persistent().get_ttl(key))
                    })
                    .collect();
                (
                    values,
                    config,
                    ttls,
                    self.env.storage().instance().get_ttl(),
                )
            });
        let token = token::Client::new(&self.env, &self.token);
        State {
            persistent,
            config,
            persistent_ttls,
            instance_ttl,
            artisan_balance: token.balance(&self.artisan),
            contract_balance: token.balance(&self.contract),
            events: self.env.events().all().len(),
        }
    }

    fn rejects(&self, error: Error) {
        let before = self.state();
        assert_eq!(
            self.client().try_cure_liquidation(&self.artisan),
            Err(Ok(error))
        );
        assert_eq!(self.state(), before);
    }
}

#[test]
fn cure_rejects_paused_platform_without_mutation() {
    let f = Fixture::new();
    f.config(true, 100);
    f.rejects(Error::ContractPaused);
}

#[test]
fn cure_requires_artisan_authorization_without_mutation() {
    let f = Fixture::new();
    f.env.set_auths(&[]);
    let before = f.state();
    assert!(matches!(
        f.client().try_cure_liquidation(&f.artisan),
        Err(Err(_))
    ));
    assert_eq!(f.state(), before);
}

#[test]
fn cure_rejects_another_accounts_authorization() {
    let f = Fixture::new();
    let before = f.state();
    let args = (f.artisan.clone(),).into_val(&f.env);
    let result = f
        .client()
        .mock_auths(&[MockAuth {
            address: &f.other,
            invoke: &MockAuthInvoke {
                contract: &f.contract,
                fn_name: "cure_liquidation",
                args,
                sub_invokes: &[],
            },
        }])
        .try_cure_liquidation(&f.artisan);
    assert!(matches!(result, Err(Err(_))));
    assert_eq!(f.state(), before);
}

#[test]
fn cure_rejects_insufficient_stake_without_migrating_legacy_storage() {
    let f = Fixture::new();
    f.stake(99, true);
    f.rejects(Error::InsufficientStake);
}

#[test]
fn cure_rejects_insufficient_current_stake_without_mutation() {
    let f = Fixture::new();
    f.stake(99, false);
    f.rejects(Error::InsufficientStake);
}

#[test]
fn cure_rejects_collateral_overflow_even_with_maximum_stake() {
    let f = Fixture::new();
    f.config(false, i128::MAX);
    f.stake(i128::MAX, false);
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ActiveObligations(f.artisan.clone()), &2u32);
    });
    f.rejects(Error::CounterOverflow);
}

#[test]
fn cure_rejects_negative_collateral_inputs() {
    let f = Fixture::new();
    f.stake(-1, false);
    f.rejects(Error::InsufficientStake);
    f.stake(100, false);
    f.config(false, -1);
    f.rejects(Error::InsufficientStake);
}

#[test]
fn cure_rejects_healthy_or_missing_status() {
    let f = Fixture::new();
    f.env.as_contract(&f.contract, || {
        f.env.storage().persistent().set(
            &DataKey::LiquidationStatus(f.artisan.clone()),
            &LiquidationStatus::Healthy,
        );
    });
    f.rejects(Error::NotLiquidationEligible);
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::LiquidationStatus(f.artisan.clone()));
    });
    f.rejects(Error::NotLiquidationEligible);
}

#[test]
fn cure_rejects_missing_platform_configuration() {
    let f = Fixture::new();
    f.env.as_contract(&f.contract, || {
        f.env.storage().instance().remove(&DataKey::PlatformConfig)
    });
    f.rejects(Error::PlatformNotInitialized);
}

#[test]
fn cure_at_exact_collateral_boundary_only_marks_own_open_records() {
    for status in [
        LiquidationStatus::UnderCollateralized,
        LiquidationStatus::LiquidationEligible,
        LiquidationStatus::Liquidated,
    ] {
        let f = Fixture::new();
        f.env.as_contract(&f.contract, || {
            f.env
                .storage()
                .persistent()
                .set(&DataKey::LiquidationStatus(f.artisan.clone()), &status);
        });
        let before = f.state();
        f.client().cure_liquidation(&f.artisan);
        assert_eq!(f.env.auths()[0].0, f.artisan);
        let after = f.state();
        assert_eq!(before.artisan_balance, after.artisan_balance);
        assert_eq!(before.contract_balance, after.contract_balance);
        assert_eq!(before.config, after.config);
        // Only status (index 3) and the artisan's open record (index 7) change.
        for (index, value) in before.persistent.iter().enumerate() {
            if index != 3 && index != 7 {
                assert_eq!(Some(value), after.persistent.get(index));
            }
        }
        f.env.as_contract(&f.contract, || {
            assert_eq!(
                f.env
                    .storage()
                    .persistent()
                    .get::<_, LiquidationStatus>(&DataKey::LiquidationStatus(f.artisan.clone())),
                Some(LiquidationStatus::Healthy)
            );
            let record: LiquidationRecord = f
                .env
                .storage()
                .persistent()
                .get(&DataKey::LiquidationRecord(0))
                .unwrap();
            assert!(record.cured);
            assert_eq!(record.cured_at, 123_456);
        });
        assert_eq!(after.events, before.events + 1);
        assert_eq!(
            f.env.events().all().last().unwrap().1,
            (
                Symbol::new(&f.env, "stake_liquidation_cured"),
                f.artisan.clone()
            )
                .into_val(&f.env)
        );
        f.rejects(Error::NotLiquidationEligible);
    }
}

#[test]
fn cure_accepts_no_remaining_obligations_without_moving_tokens() {
    let f = Fixture::new();
    f.config(false, i128::MAX);
    f.stake(0, true);
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::ActiveObligations(f.artisan.clone()))
    });
    let before = f.state();
    f.client().cure_liquidation(&f.artisan);
    let after = f.state();
    assert_eq!(after.artisan_balance, before.artisan_balance);
    assert_eq!(after.contract_balance, before.contract_balance);
    assert_eq!(after.persistent[0], before.persistent[0]);
    assert_eq!(after.persistent[1], before.persistent[1]);
}

#[test]
fn cure_pause_gate_precedes_missing_authorization() {
    let f = Fixture::new();
    f.config(true, 100);
    f.env.set_auths(&[]);
    f.rejects(Error::ContractPaused);
}

#[test]
fn cure_accepts_maximum_stake_without_scaled_ratio_overflow() {
    let f = Fixture::new();
    f.config(false, i128::MAX);
    f.stake(i128::MAX, false);
    f.client().cure_liquidation(&f.artisan);
    f.rejects(Error::NotLiquidationEligible);
}
