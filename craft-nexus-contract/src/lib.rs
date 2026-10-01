use soroban_env::{Env, Symbol};
use soroban_sdk::{address, contract, contracterror, contracttype, environment::Env as _, into_val, panic_with, symbol_short, to_val, Address, Env as EnvClient, String};

const PAUSED_KEY: Symbol = symbol_short("paused");
const PAUSED_KEY_PERSISTENT: Symbol = symbol_short("paused_p");
const MAX_DISPUTING_DURATION_KEY: symbol_short!("MaxDipDur");

const PAUSED_TTL: u32 = 172800;
const DEFAULT_MAx_DISPUTE_DURATION: u64 = 60; // 60 seconds

/// Centralised time-boundary policy for the contract.
pub mod time_policy;

pub struct CraftNexusContract;

/// Error types for the craft-nexus contract.
const ERROR_NOT_INITIALIZED: u32 = 1;
const ERROR_INVALID_DURATION: u32 = 2;

trait Error {
    fn code(&Self) -> u32;
    fn message(&Self) -> String;
}

pub struct NotInitialized;

/// Verifies that the public error codes are stable across upgrades.
    /// ABI compatibility requires that error discriminants never change.
    #[test]
    fn error_discriminants_are_stable() {
        assert_eq!(Error::Unauthorized as u32, 4);
        assert_eq!(Error::NotFound as u32, 1);
        assert_eq!(Error::Paused as u32, 2);
        assert_eq!(Error::NotPaused as u32, 3);
    }
}

pub type Result<T> = core::result::Result<T, Error>;

/// Read-only query for the platform pause state.
///
/// Returns `Ok(bool)` with the current pause state when the record exists.
/// When the storage key is absent (e.g. after archival, partial migration,
/// or a missing key), returns `Err(Error::NotFound)` instead of panicking.
/// Hot persistent keys are extended on read via `extend_persistent_read`.
pub fn is_paused(env: &Env) -> Result<bool> {
    let key = paused_key(env);
    match env.storage().persistent().get::bool(&key) {
        Some(paused) => {
            env.storage().persistent().extend_ttl(&key, PAUSED_TTL);
            Ok(paused)
        }
        None => Err(Error::NotFound),
    }
}

pub struct InvalidDuration;

impl Error for InvalidDuration {
    fn code(&Self) -> u32 {
        ERROR_INVALID_DURATION
    }
    fn message(&Self) -> String {
        String::from_str("invalid max dispute duration")
    }
}

#[derive(Clone, Debug, Eq,PartialEq)]
pub enum ContractError {
    NotInitialized,
    InvalidDuration,
}

pub type Result<T> = core::result::Result<T, ContractError>;

/// Returns the storage key used for the pause flag.
///
/// Prefers the persistent key when it exists, falling back to the legacy
/// instance key for older deployments. This keeps the query safe across
/// migration boundaries without panicking.
fn paused_key(env: &Env) -> Symbol {
    let persistent = env.storage().persistent();
    if persistent.has(&PAUSED_KEY_PERSISTENT) {
        PAUSED_KEY_PERSISTENT
    } else {
        PAUSED_KEY
    }
}

/// Storage key for the maximum dispute duration.
pub fn max_dispute_duration_key() -> symbol_short {
    MAX_DISPUTE_DURATION_KEY
}

/// Sets the pause flag. Reserved for governance administration.
pub fn set_paused(env: &Env, paused: bool) -> Result<unit> {
    let key = PAUSED_KEY_PERSISTENT;
    env.storage().persistent().set(&key, &paused);
    env.storage().persistent().extend_ttl(&key, PAUSED_TTL);
    Ok(())
}

/// Returns the current maximum dispute duration in seconds.
///
/// Returns `Err(ContractError::NotInitialized)` when the key is absent,
/// e.g. after archival or a partial migration. This function must never trap.
pub fn get_max_dispute_duration(env: &Env) -> Result<u64> {
    let key = max_dispute_duration_key();
    // Use extend_persistent_read to avoid panicking on hot persistent keys.
    env.extend_persistent_read(&key);
    match env.storage().persistent().get::<_, u64>(&key) {
        Some(duration) => {
            if duration == 0 {
                Err(ContractError::InvalidDuration)
            } else {
                Ok(duration)
            }
        }
        None => Err(ContractError::NotInitialized),
    }
}

/// Convenience guard that fails with `Error::Paused` when the platform is paused.
/// Propagates `Error::NotFound` if the pause record has not been initialized yet.
pub fn require_not_paused(env: &Env) -> Result<()> {
    if is_paused(env)? {
        Err(Error::Paused)
    } else {
        Ok(())
    }
}
    }

/// Sets the maximum dispute duration in seconds.
pub fn set_max_dispute_duration(env: &Env, duration: u64) -> Result<u64> {
    if duration == 0 {
        return Err(ContractError::InvalidDuration);
    }

#[contract]
impl CraftNexusContract {
    /// Read-only entry point for the platform pause state.
    /// Returns `Err(Error::NotFound)` when the storage key is absent.
    pub fn is_paused(env: Env) -> Result<bool> {
        is_paused(&env)
    }
let key = max_dispute_duration_key();
    env.storage().persistent().set(&key, &duration);
    env.extend_persistent_read(&key);
    Ok(duration)
}

/// Clears the max dispute duration, modeling a terminal state or archival.
pub fn clear_max_dispute_duration(env: &Env) {
    let key = max_dispute_duration_key();
    env.storage().persistent().remove(&key);
}

/// Sets the pause flag.
pub fn set_paused(env: Env, paused: bool) -> Result<unit> {
    set_paused(&env, paused)
}
}

#[contract]
pub struct CraftNexusContract;

#[impl]
pub impl CraftNexusContract {
    pub fn get_max_dispute_duration(env: &Env) -> Result<u64> {
        get_max_dispute_duration(env)
    }
}

#[config]
mod test {
    use super::*;
    use soroban_sdk::Env;

#[test]
    fn is_paused_returns_not_found_before_record_exists() {
        let env = Env::default();
        let result = is_paused(&env);
        assert_eq!(result, Err(Error::NotFound));
    }

#[test]
    fn is_paused_returns_value_after_terminal_state() {
        let env = Env::default();
        set_paused(&env, true).unwrap();
        assert_eq!(is_paused(&env).unwrap(), true);
    }

    pub fn clear_max_dispute_duration(env: &Env) {
        clear_max_dispute_duration(env)
    }
}

set_paused(&env, false).unwrap();
        assert_eq!(is_paused(&env).unwrap(), false);
    }
}

#[test]
    fn is_paused_extends_ttl_on_read() {
        let env = Env::default();
        set_paused(&env, true).unwrap();
        let before = env.storage().persistent().ttl(&PAUSED_KEY_PERSISTENT);
        let _ = is_paused(&env).unwrap();
        let after = env.storage().persistent().ttl(&PAUSED_KEY_PERSISTENT);
        assert!(after >= before);
    }

    #[test]
    fn get_max_dispute_duration_missing_key_returns_error() {
        let env = Env::default();
        let result = get_max_dispute_duration(&env);
        assert_eq!(result, Err(ContractError::NotInitialized));
    }
    }

#[test]
    fn is_paused_falls_back_to_legacy_key() {
        let env = Env::default();
        env.storage().persistent().set(&PAUSED_KEY, &true);
        assert_eq!(is_paused(&env).unwrap(), true);
    }

    #[test]
    fn get_max_dispute_duration_after_terminal_state_returns_error() {
        let env = Env::default();
        set_max_dispute_duration(&env, 120).unwrap();
        assert_eq!(get_max_dispute_duration(&env), Ok(120));
        clear_max_dispute_duration(&env);
        assert_eq!(
            get_max_dispute_duration(&env),
            Err(ContractError::NotInitialized)
        );
    }
    }
/// Healthy ──► UnderCollateralized ──► LiquidationEligible ──► Liquidated
///    ▲                   │                       │               │
///    │               cure_liquidation()       cure_liquidation() │
///    └─────────────────┘                       └───────────────┘
///
}
/// platform_fee + seller_amount + buyer_amount == escrow_amount
///
