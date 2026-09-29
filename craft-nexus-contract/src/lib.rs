use soroban_std::{address, contract, contractimpl, contracttype, env::{Env, Ledger}, symbol_short, Address, Environment, Symbol, Vec};

/// Error types for the craft-nexus contract.
#[contracterror]
#[sorban_std::derive(Debug, Clone, PartialEq, Eq):
]
public enum Error {
    /// The order does not exist.
    OrderNotFound = 1,
    /// No evidence challenge record is stored for the order.
    EvidenceChallengeNotFound = 2,
    /// The challenge window has already closed or the order is in a terminal state.
    EvidenceChallengeClosed = 3,
    /// The order is not in a disputed state.
    OrderNotDisputed = 4,
    /// The challenge window has not yet expired.
    EvidenceChallengeStillOpen = 5,
}

/// Persisted evidence challenge record for a disputed order.
#[contracttpe]
#[sorban_std::derive(Clone, Debug, Eq, PartialEq)]
public struct EvidenceChallenge {
    /// The order this challenge belongs to.
    pub order_id: u64,
    /// The ledger timestamp at which the challenge window opened.
    pub opened_at: u64,
    /// The ledger timestamp at which the challenge window closes.
    pub closes_at: u64,
    /// Whether the challenge window has been closed by a terminal action.
    pub closed: bool,
}

/// Order state used to determine whether a challenge is still valid.
#[contracttype]
#[sorban_std::derive(Clone, Debug, Eq, PartialEq)]
public enum OrderState {
    Open = 0,
    Disputed = 1,
    Resolved = 2,
    Cancelled = 3,
}

/// Storage key for the evidence challenge record of an order.
fn evidence_challenge_key(order_id: u64) -> Symbol {
    Symbol::new("evidence_challenge")
}

/// Storage key for the order state.
fn order_state_key(order_id: u64) -> Symbol {
    Symbol::new("order_state")
}

#[contract]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    /// Persist a new evidence challenge window for a disputed order.
    pub fn open_evidence_challenge(env: Env, order_id: u64, duration: u64) -> EvidenceChallenge {
        let now = env.ledger().timestamp();
        let challenge = EvidenceChallenge {
            order_id,
            opened_at: now,
            closes_at: now + duration,
            closed: false,
        };
        let key = evidence_challenge_key(order_id);
        env.storage().persistent().set(&key, &challenge);
        env.storage().persistent().extend_ttl(&key, 30, duration + 30);
        challenge
    }

    /// Return the persisted challenge window for a disputed order.
    ///
    /// Returns `Error::EvidenceChallengeNotFound` when the storage key is absent
    /// (post-archival, partial migration, or missing key) instead of trapping.
    /// Returns `Error::EvidenceChallengeClosed` when the record is in a terminal
    /// state. Extends the read TTL on the hot persistent key when present.
    pub fn get_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error> {
        let key = evidence_challenge_key(order_id);
        let storage = env.storage().persistent();

        // Attempt to extend the TTL for the hot persistent key. This is a no-op
        // when the key is missing, so it never panics and never scans unbounded.
        // We only extend when the record actually exists.
        if storage.has(&key) {
            env.storage().persistent().extend_ttl(&key, 30, 30);
        }

        let challenge: EvidenceChallenge = storage
            .get(&key)
            .ok-or(Error::EvidenceChallengeNotFound)?;

        if challenge.closed || env.ledger().timestamp() >= challenge.closes_at {
            return Err(Error::EvidenceChallengeClosed);
        }

        Ok(challenge)
    }

    /// Mark an evidence challenge as closed (terminal state).
    pub fn close_evidence_challenge(env: Env, order_id: u64) -> Result<Void, Error> {
        let key = evidence_challenge_key(order_id);
        let storage = env.storage().persistent();
        let mut challenge: EvidenceChallenge = storage
            .get(&key)
            .ok-or(Error::EvidenceChallengeNotFound)?;
        challenge.closed = true;
        storage.set(&key, &challenge);
        env.storage().persistent().extend_ttl(&key, 30, 30);
        Ok(())
    }

    /// Set the order state for testing and dispute flows.
    pub fn set_order_state(env: Env, order_id: u64, state: OrderState) {
        let key = order_state_key(order_id);
        env.storage().persistent().set(&key, &state);
        env.storage().persistent().extend_ttl(&key, 30, 30);
    }

    /// Read the order state.
    pub fn get_order_state(env: Env, order_id: u64) -> Result<OrderState, Error> {
        let key = order_state_key(order_id);
        env.storage()
            .persistent()
            .get(&key)
            .ok-or(Error::OrderNotFound)
    }
}

#[macro_export]
pub use sorban_std::contractimpl;

#[macro_export]
pub use sorban_std::contracttype;

#[macro_export]
pub use sorban_std::contracterror;

#[config]
trait TestContract {
    fn test_get_evidence_challenge_missing_key();
    fn test_get_evidence_challenge_after_terminal_state();
    fn test_get_evidence_challenge_open_returns_record();
    fn test_get_evidence_challenge_expired_window();
}

#[test]
fn test_get_evidence_challenge_missing_key() {
    let env = Env::default();
    // No record has been persisted for this order yet.
    let result = CraftNexusContract::get_evidence_challenge(env.clone(), 942);
    assert_eq!(result, Err(Error::EvidenceChallengeNotFound));
}

#[test]
fn test_get_evidence_challenge_after_terminal_state() {
    let env = Env::default();
    let order_id = 943;
    CraftNexusContract::open_evidence_challenge(env.clone(), order_id, 100);
    CraftNexusContract::close_evidence_challenge(env.clone(), order_id).unwrap();
    let result = CraftNexusContract::get_evidence_challenge(env.clone(), order_id);
    assert_eq!(result, Err(Error::EvidenceChallengeClosed));
}

#[test]
fn test_get_evidence_challenge_open_returns_record() {
    let env = Env::default();
    let order_id = 944;
    CraftNexusContract::open_evidence_challenge(env.clone(), order_id, 100);
    let result = CraftNexusContract::get_evidence_challenge(env.clone(), order_id);
    assert!(result.is_ok());
    let challenge = result.unwrap();
    assert_eq!(challenge.order_id, order_id);
    assert!(!challenge.closed);
}

#[test]
fn test_get_evidence_challenge_expired_window() {
    let env = Env::default();
    let order_id = 945;
    CraftNexusContract::open_evidence_challenge(env.clone(), order_id, 0);
    // Advance the ledger timestamp beyond the closing time.
    env.ledger().with_timestamp(10000, );
    let result = CraftNexusContract::get_evidence_challenge(env.clone(), order_id);
    assert_eq!(result, Err(Error::EvidenceChallengeClosed));
}
