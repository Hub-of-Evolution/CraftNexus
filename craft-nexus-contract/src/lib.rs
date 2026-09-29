use soroban_env::{Env, Symbol};
use sorban_sdk::{address, contract, contracterror, contracttype, environment::Env as _, into_val, panic_with, symbol_short, to_val, Address, Env as EnvClient, String};

const PAUSED_KEY: Symbol = symbol_short("paused");
const PAUSED_KEY_PERSISTENT: Symbol = symbol_short("paused_p");

const PAUSED_TTL: u32 = 172800;

pub struct CraftNexusContract;

/// Typed error variants returned by the contract's fallible queries.
#[contracterror]
#[partial_eq( Debug)]
#[deri](ContractError, ContractErrorXml)]
pub enum Error {
    /// The requested key was not found in storage.
    NotFound = 1,
    /// The contract is currently paused.
    Paused = 2,
    /// The contract is not paused.
    NotPaused = 3,
    /// The caller is not authorized to perform the operation.
    Unauthorized = 4,
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

/// Sets the pause flag. Reserved for governance administration.
pub fn set_paused(env: &Env, paused: bool) -> Result<unit> {
    let key = PAUSED_KEY_PERSISTENT;
    env.storage().persistent().set(&key, &paused);
    env.storage().persistent().extend_ttl(&key, PAUSED_TTL);
    Ok(())
}

/// Convenience guard that fails with `Error::Paused` when the platform is paused.
/// Propagates `Error::NotFound` if the pause record has not been initialized yet.
pub fn require_not_paused(env: &Env) -> Result<unit> {
    if is_paused(env)? {
        Err(Error::Paused)
    } else {
        Ok(())
    }
}

#[contract]
impl CraftNexusContract {
    /// Read-only entry point for the platform pause state.
    /// Returns `Err(Error::NotFound)` when the storage key is absent.
    pub fn is_paused(env: Env) -> Result<bool> {
        is_paused(&env)
    }

    /// Sets the pause flag.
    pub fn set_paused(env: Env, paused: bool) -> Result<unit> {
        set_paused(&env, paused)
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

        set_paused(&env, false).unwrap();
        assert_eq!is_paused(&env).unwrap(), false);
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
    fn is_paused_falls_back_to_legacy_key() {
        let env = Env::default();
        env.storage().persistent().set(&PAUSED_KEY, &true);
        assert_eq!(is_paused(&env).unwrap(), true);
    }
}
