//! Attestation expiry and ledger binding for cross-contract authorization (#1122).
//!
//! Cross-contract authorization evidence — the [`OnboardingAttestation`] value
//! this escrow contract receives from the onboarding contract — travels as a
//! plain value. Evidence with no validity window can be replayed forever, and
//! evidence with no deployment binding can be replayed against a *different*
//! instance of the same contract code. Both are authorization-freshness bugs:
//! the caller proves a state that was once true.
//!
//! This module owns the ledger-side policy for that evidence. Four pieces of
//! context are bound together and validated as a unit:
//!
//! | Field | Binds the evidence to |
//! |---|---|
//! | `issuance_ledger` | the first ledger on which it may be used |
//! | `expiry_ledger` | the first ledger on which it is already stale |
//! | `contract_instance` | the deployment it was minted for |
//! | `operation_nonce` | the single operation it was minted for |
//!
//! # Validity interval
//!
//! Ledger validity is half-open, matching the `[start, start + duration)`
//! convention documented in [`crate::time_policy`]:
//!
//! ```text
//! valid ⟺ issuance_ledger <= current_ledger < expiry_ledger
//! ```
//!
//! The `expiry_ledger` itself is **already expired**. That makes the expiry
//! boundary observable and testable without an off-by-one: an attestation
//! issued at ledger 1_000 with a 100-ledger window is valid on ledgers
//! 1_000..=1_099 and rejected from ledger 1_100 onwards.
//!
//! [`OnboardingAttestation`]: crate::OnboardingAttestation
//!
//! # Pure by construction
//!
//! Nothing here reads storage, the ledger, or the environment: callers pass
//! `current_ledger`, the instance they are bound to, and the expected nonce in,
//! so the same input always produces the same verdict. That is what makes
//! "expired attestations fail deterministically" true, and it is why the module
//! is `no_std` and allocation-free like the rest of the crate.
//!
//! The instance identity is generic so the module can be used with the real
//! [`Address`] that [`OnboardingAttestation::contract_instance`] carries, while
//! unit tests can bind plain byte ids.
//!
//! [`Address`]: soroban_sdk::Address
//! [`OnboardingAttestation::contract_instance`]: crate::OnboardingAttestation::contract_instance

// ── Limits ────────────────────────────────────────────────────────────────────

/// Default validity window for a freshly issued attestation: 17 280 ledgers,
/// i.e. ~24 hours at the Stellar ~5 s close time.
pub const DEFAULT_ATTESTATION_VALIDITY_LEDGERS: u32 = 17_280;

/// Hard ceiling on any attestation validity window: 518 400 ledgers, i.e.
/// ~30 days. A longer window would outlive the state it attests to, so
/// [`Attestation::issue`] refuses to mint one.
pub const MAX_ATTESTATION_VALIDITY_LEDGERS: u32 = 518_400;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Identity of one contract deployment.
///
/// This is the 32-byte contract id of the instance the evidence is bound to,
/// so a value minted for deployment A can never authorize an operation on
/// deployment B — even when both run identical WASM.
pub type ContractInstanceId = [u8; 32];

/// Why an attestation was rejected.
///
/// The variants are ordered by the precedence documented on
/// [`Attestation::validate`], and each maps to an ABI-stable contract error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AttestationError {
    /// The requested validity window is zero ledgers long.
    ZeroValidityWindow = 1,
    /// The requested validity window exceeds [`MAX_ATTESTATION_VALIDITY_LEDGERS`].
    ValidityWindowTooLong = 2,
    /// `issuance_ledger + validity` overflowed `u32`.
    LedgerWindowOverflow = 3,
    /// `current_ledger` is before the issuance ledger — the evidence is
    /// future-dated.
    NotYetIssued = 4,
    /// `current_ledger` is at or past the expiry ledger — the evidence is stale.
    Expired = 5,
    /// The evidence was minted for a different contract deployment.
    ForeignContractInstance = 6,
    /// The evidence was minted for a different operation.
    OperationNonceMismatch = 7,
}

/// Cross-contract authorization evidence with a bounded validity period and
/// explicit ledger-context binding.
///
/// `I` is the contract-instance identity: [`ContractInstanceId`] for byte-level
/// callers, or the `Address` carried by an onboarding attestation.
///
/// Construct with [`Attestation::issue`] (or
/// [`Attestation::issue_default_window`]) so the window limits are enforced at
/// mint time rather than at every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation<I> {
    issuance_ledger: u32,
    expiry_ledger: u32,
    contract_instance: I,
    operation_nonce: u64,
}

impl<I> Attestation<I> {
    /// Mints an attestation for `contract_instance` and `operation_nonce`,
    /// valid on the half-open ledger interval
    /// `[issuance_ledger, issuance_ledger + validity_ledgers)`.
    ///
    /// # Errors
    ///
    /// * [`AttestationError::ZeroValidityWindow`] — `validity_ledgers == 0`.
    /// * [`AttestationError::ValidityWindowTooLong`] — the window exceeds
    ///   [`MAX_ATTESTATION_VALIDITY_LEDGERS`].
    /// * [`AttestationError::LedgerWindowOverflow`] — `issuance_ledger +
    ///   validity_ledgers` does not fit in `u32`.
    pub fn issue(
        issuance_ledger: u32,
        validity_ledgers: u32,
        contract_instance: I,
        operation_nonce: u64,
    ) -> Result<Self, AttestationError> {
        if validity_ledgers == 0 {
            return Err(AttestationError::ZeroValidityWindow);
        }
        if validity_ledgers > MAX_ATTESTATION_VALIDITY_LEDGERS {
            return Err(AttestationError::ValidityWindowTooLong);
        }
        let expiry_ledger = issuance_ledger
            .checked_add(validity_ledgers)
            .ok_or(AttestationError::LedgerWindowOverflow)?;
        Ok(Self {
            issuance_ledger,
            expiry_ledger,
            contract_instance,
            operation_nonce,
        })
    }

    /// Mints an attestation with [`DEFAULT_ATTESTATION_VALIDITY_LEDGERS`].
    pub fn issue_default_window(
        issuance_ledger: u32,
        contract_instance: I,
        operation_nonce: u64,
    ) -> Result<Self, AttestationError> {
        Self::issue(
            issuance_ledger,
            DEFAULT_ATTESTATION_VALIDITY_LEDGERS,
            contract_instance,
            operation_nonce,
        )
    }

    /// First ledger on which the attestation is valid (inclusive).
    #[inline(always)]
    pub fn issuance_ledger(&self) -> u32 {
        self.issuance_ledger
    }

    /// First ledger on which the attestation is no longer valid (exclusive).
    #[inline(always)]
    pub fn expiry_ledger(&self) -> u32 {
        self.expiry_ledger
    }

    /// Deployment this attestation is bound to.
    #[inline(always)]
    pub fn contract_instance(&self) -> &I {
        &self.contract_instance
    }

    /// Operation this attestation is bound to.
    #[inline(always)]
    pub fn operation_nonce(&self) -> u64 {
        self.operation_nonce
    }

    /// Length of the validity window in ledgers.
    #[inline(always)]
    pub fn validity_ledgers(&self) -> u32 {
        self.expiry_ledger - self.issuance_ledger
    }

    /// `true` once `current_ledger` has reached the expiry ledger.
    ///
    /// The expiry ledger itself counts as expired.
    #[inline(always)]
    pub fn is_expired(&self, current_ledger: u32) -> bool {
        current_ledger >= self.expiry_ledger
    }

    /// `true` when `current_ledger` precedes the issuance ledger.
    #[inline(always)]
    pub fn is_not_yet_issued(&self, current_ledger: u32) -> bool {
        current_ledger < self.issuance_ledger
    }

    /// Ledgers left before expiry, saturating at zero once expired.
    #[inline(always)]
    pub fn remaining_ledgers(&self, current_ledger: u32) -> u32 {
        self.expiry_ledger.saturating_sub(current_ledger)
    }
}

impl<I: PartialEq> Attestation<I> {
    /// `true` when the attestation was minted for `current_instance`.
    #[inline(always)]
    pub fn binds_instance(&self, current_instance: &I) -> bool {
        &self.contract_instance == current_instance
    }

    /// `true` when the attestation was minted for `expected_nonce`.
    #[inline(always)]
    pub fn matches_nonce(&self, expected_nonce: u64) -> bool {
        self.operation_nonce == expected_nonce
    }

    /// Validates the attestation against the caller's ledger context.
    ///
    /// Checks run in a fixed precedence so the returned error is deterministic
    /// when several things are wrong at once:
    ///
    /// 1. [`AttestationError::NotYetIssued`]
    /// 2. [`AttestationError::Expired`]
    /// 3. [`AttestationError::ForeignContractInstance`]
    /// 4. [`AttestationError::OperationNonceMismatch`]
    ///
    /// Freshness is decided before identity on purpose: an expired attestation
    /// is rejected as expired no matter which instance or operation presents
    /// it, which keeps the failure mode stable for audit tooling and stops the
    /// error code from leaking which of the other fields happened to match.
    pub fn validate(
        &self,
        current_ledger: u32,
        current_instance: &I,
        expected_nonce: u64,
    ) -> Result<(), AttestationError> {
        if self.is_not_yet_issued(current_ledger) {
            return Err(AttestationError::NotYetIssued);
        }
        if self.is_expired(current_ledger) {
            return Err(AttestationError::Expired);
        }
        if !self.binds_instance(current_instance) {
            return Err(AttestationError::ForeignContractInstance);
        }
        if !self.matches_nonce(expected_nonce) {
            return Err(AttestationError::OperationNonceMismatch);
        }
        Ok(())
    }

    /// Convenience wrapper over [`Attestation::validate`] for call sites that
    /// only need a boolean verdict.
    #[inline(always)]
    pub fn is_valid(&self, current_ledger: u32, current_instance: &I, expected_nonce: u64) -> bool {
        self.validate(current_ledger, current_instance, expected_nonce)
            .is_ok()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE: ContractInstanceId = [7u8; 32];
    const OTHER_INSTANCE: ContractInstanceId = [8u8; 32];

    fn sample() -> Attestation<ContractInstanceId> {
        Attestation::issue(1_000, 100, INSTANCE, 42).unwrap()
    }

    #[test]
    fn issue_binds_all_four_contexts() {
        let a = sample();
        assert_eq!(a.issuance_ledger(), 1_000);
        assert_eq!(a.expiry_ledger(), 1_100);
        assert_eq!(a.validity_ledgers(), 100);
        assert_eq!(a.contract_instance(), &INSTANCE);
        assert_eq!(a.operation_nonce(), 42);
    }

    #[test]
    fn issue_default_window_uses_24h_ledger_budget() {
        let a = Attestation::issue_default_window(500, INSTANCE, 1).unwrap();
        assert_eq!(a.issuance_ledger(), 500);
        assert_eq!(a.expiry_ledger(), 500 + DEFAULT_ATTESTATION_VALIDITY_LEDGERS);
    }

    #[test]
    fn issue_rejects_zero_and_oversized_windows() {
        assert_eq!(
            Attestation::issue(10, 0, INSTANCE, 1),
            Err(AttestationError::ZeroValidityWindow)
        );
        assert_eq!(
            Attestation::issue(10, MAX_ATTESTATION_VALIDITY_LEDGERS + 1, INSTANCE, 1),
            Err(AttestationError::ValidityWindowTooLong)
        );
        // The ceiling itself is accepted.
        assert!(Attestation::issue(10, MAX_ATTESTATION_VALIDITY_LEDGERS, INSTANCE, 1).is_ok());
    }

    #[test]
    fn issue_rejects_ledger_window_overflow() {
        assert_eq!(
            Attestation::issue(u32::MAX, 1, INSTANCE, 1),
            Err(AttestationError::LedgerWindowOverflow)
        );
        assert_eq!(
            Attestation::issue(u32::MAX - 10, 11, INSTANCE, 1),
            Err(AttestationError::LedgerWindowOverflow)
        );
        // Exactly reaching u32::MAX is representable.
        let a = Attestation::issue(u32::MAX - 10, 10, INSTANCE, 1).unwrap();
        assert_eq!(a.expiry_ledger(), u32::MAX);
    }

    #[test]
    fn issuance_boundary_is_inclusive() {
        let a = sample();
        // One ledger before issuance: future-dated evidence.
        assert!(a.is_not_yet_issued(999));
        assert_eq!(
            a.validate(999, &INSTANCE, 42),
            Err(AttestationError::NotYetIssued)
        );
        // Exactly on the issuance ledger: valid.
        assert!(a.is_valid(1_000, &INSTANCE, 42));
        assert_eq!(a.validate(1_000, &INSTANCE, 42), Ok(()));
    }

    #[test]
    fn expiry_boundary_is_exclusive() {
        let a = sample();
        // Last valid ledger.
        assert!(!a.is_expired(1_099));
        assert!(a.is_valid(1_099, &INSTANCE, 42));
        // Exactly on the expiry ledger: already expired.
        assert!(a.is_expired(1_100));
        assert_eq!(
            a.validate(1_100, &INSTANCE, 42),
            Err(AttestationError::Expired)
        );
        // Well past expiry stays expired.
        assert_eq!(
            a.validate(9_999, &INSTANCE, 42),
            Err(AttestationError::Expired)
        );
    }

    #[test]
    fn boundary_helpers_agree_with_validate() {
        let a = sample();
        for ledger in [999u32, 1_000, 1_050, 1_099, 1_100, 1_101] {
            let expected = !a.is_not_yet_issued(ledger) && !a.is_expired(ledger);
            assert_eq!(
                a.is_valid(ledger, &INSTANCE, 42),
                expected,
                "ledger {ledger} disagreed with the boundary helpers"
            );
        }
    }

    #[test]
    fn attestation_cannot_be_moved_to_another_contract_instance() {
        let a = sample();
        assert_eq!(
            a.validate(1_050, &OTHER_INSTANCE, 42),
            Err(AttestationError::ForeignContractInstance)
        );
        assert!(!a.is_valid(1_050, &OTHER_INSTANCE, 42));
        // The identical ledger context succeeds on the bound instance.
        assert!(a.is_valid(1_050, &INSTANCE, 42));
    }

    #[test]
    fn operation_nonce_is_bound() {
        let a = sample();
        assert_eq!(
            a.validate(1_050, &INSTANCE, 43),
            Err(AttestationError::OperationNonceMismatch)
        );
        assert!(!a.matches_nonce(43));
        assert!(a.matches_nonce(42));
    }

    #[test]
    fn validate_reports_expiry_before_identity_mismatch() {
        // Deterministic precedence: a stale attestation is reported as stale
        // even when it is also presented to the wrong instance with the wrong
        // nonce, so the failure mode cannot be used to probe other fields.
        let a = sample();
        assert_eq!(
            a.validate(5_000, &OTHER_INSTANCE, 43),
            Err(AttestationError::Expired)
        );
        assert_eq!(
            a.validate(1, &OTHER_INSTANCE, 43),
            Err(AttestationError::NotYetIssued)
        );
    }

    #[test]
    fn remaining_ledgers_saturates_at_expiry() {
        let a = sample();
        assert_eq!(a.remaining_ledgers(1_000), 100);
        assert_eq!(a.remaining_ledgers(1_099), 1);
        assert_eq!(a.remaining_ledgers(1_100), 0);
        assert_eq!(a.remaining_ledgers(50_000), 0);
    }

    #[test]
    fn single_ledger_window_is_valid_for_exactly_one_ledger() {
        let a = Attestation::issue(77, 1, INSTANCE, 9).unwrap();
        assert_eq!(a.expiry_ledger(), 78);
        assert!(a.is_valid(77, &INSTANCE, 9));
        assert!(!a.is_valid(78, &INSTANCE, 9));
    }

    #[test]
    fn maximum_ledger_attestation_expires_at_u32_max() {
        let a = Attestation::issue(u32::MAX - 10, 10, INSTANCE, 1).unwrap();
        assert!(a.is_valid(u32::MAX - 1, &INSTANCE, 1));
        assert!(!a.is_valid(u32::MAX, &INSTANCE, 1));
        assert_eq!(
            a.validate(u32::MAX, &INSTANCE, 1),
            Err(AttestationError::Expired)
        );
    }

    #[test]
    fn instance_identity_can_be_any_comparable_type() {
        // The escrow contract binds the real `Address` it received, so the
        // generic parameter is what makes the module usable in the contract
        // instead of only in tests.
        let a = Attestation::issue(10, 5, "deployment-a", 3).unwrap();
        assert!(a.is_valid(10, &"deployment-a", 3));
        assert_eq!(
            a.validate(10, &"deployment-b", 3),
            Err(AttestationError::ForeignContractInstance)
        );
    }
}
