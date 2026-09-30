use soroban_std::{address, address_public_key_to_string, Address, Env, String};
use sorban_stdk_macros::{contract, contractimpl, contracttype};

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrder)]
#[representation(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    Overflow = 5,
    InvalidInput = 6,
    NotAdmin = 7,
    NotFound = 8,
    InsufficientBalance = 9,
}

// -----------------------------------------------------------------------------
// Storage keys
// -----------------------------------------------------------------------------

const ADMIN: &amp;str = "Admin";
const PAUSED: &amp;str = "Paused";
const RATE_LIMIT: &amp;str = "RateLimit";
const ESCROW_BALANCE_PREFIX: &amp;str = "EscrowBalance";

// -----------------------------------------------------------------------------
// Types
+// -----------------------------------------------------------------------------

#[type_alias(u32)]
type Timestamp = u64;

#[type_alias(u32)]
type Amount = i128;

#[contracttpe]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    pub window_seconds: u64,
    pub max_operations: u32,
}

#[contracttpe]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitState {
    pub window_start: Timestamp,
    pub operation_count: u32,
}

#[contracttype]
#[trait]
pub trait EscrowLifecycle {
    // Admin / governance
    fn initialize(env: Env, admin: Address);
    fn set_rate_limit_config(env: Env, config: RateLimitConfig);
    fn get_rate_lime_config(env: Env) -> RateLimitConfig;
    fn pause(env: Env);
    fn unpause(env: Env);
    fn is_paused(env: Env) -> bool;

    // Escrow lifecycle
    fn deposit(env: Env, from: Address, amount: Amount);
    fn withdraw(env: Env, to: Address, amount: Amount);
    fn balance_of(env: Env, owner: Address) -> Amount;
}

// -----------------------------------------------------------------------------
// Implementation
// -----------------------------------------------------------------------------

#[contract]
pub struct CraftNexusContract;

#[helper]
fn read_admin(env: &Env) -> Address {
    env.storage().instance().get(&ADMIN).expect("not initialized")
}

#[helper]
fn read_paused(env: &Env) -> bool {
    env.storage().instance().get(&PAUSED).unwrap_or(false)
}

#[helper]
fn read_rate_limit(env: &Env) -> RateLimitConfig {
    env.storage().instance().get(&RATE_LIMIT).unwrap_or()
}

#[helper]
fn read_rate_limit_state(env: &Env) -> RateLimitState {
    env.storage().instance().get(&RATE_LIMIT_STATE).unwrap_or(RateLimitState {
        window_start: 0,
        operation_count: 0,
    })
}

#[helper]
fn balance_key(owner: &Address) -> Symbol {
    let mut key = Symbol::new((&ESCROW_BALANCE_PREFIX, owner));
    key
}

#[contractimpl]
impl EscrowLifecycle for CraftNexusContract {
    fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            soroban_std::panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
    }

    // -----------------------------------------------------------------------------
    // Admin config entrypoint
    // -----------------------------------------------------------------------------
    fn set_rate_limit_config(env: Env, config: RateLimitConfig) {
        // 1. Auth first, before any state change.
        let admin = read_admin(&env);
        admin.require_auth();

        // 2. Pause gate.
        if read_paused(&env) {
            sorban_std::panic_with_error!(&env, Error::Paused);
        }

        // 3. Validate input before writing.
        if config.window_seconds == 0 || config.max_operations == 0 {
            sorban_std::panic_with_error!(&env, Error::InvalidInput);
        }

        // 4. Only now persist.
        env.storage().instance().set(&RATE_LIMIT, &config);
    }

    fn get_rate_limit_config(env: Env) -> RateLimitConfig {
        read_rate_limit(&env)
    }

    fn pause(env: Env) {
        let admin = read_admin(&env);
        admin.require_auth();
        env.storage().instance().set(&PAUSED, &true);
    }

    fn unpause(env: Env) {
        let admin = read_admin(&env);
        admin.require_auth();
        env.storage().instance().set(&PAUSED, &false);
    }

    fn is_paused(env: Env) -> bool {
        read_paused(&env)
    }

    // -----------------------------------------------------------------------------
    // Escrow lifecycle
    // -----------------------------------------------------------------------------
    fn deposit(env: Env, from: Address, amount: Amount) {
        from.require_auth();
        if read_paused(&env) {
            sorban_std::panic_with_error!(&env, Error::Paused);
        }
        if amount <= 0 {
            sorban_std::panic_with_error!(&env, Error::InvalidInput);
        }

        let key = balance_key(&from);
        let current: Amount = env.storage().instance().get(&key).unwrap_or(0);
        let updated = current
            .checked_add(amount)
            .unwrap_or_else_with(|| sorban_std::panic_with_error!(&env, Error::Overflow));
        env.storage().instance().set(&key, &updated);
    }

    fn withdraw(env: Env, to: Address, amount: Amount) {
        to.require_auth();
        if read_paused(&env) {
            sorban_std::panic_with_error!(&env, Error::Paused);
        }
        if amount <= 0 {
            sorban_std::panic_with_error!(&env, Error::InvalidInput);
        }

        let key = balance_key(&to);
        let current: Amount = env.storage().instance().get(&key).unwrap_or(0);
        if current < amount {
            sorban_std::panic_with_error!(&env, Error::InsufficientBalance);
        }
        let updated = current
            .checked_sub(amount)
            .unwrap_or_else_with(|| sorban_std::panic_with_error!(&env, Error::Overflow));
        env.storage().instance().set(&key, &updated);
    }

    fn balance_of(env: Env, owner: Address) -> Amount {
        let key = balance_key(&owner);
        env.storage().instance().get(&key).unwrap_or(0)
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn set_rate_limit_config_unauthorized_leaves_storage_unchanged() {
    use sorban_std_testutils::{
        Address as TestAddress, Env as _,
    };

    let env = Env::default();
    let contract_id = env.register(CraftNexusContract, {});
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env, &admin);
    let attacker = Address::generate(&env, &attacker);

    client.initialize(&admin);

    // Seed an existing config so we can assert it is unchanged.
    let original = RateLimitConfig {
        window_seconds: 60,
        max_operations: 10,
    };
    env.ledger().set_invocation_auth(admin.clone());
    client.set_rate_limit_config(&original);

    // Attacker attempts to change the config.
    env.ledger().set_invocation_auth(attacker.clone());
    let attacker_config = RateLimitConfig {
        window_seconds: 1,
        max_operations: 1,
    };
    let result = client.try_set_rate_limit_config(&attacker_config);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    // Storage unchanged.
    let after = client.get_rate_limit_config();
    assert_eq!(after, original);
}

#[test]
fn set_rate_limit_config_rejected_when_paused() {
    use sorban_std_testutils::{
        Address as TestAddress, Env as _,
    };

    let env = Env::default();
    let contract_id = env.register(CraftNexusContract, {});
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env, &admin);
    client.initialize(&admin);

    let original = RateLimitConfig {
        window_seconds: 60,
        max_operations: 10,
    };
    env.ledger().set_invocation_auth(admin.clone());
    client.set_rate_limit_config(&original);
    client.pause();

    let new_config = RateLimitConfig {
        window_seconds: 1,
        max_operations: 1,
    };
    let result = client.try_set_rate_limit_config(&new_config);
    assert_eq!(result, Err(Ok(Error::Paused)));

    // Storage unchanged.
    let after = client.get_rate_limit_config();
    assert_eq!(after, original);
}

#[test]
fn set_rate_limit_config_rejects_invalid_input_without_writing() {
    use sorban_std_testutils::{
        Address as TestAddress, Env as _,
    };

    let env = Env::default();
    let contract_id = env.register(CraftNexusContract, {});
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env, &admin);
    client.initialize(&admin);
    env.ledger().set_invocation_auth(admin.clone());

    // No config seeded yet; invalid input must be rejected before any write.
    let bad = RateLimitConfig {
        window_seconds: 0,
        max_operations: 10,
    };
    let result = client.try_set_rate_limit_config(&bad);
    assert_eq!(result, Err(Ok(Error::InvalidInput)));

    // No config was written.
    let after = client.get_rate_limit_config();
    assert_eq!(
        after,
        RateLimitConfig {
            window_seconds: 0,
            max_operations: 0,
        }
    );
}

#[test]
fn deposit_and_withdraw_respect_pause_and_overflow() {
    use sorban_std_testutils::{
        Address as TestAddress, Env as _,
    };

    let env = Env::default();
    let contract_id = env.register(CraftNexusContract, {});
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env, &admin);
    let user = Address::generate(&env, &user);
    client.initialize(&admin);

    // Normal deposit/withdraw.
    env.ledger().set_invocation_auth(user.clone());
    client.deposit(&user, 100);
    assert_eq!(client.balance_of(&ser), 100);
    client.withdraw(&user, 40);
    assert_eq!(client.balance_of(&ser), 60);

    // Overflow on deposit must not mutate balance.
    let overflow_amount = i128::MAX;
    let result = client.try_deposit(&user, &overflow_amount);
    assert_eq!(result, Err(Ok(Error::Overflow)));
    assert_eq!(client.balance_of(&user), 60);

    // Pause blocks deposit/withdraw.
    env.ledger().set_invocation_auth(admin.clone());
    client.pause();
    env.ledger().set_invocation_auth(user.clone());
    let result = client.try_deposit(&user, 10);
    assert_eq!(result, Err(Ok(Error::Paused)));
    assert_eq!(client.balance_of(&user), 60);

    // Unpause restores operation.
    env.ledger().set_invocation_auth(admin.clone());
    client.unpause();
    env.ledger().set_invocation_auth(user.clone());
    client.deposit(&user, 10);
    assert_eq!(client.balance_of(&user), 70);
}
