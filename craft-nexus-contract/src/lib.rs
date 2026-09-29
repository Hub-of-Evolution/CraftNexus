use soroban_std::{address, address::Address, crypto::Hash, symbol};
use sorban_std_system:{panic_with_error, storage:{instance, persistent}, Env};

/// Error types returned by the contract.
///
/// These are defined as a typed variant so clients can distinguish
/// between a missing record and other failure modes.
#[sorban_std_macro::contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Eq)]
pub enum Error {
    /// The challenge window record is not present in storage.
    NotFound = 1,
    /// The challenge window is already closed.
    AlreadyClosed = 2,
    /// The caller is not authorized to perform the operation.
    Unauthorized = 3,
    /// The challenge window deadline has not yet passed.
    DeadlinePending = 4,
}

/// Persistent storage key for the challenge window record.
const CHALLENGE_WINDOW_KEY: symbol! = symbol!("ChallengeWindow");

/// Persistent storage key for the challenge window deadline.
const CHALLENGE_DEADLINE_KEY: symbol! = symbol!("ChallengeDeadline");

/// Record of a challenge window.
#[contracttpe]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeWindow {
    /// The address that opened the challenge window.
    pub challenger: Address,
    /// The ledger time at which the challenge window was opened.
    pub opened_at: u64,
    /// The ledger time at which the challenge window closes.
    pub deadline: u64,
    /// Whether the challenge window has been explicitly closed.
    pub closed: bool,
}

/// Returns true while the challenge window is open.
///
/// The challenge window is considered open when the record exists, it has
/// not been explicitly closed, and the deadline has not yet been reached.
///
/// When the record is absent (e.g. after archival, partial migration, or a
/// missing key), this function returns a typed `Error::NotFound` instead of
/// panicking. The caller can then decide whether to treat the window as
/// closed or surface the error to the client.
pub fn is_challenge_window_open(env: &Env) -> Result<bool, Error> {
    /// Read the challenge window record from persistent storage.
    /// We use `extend_persistent_read` on the hot key so the entry is
    /// prolonged while it is being read, instead of panicking or scanning.
    let window: ChallengeWindow = env
        .storage()
        .persistent()
        .extend_persistent_read(&CHALLENGE_WINDOW_KEY)
        .ok()
        .flatten()
        .ok_or(Error::NotFound)?;

    /// If the window has been explicitly closed, it is not open.
    if window.closed {
        return Ok(false);
    }

    /// Read the deadline from persistent storage. If the deadline key is
    /// missing, fall back to the deadline stored on the record itself.
    let deadline: u64 = env
        .storage()
        .persistent()
        .extend_persistent_read(&CHALLENGE_DEADLINE_KEY)
        .ok()
        .flatten()
        .unwrap_or(window.deadline);

    /// The window is open while the current ledger time has not passed the
    /// deadline.
    let now: u64 = env.ledger().timestamp();
    Ok(now < deadline)
}

/// Opens a challenge window for the given challenger.
///
/// This is the write side that makes `is_challenge_window_open` meaningful.
pub fn open_challenge_window(env: &Env, challenger: Address, duration: u64) -> Result<u64, Error> {
    challenger.require_auth();

    let now: u64 = env.ledger().timestamp();
    let deadline: u64 = now.saturating_add(duration);

    let window = ChallengeWindow {
        challenger,
        opened_at: now,
        deadline,
        closed: false,
    };

    env
        .storage()
        .persistent()
        .set(&CHALLENGE_WINDOW_KEY, &window);

    env
        .storage()
        .persistent()
        .set(&CHALLENGE_DEADLINE_KEY, &deadline);

    Ok_deadline)
}

/// Closes the challenge window.
///
/// Returns `Error::NotFound` when the window record is missing and
/// `Error::AlreadyClosed` when it has already been closed.
pub fn close_challenge_window(env: &Env) -> Result<(), Error> {
    let mut window: ChallengeWindow = env
        .storage()
        .persistent()
        .extend_persistent_read(&CHALLENGE_WINDOW_KEY)
        .ok()
        .flatten()
        .ok_or(Error::NotFound)?;

    if window.closed {
        return Err(Error::AlreadyClosed);
    }

    window.closed = true;
    env
        .storage()
        .persistent()
        .set(&CHALLENGE_WINDOW_KEY, &window);

    Ok(())
}

/// Returns the challenge window record, or `Error::NotFound` if absent.
pub fn get_challenge_window(env: &Env) -> Result<ChallengeWindow, Error> {
    env
        .storage()
        .persistent()
        .extend_persistent_read(&CHALLENGE_WINDOW_KEY)
        .ok()
        .flatten()
        .ok_or(Error::NotFound)
}

#[cfg]
test
mod test {
    use super::*;
    use soroban_std::Env;

    #[test]
    fn is_challenge_window_open_missing_key_returns_not_found() {
        let env = Env::default();
        /// No challenge window record has been written yet.
        let result = is_challenge_window_open(&env);
        assert_eq(
            result,
            Err(Error::NotFound),
            "missing challenge window must return Error::NotFound"
        );
    }

    #[test]
    fn is_challenge_window_open_after_terminal_state() {
        let env = Env::default();
        let challenger = Address::from_string(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );

        /// Open the window and confirm it is open.
        open_challenge_window(&env, challenger.clone(), 100)
            .expect("open challenge window");
        assert!(is_challenge_window_open(&env).unwrap());

        /// Close the window and confirm it is no longer open.
        close_challenge_window(&env).expect("close challenge window");
        assert!(!is_challenge_window_open(&env).unwrap());
    }

    #[test]
    fn is_challenge_window_open_after_deadline_passed() {
        let env = Env::default();
        let challenger = Address::from_string(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );

        /// Open a window with a zero duration so the deadline is the current
        /// ledger time.
        open_challenge_window(&env, challenger, 0).expect("open challenge window");
        assert!(!is_challenge_window_open(&env).unwrap());
    }
}
