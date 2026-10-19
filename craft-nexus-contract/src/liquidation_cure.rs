//! Cure validation is read-only until every rejection gate has passed.

use super::{
    CraftNexusContract, DataKey, Error, LiquidationRecord, LiquidationStatus, PlatformConfig,
};
use soroban_sdk::{panic_with_error, Address, Env, Symbol};

pub(crate) fn cure_liquidation(env: &Env, artisan: &Address) -> Result<(), Error> {
    CraftNexusContract::check_not_paused(env);
    artisan.require_auth();

    let status_key = DataKey::LiquidationStatus(artisan.clone());
    let status: LiquidationStatus = env
        .storage()
        .persistent()
        .get(&status_key)
        .unwrap_or(LiquidationStatus::Healthy);
    if status == LiquidationStatus::Healthy {
        panic_with_error!(env, Error::NotLiquidationEligible);
    }

    // Do not use evaluate_stake_health/get_stake here: they persist a health
    // snapshot or migrate legacy stake before an insufficient-stake rejection.
    // Reading directly also avoids refreshing TTLs on rejected calls.
    let config: PlatformConfig = env
        .storage()
        .instance()
        .get(&DataKey::PlatformConfig)
        .unwrap_or_else(|| panic_with_error!(env, Error::PlatformNotInitialized));
    let stake = CraftNexusContract::get_artisan_stake_read_only(env, artisan.clone())
        .map(|record| record.amount)
        .unwrap_or(0);
    let obligations: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveObligations(artisan.clone()))
        .unwrap_or(0);

    // Reject malformed negative collateral rather than letting it cure an
    // account. Never saturate required collateral: overflow is not a cure.
    if stake < 0 || config.min_stake_required < 0 {
        panic_with_error!(env, Error::InsufficientStake);
    }
    let required = i128::from(obligations)
        .checked_mul(config.min_stake_required)
        .unwrap_or_else(|| panic_with_error!(env, Error::CounterOverflow));
    let deficit = required
        .checked_sub(stake)
        .unwrap_or_else(|| panic_with_error!(env, Error::CounterUnderflow));
    if deficit > 0 && obligations > 0 {
        panic_with_error!(env, Error::InsufficientStake);
    }

    // All rejection gates are complete. Cure only changes status/audit records;
    // it neither moves tokens nor changes stake, obligations or record counts.
    env.storage()
        .persistent()
        .set(&status_key, &LiquidationStatus::Healthy);
    CraftNexusContract::extend_persistent(env, &status_key);

    let count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LiquidationRecordCount)
        .unwrap_or(0);
    for index in 0..count {
        if let Some(id) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::LiquidationRecordIndexed(index))
        {
            let record_key = DataKey::LiquidationRecord(id);
            if let Some(mut record) = env
                .storage()
                .persistent()
                .get::<DataKey, LiquidationRecord>(&record_key)
            {
                if record.artisan == *artisan && !record.cured {
                    record.cured = true;
                    record.cured_at = env.ledger().timestamp();
                    env.storage().persistent().set(&record_key, &record);
                    CraftNexusContract::extend_persistent(env, &record_key);
                }
            }
        }
    }

    env.events().publish(
        (Symbol::new(env, "stake_liquidation_cured"), artisan.clone()),
        env.ledger().timestamp(),
    );
    Ok(())
}
