#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, testutils::Address as _, Address, Env, String, Symbol, IntoVal
};

// ============================================================================
// 1. MOCK TARGET PROTOCOL (CraftNexus)
// This simulates your main protocol to verify the reentrancy guards and invariants.
// ============================================================================

#[contracttype]
pub enum ProtocolDataKey {
    Escrow,
    Stake(Address),
    ReentrancyGuard,
}

#[contract]
pub struct MockCraftNexusContract;

#[contractimpl]
impl MockCraftNexusContract {
    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
        from.require_auth();

        // 1. Check and set Reentrancy Guard
        let is_entered: bool = env.storage().temporary().get(&ProtocolDataKey::ReentrancyGuard).unwrap_or(false);
        if is_entered {
            panic!("ReentrancyGuard: reentrant call detected");
        }
        env.storage().temporary().set(&ProtocolDataKey::ReentrancyGuard, &true);

        // 2. Interact with the external token (This is where the malicious callback happens)
        let args = (from.clone(), env.current_contract_address(), amount);
        env.invoke_contract::<()>(&token, &Symbol::new(&env, "transfer"), args.into_val(&env));

        // 3. Update internal state (Invariants)
        let escrow: i128 = env.storage().instance().get(&ProtocolDataKey::Escrow).unwrap_or(0);
        env.storage().instance().set(&ProtocolDataKey::Escrow, &(escrow + amount));

        let stake: i128 = env.storage().instance().get(&ProtocolDataKey::Stake(from.clone())).unwrap_or(0);
        env.storage().instance().set(&ProtocolDataKey::Stake(from), &(stake + amount));

        // 4. Release Reentrancy Guard
        env.storage().temporary().set(&ProtocolDataKey::ReentrancyGuard, &false);
    }

    pub fn total_escrow(env: Env) -> i128 {
        env.storage().instance().get(&ProtocolDataKey::Escrow).unwrap_or(0)
    }

    pub fn stake_of(env: Env, user: Address) -> i128 {
        env.storage().instance().get(&ProtocolDataKey::Stake(user)).unwrap_or(0)
    }
}

// ============================================================================
// 2. MALICIOUS TOKEN FIXTURE
// Models a token that attempts recursive calls during transfers.
// ============================================================================

#[contracttype]
pub enum TokenDataKey {
    Balance(Address),
    TargetContract,
    AttemptReentrancy,
}

#[contract]
pub struct MaliciousToken;

#[contractimpl]
impl MaliciousToken {
    pub fn initialize(env: Env, target: Address, attempt_reentrancy: bool) {
        env.storage().instance().set(&TokenDataKey::TargetContract, &target);
        env.storage().instance().set(&TokenDataKey::AttemptReentrancy, &attempt_reentrancy);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let bal: i128 = env.storage().instance().get(&TokenDataKey::Balance(to.clone())).unwrap_or(0);
        env.storage().instance().set(&TokenDataKey::Balance(to), &(bal + amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        
        let attempt: bool = env.storage().instance().get(&TokenDataKey::AttemptReentrancy).unwrap_or(false);
        
        if attempt {
            if let Some(target) = env.storage().instance().get::<_, Address>(&TokenDataKey::TargetContract) {
                // --- Malicious Logic: Attempt to re-enter the target protocol ---
                let args = (from.clone(), env.current_contract_address(), amount);
                
                // We use invoke_contract to attempt a recursive entry back into 'deposit'
                let _ = env.invoke_contract::<()>(&target, &Symbol::new(&env, "deposit"), args.into_val(&env));
            }
        }

        // Standard transfer state updates
        let from_bal: i128 = env.storage().instance().get(&TokenDataKey::Balance(from.clone())).unwrap_or(0);
        env.storage().instance().set(&TokenDataKey::Balance(from), &(from_bal - amount));

        let to_bal: i128 = env.storage().instance().get(&TokenDataKey::Balance(to.clone())).unwrap_or(0);
        env.storage().instance().set(&TokenDataKey::Balance(to), &(to_bal + amount));
    }
}

// ============================================================================
// 3. TEST SUITE (Issue #1068 Acceptance Criteria)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _}, Address, Env};

    fn setup() -> (Env, Address, MockCraftNexusContractClient, Address, MaliciousTokenClient) {
        let env = Env::default();
        env.mock_all_auths();

        let user = Address::generate(&env);

        // Register Contracts
        let protocol_id = env.register_contract(None, MockCraftNexusContract);
        let protocol_client = MockCraftNexusContractClient::new(&env, &protocol_id);

        let token_id = env.register_contract(None, MaliciousToken);
        let token_client = MaliciousTokenClient::new(&env, &token_id);

        // Initial funding
        token_client.mint(&user, &10000);

        (env, user, protocol_client, token_id, token_client)
    }

    #[test]
    fn test_rejects_recursive_entry() {
        let (env, user, protocol_client, token_id, token_client) = setup();

        // Arm the malicious token to attempt reentrancy
        token_client.initialize(&protocol_client.address, &true);

        // Expect the transaction to fail due to the reentrancy panic
        let res = protocol_client.try_deposit(&user, &token_id, &1000);
        
        assert!(res.is_err(), "Expected reentrancy to be rejected");
    }

    #[test]
    fn test_guard_cleanup_after_callback_failure() {
        let (env, user, protocol_client, token_id, token_client) = setup();

        // 1. Arm and trigger failure
        token_client.initialize(&protocol_client.address, &true);
        let _ = protocol_client.try_deposit(&user, &token_id, &1000);

        // 2. Disarm the malicious token (simulating normal operation resuming)
        token_client.initialize(&protocol_client.address, &false);

        // 3. Verify guard is cleaned up by attempting a normal deposit
        let res = protocol_client.try_deposit(&user, &token_id, &1000);
        
        assert!(res.is_ok(), "Guard failed to clean up after reverted execution");
    }

    #[test]
    fn test_invariants_maintained_on_failed_callback() {
        let (env, user, protocol_client, token_id, token_client) = setup();

        // Capture initial invariants
        let initial_escrow = protocol_client.total_escrow();
        let initial_stake = protocol_client.stake_of(&user);

        // Execute malicious callback
        token_client.initialize(&protocol_client.address, &true);
        let _ = protocol_client.try_deposit(&user, &token_id, &1000);

        // Assert invariants remain untouched after the hostile transaction reverts
        assert_eq!(
            protocol_client.total_escrow(),
            initial_escrow,
            "Escrow invariant violated: State modified during failed callback"
        );
        assert_eq!(
            protocol_client.stake_of(&user),
            initial_stake,
            "Stake invariant violated: State modified during failed callback"
        );
    }
}