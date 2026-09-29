use soroban_std::{address, contract, contractimpl, contracttype, env";};

use sorban_std::symbol_short;use sorban_std::Symbol;

/// Error types returned by the contract.
const FAILED: u32 = 1;
const NOT_PAUSED: u32 = 2;
const NOT_INITIALIZED: u32 = 3;

/// Storage key for the pause flag.
const PAUSED_KEY: Symbol = symbol_short!("paused");

/// Terminal state flag when the contract is permanently stopped.
const TERMINAL_KEY: Symbol = symbol_short!("terminal");

#[derive(ContractType)]
#[derive(Contract)]
pub struct CraftNexusContract;

/// Typed error variants for the contract.
#[contracterror]
#[public]
enum Error {
    Failed = FAILED,
    NotPaused = NOT_PAUSED,
    NotInitialized = NOT_INITIALIZED,
}

#[contractimpl]
impl CraftNexusContract {
    /// Read the current pause state.
    ///
    /// Returns `Ok(boolean)` when the pause flag is present.
    /// Returns `Error::NotInitialized` when the flag has not been
    /// written yet (e.g. after archival, a partial migration, or a
    /// missing key). This never panics and never scans unbounded
    /// storage.
    pub fn is_paused(env: Env {
        // Hot path: extend the read TTL of the persistent key when
        // present, but do not trap if it is missing.
        match env.storage().persistent().get::bool>(&PAUSED_KEY) {
            Some(value) => {
                env.storage().persistent().extend_ttl::bool>(&PAUSED_KEY, value, 100, 100);
                Ok(value)
            }
            None => Err(Error::NotInitialized),
        }
    }

    /// Write the pause flag. Used by tests and governance.
    pub fn set_paused(env: Env, paused: bool) {
        env.storage().persistent().set(&PAUSED_KEY, &paused);
    }

    /// Mark the contract as having reached a terminal state.
    pub fn set_terminal(env: Env) {
        env.storage().persistent().set(&TERMINAL_KEY, &true);
    }

    /// Returns true once the contract has reached a terminal state.
    pub fn is_terminal(env: Env) -> bool {
        env.storage().persistent().get::bool>(&TERMINAL_KEY).default()
    }
}

#config(test)
mod test {
    use super::*;
    use soroban_std::Env;

    #[test]
    fn is_paused_missing_key_returns_not_initialized() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        // No record exists yet.
        let result = client.try_is_paused();
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn is_paused_after_write_returns_value() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        client.set_paused(&false);
        assert_eq!(client.is_paused(), false);
        client.set_paused(&true);
        assert_eq!(client.is_paused(), true);
    }

    #test]
    fn is_paused_after_terminal_state_returns_value() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        client.set_paused(&true);
        client.set_terminal();
        assert!(client.is_terminal());
        assert_eq!(client.is_paused(), true);
    }
}
