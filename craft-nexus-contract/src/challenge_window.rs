//! Guard logic for `close_challenge_window` (#1317).
//!
//! `close_challenge_window` finalizes the evidence-challenge window of a
//! disputed escrow. It moves no value itself, but closing the window unlocks
//! settlement, so every rejected call must leave storage untouched. The
//! entrypoint therefore evaluates **all** guards below before its first write
//! (the closed-window marker), and each rejection is a typed [`Error`]:
//!
//! | # | Guard                                            | Error                        |
//! |---|--------------------------------------------------|------------------------------|
//! | 1 | platform not paused                              | `ContractPaused`             |
//! | 2 | caller is a dispute party or privileged resolver | `Unauthorized`               |
//! | 3 | escrow is `Disputed`                             | `InvalidEscrowState`         |
//! | 4 | window not already closed / settlement not final | `SettlementAlreadyFinalized` |
//! | 5 | deadline computable without overflow             | `CounterOverflow`            |
//! | 6 | deadline has elapsed (`now >= deadline`)         | `ChallengeWindowActive`      |
//!
//! Authorized callers (guard 2, after `caller.require_auth()`): the escrow's
//! buyer or seller, or the platform admin, arbitrator, or moderator — the same
//! set that can settle the dispute. Any other address is rejected even though
//! it has signed.
//!
//! The functions here are pure (no storage, no auth), so the decision for a
//! given set of inputs is deterministic and each guard is unit-tested directly.

use crate::Error;
use soroban_sdk::Address;

/// The role-relevant parties of a dispute and the platform.
pub(crate) struct ChallengeParties<'a> {
    pub buyer: &'a Address,
    pub seller: &'a Address,
    pub admin: &'a Address,
    pub arbitrator: &'a Address,
    pub moderator: Option<&'a Address>,
}

/// State observed by `close_challenge_window` before it writes anything.
pub(crate) struct CloseWindowState {
    pub paused: bool,
    pub disputed: bool,
    pub already_closed: bool,
    pub settlement_final: bool,
    /// Stored deadline from the challenge record, if one exists.
    pub stored_deadline: Option<u64>,
    /// Legacy fallback inputs, used only when no challenge record exists.
    pub dispute_started_at: u64,
    pub challenge_window: u32,
    pub now: u64,
}

/// Guard 2: may `caller` close the window?
pub(crate) fn is_authorized_closer(caller: &Address, parties: &ChallengeParties) -> bool {
    caller == parties.buyer
        || caller == parties.seller
        || caller == parties.admin
        || caller == parties.arbitrator
        || parties.moderator == Some(caller)
}

/// Guard 5: the challenge deadline, with checked arithmetic.
///
/// A stored deadline is immutable once written at dispute time and is used
/// as-is. The legacy fallback (`started_at + window`) used to be an unchecked
/// `u64` addition; an overflow now fails closed with `CounterOverflow` instead
/// of trapping (overflow checks on) or wrapping to a past deadline (off).
pub(crate) fn challenge_deadline(
    stored_deadline: Option<u64>,
    dispute_started_at: u64,
    challenge_window: u32,
) -> Result<u64, Error> {
    match stored_deadline {
        Some(deadline) => Ok(deadline),
        None => dispute_started_at
            .checked_add(challenge_window as u64)
            .ok_or(Error::CounterOverflow),
    }
}

/// Guards 1–6, in order. `Ok(())` means the window may be closed now.
pub(crate) fn check_close(
    caller: &Address,
    parties: &ChallengeParties,
    state: &CloseWindowState,
) -> Result<(), Error> {
    if state.paused {
        return Err(Error::ContractPaused);
    }
    if !is_authorized_closer(caller, parties) {
        return Err(Error::Unauthorized);
    }
    if !state.disputed {
        return Err(Error::InvalidEscrowState);
    }
    if state.already_closed || state.settlement_final {
        return Err(Error::SettlementAlreadyFinalized);
    }
    let deadline = challenge_deadline(
        state.stored_deadline,
        state.dispute_started_at,
        state.challenge_window,
    )?;
    if state.now < deadline {
        return Err(Error::ChallengeWindowActive);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    struct Fixture {
        buyer: Address,
        seller: Address,
        admin: Address,
        arbitrator: Address,
        moderator: Address,
        stranger: Address,
    }

    impl Fixture {
        fn new(env: &Env) -> Self {
            Fixture {
                buyer: Address::generate(env),
                seller: Address::generate(env),
                admin: Address::generate(env),
                arbitrator: Address::generate(env),
                moderator: Address::generate(env),
                stranger: Address::generate(env),
            }
        }

        fn parties(&self, with_moderator: bool) -> ChallengeParties<'_> {
            ChallengeParties {
                buyer: &self.buyer,
                seller: &self.seller,
                admin: &self.admin,
                arbitrator: &self.arbitrator,
                moderator: if with_moderator {
                    Some(&self.moderator)
                } else {
                    None
                },
            }
        }
    }

    /// A disputed escrow whose stored deadline (1_000) has elapsed at now = 1_000.
    fn closable() -> CloseWindowState {
        CloseWindowState {
            paused: false,
            disputed: true,
            already_closed: false,
            settlement_final: false,
            stored_deadline: Some(1_000),
            dispute_started_at: 0,
            challenge_window: 0,
            now: 1_000,
        }
    }

    #[test]
    fn every_authorized_role_can_close_after_deadline() {
        let env = Env::default();
        let f = Fixture::new(&env);
        for caller in [&f.buyer, &f.seller, &f.admin, &f.arbitrator, &f.moderator] {
            assert_eq!(check_close(caller, &f.parties(true), &closable()), Ok(()));
        }
    }

    #[test]
    fn unauthorized_caller_is_rejected() {
        let env = Env::default();
        let f = Fixture::new(&env);
        assert_eq!(
            check_close(&f.stranger, &f.parties(true), &closable()),
            Err(Error::Unauthorized)
        );
    }

    /// With no moderator configured, an address must not match `None`.
    #[test]
    fn moderator_role_requires_a_configured_moderator() {
        let env = Env::default();
        let f = Fixture::new(&env);
        assert_eq!(
            check_close(&f.moderator, &f.parties(false), &closable()),
            Err(Error::Unauthorized)
        );
    }

    #[test]
    fn paused_platform_rejects_even_privileged_callers() {
        let env = Env::default();
        let f = Fixture::new(&env);
        let state = CloseWindowState {
            paused: true,
            ..closable()
        };
        for caller in [&f.admin, &f.buyer, &f.stranger] {
            assert_eq!(
                check_close(caller, &f.parties(true), &state),
                Err(Error::ContractPaused)
            );
        }
    }

    #[test]
    fn non_disputed_escrow_is_rejected() {
        let env = Env::default();
        let f = Fixture::new(&env);
        let state = CloseWindowState {
            disputed: false,
            ..closable()
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &state),
            Err(Error::InvalidEscrowState)
        );
    }

    #[test]
    fn closing_twice_or_after_settlement_is_rejected() {
        let env = Env::default();
        let f = Fixture::new(&env);
        for state in [
            CloseWindowState {
                already_closed: true,
                ..closable()
            },
            CloseWindowState {
                settlement_final: true,
                ..closable()
            },
        ] {
            assert_eq!(
                check_close(&f.buyer, &f.parties(true), &state),
                Err(Error::SettlementAlreadyFinalized)
            );
        }
    }

    #[test]
    fn deadline_boundary_is_inclusive() {
        let env = Env::default();
        let f = Fixture::new(&env);
        let one_before = CloseWindowState {
            now: 999,
            ..closable()
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &one_before),
            Err(Error::ChallengeWindowActive)
        );
        let at = closable();
        assert_eq!(check_close(&f.buyer, &f.parties(true), &at), Ok(()));
    }

    #[test]
    fn legacy_deadline_uses_checked_addition() {
        assert_eq!(challenge_deadline(None, 100, 50), Ok(150));
        assert_eq!(
            challenge_deadline(None, u64::MAX, 1),
            Err(Error::CounterOverflow)
        );
        assert_eq!(challenge_deadline(None, u64::MAX, 0), Ok(u64::MAX));
        // A stored deadline is authoritative, even with overflowing fallback inputs.
        assert_eq!(challenge_deadline(Some(7), u64::MAX, u32::MAX), Ok(7));
    }

    #[test]
    fn legacy_overflow_is_reported_not_trapped() {
        let env = Env::default();
        let f = Fixture::new(&env);
        let state = CloseWindowState {
            stored_deadline: None,
            dispute_started_at: u64::MAX - 10,
            challenge_window: 11,
            now: u64::MAX,
            ..closable()
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &state),
            Err(Error::CounterOverflow)
        );
    }

    /// Guard order is part of the contract: each earlier guard wins.
    #[test]
    fn guard_order_is_fixed() {
        let env = Env::default();
        let f = Fixture::new(&env);
        let everything_wrong = CloseWindowState {
            paused: true,
            disputed: false,
            already_closed: true,
            settlement_final: true,
            stored_deadline: None,
            dispute_started_at: u64::MAX,
            challenge_window: u32::MAX,
            now: 0,
        };
        assert_eq!(
            check_close(&f.stranger, &f.parties(true), &everything_wrong),
            Err(Error::ContractPaused)
        );
        let s = CloseWindowState {
            paused: false,
            ..everything_wrong
        };
        assert_eq!(
            check_close(&f.stranger, &f.parties(true), &s),
            Err(Error::Unauthorized)
        );
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &s),
            Err(Error::InvalidEscrowState)
        );
        let s = CloseWindowState {
            disputed: true,
            ..s
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &s),
            Err(Error::SettlementAlreadyFinalized)
        );
        let s = CloseWindowState {
            already_closed: false,
            settlement_final: false,
            ..s
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &s),
            Err(Error::CounterOverflow)
        );
        let s = CloseWindowState {
            dispute_started_at: 10,
            challenge_window: 5,
            ..s
        };
        assert_eq!(
            check_close(&f.buyer, &f.parties(true), &s),
            Err(Error::ChallengeWindowActive)
        );
    }
}
