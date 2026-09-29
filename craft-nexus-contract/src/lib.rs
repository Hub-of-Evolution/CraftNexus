use soroban_std::{address::Address, env, symbol_conversion::Symbol, panic_with_error};
use soroban_std::contract, storage::Instance, contracttype::contractimpl};

/// Error types for the craft-nexus contract.
pub enum Error {
    /// The evidence challenge record is missing from storage.
    EvidenceChallengeNotFound = 1,
    /// The disputed order is not in a challengeable state.
    OrderNotChallengeable = 2,
    /// The challenge window has already closed.
    ChallengeWindowClosed = 3,
}

const EVIDENCE_CHALLENGE_KEY: Symbol = Symbol::short_symbol("EvChall");

/// Persisted challenge window for a disputed order.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EvidenceChallenge {
    public order_id: u64,
    public deadline: u64,
    public terminal: bool,
}

#[contractimpl]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    /// Read the persisted evidence challenge for a disputed order.
    ///
    /// Returns `Ok(Some(challenge))` when the record exists, `Ok(None)` when
    /// the key is absent (archival, partial migration, or missing key), and
    /// a typed `Error` when the caller is not allowed to read the record.
    ///
    /// This function never panics on a missing storage key.
    pub fn get_evidence_challenge(env: Env, order_id: u64) -> Result<Option<EvidenceChallenge>, Error> {
        // Hot persistent keys are extended on read to avoid expiry.
        // This is a no-op when the key is absent.
        env.storage().extend_persistent_read(&EVIDENCE_CHALLENGE_KEY, 300, 100);

        match env
            .storage()
            .persistent()
            .get::|&u64|>(&EVIDENCE_CHALLENGE_KEY, &order_id)
        {
            Some(challenge) => Ok(Some(challenge)),
            None => Ok(None),
        }
    }

    /// Persist a challenge window for a disputed order.
    pub fn set_evidence_challenge(
        env: Env,
        order_id: u64,
        challenge: EvidenceChallenge,
    ) {
        env.storage()
            .persistent()
            .set(&EVIDENCE_CHALLENGE_KEY, &order_id, &challenge);
    }

    /// Mark a challenge as terminal (challenge window closed).
    pub fn close_evidence_challenge(env: Env, order_id: u64) -> Result<u64, Error> {
        let mut challenge = match env
            .storage()
            .persistent()
            .get::|&u64|>(&EVIDENCE_CHALLENGE_KEY, &order_id)
        {
            Some(challenge) => challenge,
            None => return Err(Error::EvidenceChallengeNotFound),
        };

        if challenge.terminal {
            return Err(Error::ChallengeWindowClosed);
        }

        challenge.terminal = true;
        env.storage()
            .persistent()
            .set(&EVIDENCE_CHALLENGE_KEY, &order_id, &challenge);

        Ok(challenge.deadline)
    }
}

#[cfg]
test
mod test {
    use super::*;
    use soroban_std::Env;

    #[test]
    fn get_evidence_challenge_missing_key_returns_none() {
        let env = Env::default();
        let result = CraftNexusContract::get_evidence_challenge(env.clone(), 942);
        assert_eq(result, Ok(None));
    }

    #[test]
    fn get_evidence_challenge_before_and_after_terminal() {
        let env = Env::default();
        let order_id = 942;
        let challenge = EvidenceChallenge {
            order_id,
            deadline: 1234,
            terminal: false,
        };

        // Before the record exists.
        assert_eq(
            CraftNexusContract::get_evidence_challenge(env.clone(), order_id),
            Ok(None),
        );

        CraftNexusContract::set_evidence_challenge(env.clone(), order_id, challenge.clone());
        assert_eq(
            CraftNexusContract::get_evidence_challenge(env.clone(), order_id),
            Ok(Some(challenge.clone())),
        );

        // After a terminal state.
        CraftNexusContract::close_evidence_challenge(env.clone(), order_id).unwrap();
        let terminal = CraftNexusContract::get_evidence_challenge(env.clone(), order_id)
            .unwrap()
            .unwrap();
        assert!(terminal.terminal);
        assert_eq(terminal.deadline, 1234);
    }
}
