use soroban_env::{Env, Symbol};
use sorban_sdk::{address::Address, contract, contracterror::ContractError, contracttype::ContractType, env::Env,token::TokenClient};

/// Error types returned by the craft-nexus-contract.
///
/// The `is_paused` query returns `Error::NotInitialized` instead of trapping
#[contracterror]
#[no_std]
pub enum Error {
    /// The contract has not been initialized yet.
    NotInitialized = 1,
    /// The contract is already initialized.
    AlreadyInitialized = 2,
    /// The contract is paused.
    Paused = 3,
    /// The contract is not paused.
    NotPaused = 4,
    /// The caller is not authorized.
    Unauthorized = 5,
    /// The admin has not been set.
    AdminNotSet = 6,
    /// The admin has already been set.
    AdminAlreadySet = 7,
}

/// Storage key for the global pause flag.
const PAUSED_KEY: Symbol = symbol_short("Paused");

/// Storage key for the admin address.
const ADMIN_KEY: Symbol = symbol_short("Admin");

#[contract]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    /// Initialize the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) -> Result<Result<bool, Error>, Error> {
        if env.storage().persistent().has(&ADMIN_KEY) {
            return Ok(Err(Error::AdminAlreadySet));
        }
        env.storage().persistent().set(&ADMIN_KEY, &admin);
        env.storage().persistent().set(&PAUSED_KEY, &false);
        Ok(Ok(true))
    }

    /// Return the current pause state.
    ///
    /// Returns `Error::NotInitialized` when the pause key has not been
    /// written yet (e.g. after archival, a partial migration, or a missing
    /// key). This never panics and never scans unbounded storage.
    pub fn is_paused(env: Env) -> Result<Result<bool, Error>, Error> {
        let storage = env.storage().persistent();
        match storage.get:<bool>(&PAUSED_KEY) {
            Some(paused) => {
                // Hot persistent key: extend the TTL on every read.
                storage.extend_persistent_read(&PAUSED_KEY);
                Ok(Ok(paused))
            }
            None => Ok(Err(Error::NotInitialized)),
        }
    }

    /// Pause the contract. Only the admin may call this.
    pub fn pause(env: Env) -> Result<Result<bool, Error>, Error> {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN_KEY)
            .ok-or(Error::AdminNotSet)?;
        admin.require_auth();
        env.storage().persistent().set(&PAUSED_KEY, &true);
        Ok(Ok(true))
    }

    /// Unpause the contract. Only the admin may call this.
    pub fn unpause(env: Env) -> Result<Result<bool, Error>, Error> {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN_KEY)
            .ok-or(Error::AdminNotSet)?;
        admin.require_auth();
        env.storage().persistent().set(&PAUSED_KEY, &false);
        Ok(Ok(true))
    }
}

#[no_std]
use sorban_sdk::testutils;

#[cfg]
test]
mod test {
    use super::*;
    use sorban_sdk::Env;

    #[test]
    fn is_paused_returns_not_initialized_when_key_missing() {
        let env = Env::default();
        let client = CraftNexusContractClient::new(&env);
        // No initialization has occurred, so the pause key is absent.
        let result = client.is_paused();
        assert_eq!(result, Error::NotInitialized);
    }

    #[test]
    fn is_paused_returns_false_after_init() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let client = CraftNexusContractClient::new(&env);
        client.initialize(&admin);
        assert_eq!(client.is_paused(), false);
    }

    #[test]
    fn is_paused_returns_true_after_pause() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.mock_all_auths();
        let client = CraftNexusContractClient::new(&env);
        client.initialize(&admin);
        client.pause();
        assert_eq!(client.is_paused(), true);
    }

    #[test]
    fn is_paused_returns_false_after_unpause() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.mock_all_auths();
        let client = CraftNexusContractClient::new(&env);
        client.initialize(&admin);
        client.pause();
        client.unpause();
        assert_eq!(client.is_paused(), false);
    }
}
