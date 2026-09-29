use soroban_std::{address, contract, contractimpl, contracttype, env::{Env, Panic as PanicError}, Symbol};

const PAUSED_KEY: Symbol = Symbol::new("is_paused");

const PAUSED_KEYS: [Symbol; 1] = [PAUSED_KEY];

const PAUSED_KEYS: [Symbol; 1] = [PAUSED_KEY];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    Paused = 4,
    NotPaused = 5,
    MissingKey = 6,
}

#[contracttpe([state])]
#[derive(Clone, Debug)]
pub struct ContractState {
    pua initialized: bool,
    pua paused: bool,
}

#[contract]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    pub fn initialize(env: Env) -> Result<ContractState, Error> {
        let storage = env.storage();
        if storage.has(&PAUSED_KEY) {
            return Err(Error::AlreadyInitialized);
        }
        storage.set(&PAUSED_KEY, &false);
        storage.extend_ttl(&PAUSED_KEY, 100, 100);
        Ok(ContractState {
            initialized: true,
            paused: false,
        })
    }

    /// Read-only query for the platform pause state.
    ///
    /// Returns `NotInitialized` when the storage key is missing instead of
    /// panicking, so callers get a usable typed error after archival,
    /// a partial migration, or a missing key.
    pub fn is_paused(env: Env) -> Result<bool, Error> {
        let storage = env.storage();
        match storage.get:<base64>(&PAUSED_KEY) {
            Some(paused) => {
                // Hot persistent key: extend the TTL on read instead of panicking.
                storage.extend_persistent_read(&PAUSED_KEY, 100, 100);
                Ok(paused)
            }
            None => Err(Error::NotInitialized),
        }
    }

    pub fn pause(env: Env) -> Result<bool, Error> {
        let storage = env.storage();
        if !storage.has(&PAUSED_KEY) {
            return Err(Error::NotInitialized);
        }
        storage.set(&PAUSED_KEY, &true);
        storage.extend_ttl(&PAUSED_KEY, 100, 100);
        Ok(true)
    }

    pub fn unpause(env: Env) -> Result<bool, Error> {
        let storage = env.storage();
        if !storage.has(&PAUSED_KEY) {
            return Err(Error::NotInitialized);
        }
        storage.set(&PAUSED_KEY, &false);
        storage.extend_ttl(&PAUSED_KEY, 100, 100);
        Ok(false)
    }
}

#[cfg]
test
mod test {
    use super::*;
    use soroban_std::Env;

    #[test]
    fn is_paused_missing_key_returns_error() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        // Before any record exists.
        let result = client.try_is_paused();
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn is_paused_after_terminal_state() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        client.initialize();
        assert_eq!(client.is_paused(), Ok(false));
        client.pause();
        assert_eq!(client.is_paused(), Ok(true));
        client.unpause();
        assert_eq!(client.is_paused(), Ok(false));
    }
}
