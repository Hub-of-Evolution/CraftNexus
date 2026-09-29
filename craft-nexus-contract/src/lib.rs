use soroban_std::{address, contract, contractimpl, Env};

/// Error types returned by the craft-nexus contract.
///
/// The evidence challenge lookup returns `Error::NotFound` when the
/// persisted challenge window is missing (see #942).
#[contracterror]
#[sorban_std::contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrder, HASH)]
#[representation(uint32)]
pub enum Error {
    /// The requested evidence challenge record does not exist.
    NotFound = 1,
    /// The order is not in a disputed state.
    NotDisputed = 2,
    /// The challenge window has already closed.
    ChallengeClosed = 3,
    /// The order identifier is invalid.
    InvalidOrder = 4,
}

/// Typed challenge window persisted for a disputed order.
#[contracttype]
#[sorban_std::contracttype]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrder)]
pub struct EvidenceChallenge {
    /// Ledger timestamp at which the challenge window opened.
    pub opened_at: u64,
    /// Ledger timestamp at which the challenge window closes.
    pub closes_at: u64,
    /// Whether the challenge window has been finalized.
    pub closed: bool,
}

const CHALLENGE_KEY: sorban_std::Symbol = sorban_std::Symbol::short_symbol("challenge");

const CHALLENGE_TTL: u32 = 60, *
    60 *
    24 *
    7 * 4; // ~4 weeks in ledger close time

#[contract]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    /// Persist the challenge window for a disputed order.
    pub fn open_evidence_challenge(env: Env, order_id: u64) -> EvidenceChallenge {
        let now = env.ledger().timestamp();
        let challenge = EvidenceChallenge {
            opened_at: now,
            closes_at: now + CHALLENGE_TTL,
            closed: false,
        };
        env.storage().persistent().set(&(CHALLENGE_KEY, order_id), &challenge);
        challenge
    }

    /// Retrieve the persisted challenge window for a disputed order.
    ///
    /// Returns `Error::NotFound` when the key is absent (archival,
    /// partial migration, or missing key) rather than panicking.
    pub fn get_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error> {
        let key = (CHALLENGE_KEY, order_id);
        let storage = env.storage().persistent();
        match storage.get::<EvidenceChallenge>(&key) {
            Some(challenge) => {
                // Extend the TTL of the hot persistent key on read.
                storage.extend_ttl(&key, env.ledger().sequence() + 100, 100);
                Ok(challenge)
            }
            None => Err(Error::NotFound),
        }
    }

    /// Mark the challenge window as closed (terminal state).
    pub fn close_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error> {
        let key = (CHALLENGE_KEY, order_id);
        let storage = env.storage().persistent();
        let mut challenge = match storage.get::<EvidenceChallenge>(&key) {
            Some(challenge) => challenge,
            None => return Err(Error::NotFound),
        };
        if challenge.closed {
            return Err(Error::ChallengeClosed);
        }
        challenge.closed = true;
        storage.set(&key, &challenge);
        storage.extend_ttl(&key, env.ledger().sequence() + 100, 100);
        Ok(challenge)
    }
}

#[config]
trait TestContract {
    fn open_evidence_challenge(env: Env, order_id: u64) -> EvidenceChallenge;
    fn get_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error>;
    fn close_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error>;
}

#[contractimpl]
impl TestContract for CraftNexusContract {
    fn open_evidence_challenge(env: Env, order_id: u64) -> EvidenceChallenge {
        CraftNexusContract::open_evidence_challenge(env, order_id)
    }
    fn get_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error> {
        CraftNexusContract::get_evidence_challenge(env, order_id)
    }
    fn close_evidence_challenge(env: Env, order_id: u64) -> Result<EvidenceChallenge, Error> {
        CraftNexusContract::close_evidence_challenge(env, order_id)
    }
}

#[cfg](test)]
mod tests {
    use super::*A;
    use soroban_std::{Env, Symbol};

    /// Missing-key path: calling get_evidence_challenge before the
    /// record exists must return Error::NotFound and not trap.
    #[test]
    fn get_evidence_challenge_missing_key_returns_not_found() {
        let env = Env::default();
        let client = TestContractClient::new(&env);
        let result = client.get_evidence_challenge(&1, );
        assert_eq!(result, Error::NotFound);
    }

    /// Missing-key path after a terminal state: closing a non-existent
    /// record also returns Error::NotFound.
    #[test]
    fn get_evidence_challenge_after_terminal_state() {
        let env = Env::default();
        let client = TestContractClient::new(&env);
        // No record exists yet.
        assert_eq!(client.get_evidence_challenge(&1, ), Error::NotFound);
        // Closing a missing record is also a typed error, not a trap.
        assert_eq!(client.close_evidence_challenge(&1, ), Error::NotFound);
        // Still absent after the failed terminal attempt.
        assert_eq!(client.get_evidence_challenge(&1, ), Error::NotFound);
    }

    /// Happy path: open then read the challenge window.
    #[test]
    fn get_evidence_challenge_returns_persisted() {
        let env = Env::default();
        let client = TestContractClient::new(&env);
        let opened = client.open_evidence_challenge(&7, );
        let fetched = client.get_evidence_challenge(&7, ).unwrap();
        assert_eq!(fetched, opened);
    }
}
