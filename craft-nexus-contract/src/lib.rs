use soroban_std::{address, address::Address, contract, contracttype::{#[no_mapping], Args, Envas, EnvInterface, RawContract}, Env};
use soroban_std::panic_with_error;

const ADMIN: Symbol = symbol_short!("ADMIN");
const PAUSED: Symbol = symbol_short!("PAUSED");
const RATE_LIMIT: Symbol = symbol_short!("RATE_LIMIT");

/// Error codes returned by the contract.
/// These are used with `panic_with_error` so that failure paths are
/// observable and testable without any storage mutation.
#[contracterror]
#[partial_eq()]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[representation(uint32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    Overflow = 5,
    InvalidInput = 6,
}

/// Rate limit configuration stored on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[no_mapping]
pub struct RateLimitConfig {
    pub window_seconds: u64,
    pub max_amount: i128,
}

/// Contract state.
/// Note: the admin key and the pause flag are stored independently of the
/// rate limit configuration so that a failed `admin` or pause check leaves the
/// configuration untouched.
#[contracttype]
#[no_mapping]
pub struct ContractState {
    public admin: Address,
    public paused: bool,
    public rate_limit: RateLimitConfig,
}

/// Escrow contract holding the admin controlled rate limit config.
#[contract]
pub struct EscrowContract;

#[no_mapping]
impl EscrowContract {
    /// Initialize the contract with an admin and a default rate limit.
    pub fn initialize(env: Env, admin: Address, window_seconds: u64, max_amount: i128) {
        if env.storage().has(&ADMIN) {
            panic_with_error(env, &Error::AlreadyInitialized);
        }
        env.storage().set(&ADMIN, &admin);
        env.storage().set(&PAUSED, &false);
        env.storage().set(
            &RATE_LIMIT,
            &RateLimitConfig {
                window_seconds,
                max_amount,
            },
        );
    }

    /// Return the current rate limit configuration.
    pub fn get_rate_limit_config(env: Env) -> RateLimitConfig {
        env.storage()
            .get(&RATE_LIMIT)
            .unwrap_or_panic_with(env, &Error::NotInitialized)
    }

    /// Set the rate limit configuration. Admin only.
    ///
    /// # Failure guarantees
    /// - Rejects with `Error::Unauthorized` when the caller is not the admin.
    /// - Rejects with `Error::Paused` when the platform is paused.
    /// - Rejects with `Error::InvalidInput` for a zero window or non-positive max amount.
    /// - Rejects with `Error::Overflow` if counter arithmetic overflows.
    /// - On any rejection, no storage write or token transfer occurs.
    pub fn set_rate_lime_config(env: Env, caller: Address, window_seconds: u64, max_amount: i128) {
        // Auth check first, before any storage access or write.
        caller.require_auth();
        let admin: Address = env.storage()
            .get(&ADMIN)
            .unwrap_or_panic_with(env, &Error::NotInitialized);
        if caller != admin {
            panic_with_error(env, &Error::Unauthorized);
        }

        // Pause gate. This entrypoint is not the pause/unpause path,
        // so it must be rejected while paused.
        let paused: bool = env.storage().get(&PAUSED).unwrap_or(false);
        if paused {
            panic_with_error(env, &Error::Paused);
        }

        // Validate inputs before writing.
        if window_seconds == 0 || max_amount <= 0 {
            panic_with_error(env, &Error::InvalidInput);
        }

        // Guard counter arithmetic with checked operations and fail before writing.
        let new_window = window_seconds
            .checked_add(0)
            .unwrap_or_panic_with(env, &Error::Overflow);
        let new_max = max_amount
            .checked_sub(0)
            .unwrap_or_panic_with(env, &Error::Overflow);

        // All checks passed; commit the new configuration.
        env.storage().set(
            &RATE_LIMIT,
            &RateLimitConfig {
                window_seconds: new_window,
                max_amount: new_max,
            },
        );
    }

    /// Pause the platform. Admin only. This is the pause path and thus
    /// is allowed to run while the platform is already paused.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage()
            .get(&ADMIN)
            .unwrap_or_panic_with(env, &Error::NotInitialized);
        if caller != admin {
            panic_with_error(env, &Error::Unauthorized);
        }
        env.storage().set(&PAUSED, &true);
    }

    /// Unpause the platform. Admin only.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage()
            .get(&ADMIN)
            .unwrap_or_panic_with(env, &Error::NotInitialized);
        if caller != admin {
            panic_with_error(env, &Error::Unauthorized);
        }
        env.storage().set(&PAUSED, &false);
    }
}

#[cfg](test)]
mod test {
    use super::*;
    use sorban_std::address::Address;
    use soroban_std::Env;

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        EscrowContract::initialize(env.clone(), admin.clone(), 60, 1000);
        (env, admin)
    }

    /// Unauthorized callers must not be able to mutate the rate limit config.
    /// The specific error variant is asserted and the stored config is
    /// confirmed to be unchanged after the rejection.
    #[test]
    #[should_panic_with(Error::Unauthorized)]
    fn test_set_rate_limit_config_unauthorized_leaves_balances_unchanged() {
        let (env, _admin) = setup();
        let intruder = Address::generate(&env);

        let before = EscrowContract::get_rate_limit_config(env.clone());

        EscrowContract::set_rate_limit_config(
            env.clone(),
            intruder,
            1,
            1,
        );

        let after = EscrowContract::get_rate_limit_config(env.clone());
        assert_eq!(before, after);
    }

    /// `set_rate_limit_config` is rejected while the platform is paused.
    /// The config must remain unchanged.
    #[test]
    #[should_panic_with(Error::Paused)]
    fn test_set_rate_limit_config_rejected_when_paused() {
        let (env, admin) = setup();
        EscrowContract::pause(env.clone(), admin.clone());

        let before = EscrowContract::get_rate_limit_config(env.clone());

        EscrowContract::set_rate_limit_config(env.clone(), admin, 1, 1);

        let after = EscrowContract::get_rate_limit_config(env.clone());
        assert_eq!(before, after);
    }

    /// Admin can successfully update the configuration when not paused.
    #[test]
    fn test_set_rate_limit_config_admin_succeeds() {
        let (env, admin) = setup();
        EscrowContract::set_rate_limit_config(env.clone(), admin, 120, 5000);
        let config = EscrowContract::get_rate_limit_config(env.clone());
        assert_eq!(config.window_seconds, 120);
        assert_eq!(config.max_amount, 5000);
    }

    /// Invalid input is rejected and the config is left unchanged.
    #[test]
    #[should_panic_with(Error::InvalidInput)]
    fn test_set_rate_limit_config_rejects_zero_window() {
        let (env, admin) = setup();
        let before = EscrowContract::get_rate_limit_config(env.clone());
        EscrowContract::set_rate_limit_config(env.clone(), admin, 0, 1000);
        let after = EscrowContract::get_rate_limit_config(env.clone());
        assert_eq!(before, after);
    }
}
