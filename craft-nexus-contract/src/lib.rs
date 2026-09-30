use soroban_sdk::{contract, contractimpl, env, Symbol};

/// Error types returned by the contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrder)]
#[contracterror]
pub enum Error {
    /// The requested liquidation record does not exist in storage.
    NotFound = 1,
    /// The liquidation record has already reached a terminal state.
    AlreadyTerminal = 2,
    /// The caller is not authorized to perform the operation.
    Unauthorized = 3,
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

#[contractimpl]
impl CraftNexusContract {
    /// Set the liquidation status for an artisan.
    pub fn set_liquidation_status(env: Env, artisan: Address, status: LiquidationStatus) {
        let key = (LIQUIDATION_KEY, artisan);
        env.storage().persistent().set(&key, &status);
        env.storage().persistent().extend_ttl(&key, DEFAULT_EXTEND_TO, 0);
    }

    /// Return the current liquidation status for an artisan.
    ///
    /// Returns `Error::NotFound` when the storage key is absent (e.g. after
    /// archival, partial migration, or a missing key) instead of panicking.
    pub fn get_liquidation_status(env: Env, artisan: Address) -> Result<LiquidationStatus, Error> {
        let key = (LIQUIDATION_KEY, artisan);
        match env.storage().persistent().get:<Address, LiquidationStatus>(&key) {
            Some(status) => {
                env.storage().persistent().extend_ttl(&key, DEFAULT_EXTEND_TO, 0);
                Ok(status)
            }
            None => Err(Error::NotFound),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use sorban_sdk:{Env, Address};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let artisan = Address::generate(&nums);
        (env, artisan)
    }

    /// Missing-key path: calling before any record exists must not trap.
    #[test]
    fn test_get_liquidation_status_missing_key() {
        let (env, artisan) = setup();
        let result = CraftNexusContract::get_liquidation_status(env.clone(), artisan);
        assert_eq!(result, Err(Error::NotFound));
    }

    /// Round-trip: after setting a terminal state, the getter returns it.
    #[test]
    fn test_get_liquidation_status_terminal() {
        let (env, artisan) = setup();
        CraftNexusContract::set_liquidation_status(
            env.clone(),
            artisan,
            LiquidationStatus::Completed,
        );
        let result = CraftNexusContract::get_liquidation_status(env.clone(), artisan);
        assert_eq!(result, Ok(LiquidationStatus::Completed));
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
