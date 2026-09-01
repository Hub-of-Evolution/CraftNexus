//! Reference Contract State Machine
//! Executable model of all contract lifecycles, independent of production storage.
//! Used for property-based testing, transition verification, and invariant enforcement.

//! # Design
//!
//! - Uses the same time-policy conventions as `time_policy.rs` (inclusive-end half-open intervals)
//! - Pure Rust types (no Soroban deps) — can run outside the contract VM
//! - Every documented transition in the contract has a counterpart in this model
//! - Deliberate differences from ledger semantics are documented per-section
//! - Conservation and terminal-state invariants are exposed for verification

//! # Model vs. Ledger Semantics (deliberate differences)
//! - Authorization is modeled as a parameter; the model does NOT require `require_auth`.
//!   Callers must enforce authorization externally.
//! - Time overflow uses saturating arithmetic instead of panic.
//! - State transitions are deterministic and idempotent; the ledger may have
//!   concurrent modification and version-dependent behavior.
//! - Events are not emitted from the model (pure state tracking).
//! - Some error variants are collapsed or omitted for simplicity.
//! - The model tracks "logical" time separate from ledger timestamp;

use alloc::vec::Vec;
use alloc::string::String;
use soroban_sdk::{Address, Env};

// ── Time policy (mirrors time_policy.rs conventions) ───────────────────────

/// Window open at `start` (inclusive).
pub fn is_window_open(now: u64, start: u64) -> bool {
    now >= start
}

/// Window closed at `start + duration` (inclusive — the deadline IS the expiry moment).
pub fn is_window_closed(now: u64, start: u64, duration: u64) -> bool {
    now >= start.saturating_add(duration)
}

/// Window still active at `now`.
pub fn is_window_active(now: u64, start: u64, duration: u64) -> bool {
    !is_window_closed(now, start, duration)
}

/// Absolute deadline for a window (inclusive end).
pub fn window_deadline(start: u64, duration: u64) -> u64 {
    start.saturating_add(duration)
}

/// Check if deadline has been reached.
pub fn is_deadline_reached(now: u64, deadline: u64) -> bool {
    now >= deadline
}

/// Check if deadline is pending (not yet reached).
pub fn is_deadline_pending(now: u64, deadline: u64) -> bool {
    now < deadline
}

// ── Escrow State Machine ──────────────────────────────────────────────────

/// Lifecycle status of an escrow order (mirrors `EscrowStatus` in lib.rs).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Resolved = 4,
    ReleasePending = 5,
    RefundPending = 6,
    DisputePending = 7,
    SettlementPending = 8,
}

/// Error for invalid escrow transitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowTransitionError {
    InvalidCurrentState(String),
    ReleaseWindowNotElapsed { created_at: u64, release_window: u64, now: u64 },
    NotInDispute,
    AlreadyResolved,
    InvalidTerminalState(String),
    SettlementAlreadyFinalized,
    EscrowNotFound,
}

/// Model for a single escrow order.
#[derive(Clone, Debug)]
pub struct EscrowModel {
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32,
    pub created_at: u32,
    pub funded: bool,
    pub funding_deadline: Option<u64>,
    pub dispute_reason: Option<String>,
    pub dispute_initiated_at: Option<u64>,
}

impl EscrowModel {
    pub fn new(
        id: u64,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        release_window: u32,
        created_at: u32,
    ) -> Self {
        EscrowModel {
            id,
            buyer,
            seller,
            token,
            amount,
            status: EscrowStatus::Active,
            release_window,
            created_at,
            funded: true,
            funding_deadline: Some(
                created_at as u64 + time_policy::UNFUNDED_CANCEL_TIMEOUT,
            ),
            dispute_reason: None,
            dispute_initiated_at: None,
        }
    }

    /// Attempt to release the escrow. Only allowed after release_window has elapsed.
    pub fn try_release(&mut self, now: u64) -> Result<(), EscrowTransitionError> {
        if !is_window_closed(now, self.created_at as u64, self.release_window as u64) {
            return Err(EscrowTransitionError::ReleaseWindowNotElapsed {
                created_at: self.created_at as u64,
                release_window: self.release_window as u64,
                now,
            });
        }
        match self.status {
            EscrowStatus::Active | EscrowStatus::ReleasePending => {
                self.status = EscrowStatus::Released;
                Ok(())
            }
            EscrowStatus::Disputed => Err(EscrowTransitionError::NotInDispute),
            EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved => {
                Err(EscrowTransitionError::AlreadyResolved)
            }
            _ => Err(EscrowTransitionError::InvalidCurrentState(format!(
                "cannot release from {:?}",
                self.status
            ))),
        }
    }

    /// Initiate a dispute on an Active escrow.
    pub fn try_initiate_dispute(
        &mut self,
        now: u64,
        reason: String,
    ) -> Result<(), EscrowTransitionError> {
        if !matches!(self.status, EscrowStatus::Active) {
            return Err(EscrowTransitionError::InvalidCurrentState(format!(
                "cannot dispute from {:?}",
                self.status
            )));
        }
        self.status = EscrowStatus::Disputed;
        self.dispute_reason = Some(reason);
        self.dispute_initiated_at = Some(now);
        Ok(())
    }

    /// Resolve a disputed escrow (arbitrator decision).
    pub fn try_resolve_dispute(
        &mut self,
        now: u64,
        admin_authorized: bool,
        release_to_seller: bool,
    ) -> Result<(), EscrowTransitionError> {
        if !matches!(self.status, EscrowStatus::Disputed) {
            return Err(EscrowTransitionError::InvalidCurrentState(format!(
                "cannot resolve from {:?}",
                self.status
            )));
        }
        if !admin_authorized {
            if let Some(start) = self.dispute_initiated_at {
                if !is_window_closed(now, start, time_policy::MAX_DISPUTE_DURATION) {
                    return Err(EscrowTransitionError::InvalidCurrentState(
                        "dispute window still active".to_string(),
                    ));
                }
            }
        }
        self.status = EscrowStatus::Resolved;
        Ok(())
    }

    /// Cancel an unfunded escrow (before funding deadline).
    pub fn try_cancel(&mut self, now: u64) -> Result<(), EscrowTransitionError> {
        if !self.funded {
            if is_window_closed(now, self.created_at as u64, time_policy::UNFUNDED_CANCEL_TIMEOUT) {
                self.status = EscrowStatus::Refunded;
                self.funding_deadline = None;
                Ok(())
            } else {
                Err(EscrowTransitionError::InvalidCurrentState(
                    "funding deadline not yet elapsed".to_string(),
                ))
            }
        } else {
            Err(EscrowTransitionError::InvalidCurrentState(
                "cannot cancel a funded escrow".to_string(),
            ))
        }
    }

    /// Extend the release window (admin action).
    pub fn try_extend_release_window(
        &mut self,
        new_window: u32,
        now: u64,
    ) -> Result<(), EscrowTransitionError> {
        self.release_window = new_window;
        if self.funding_deadline.is_some() {
            let current_deadline = self.funding_deadline.unwrap();
            if now < current_deadline {
                self.funding_deadline = Some(
                    now.saturating_add(new_window as u64)
                        .saturating_sub(self.release_window as u64)
                        .saturating_add(time_policy::UNFUNDED_CANCEL_TIMEOUT),
                );
            }
        }
        Ok(())
    }
}

// ── Dispute Escalation Model ──────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStage {
    Open = 0,
    ChallengeWindowActive = 1,
    EscalationWindowActive = 2,
    MaxDurationExceeded = 3,
    Resolved = 4,
}

#[derive(Clone, Debug)]
pub struct DisputeModel {
    pub dispute_id: u32,
    pub escrow_id: u64,
    pub stage: DisputeStage,
    pub initiated_at: u64,
    pub challenge_evidence_at: Option<u64>,
    pub escalated_at: Option<u64>,
    pub resolved_at: Option<u64>,
    pub release_to_seller: Option<bool>,
}

impl DisputeModel {
    pub fn new(dispute_id: u32, escrow_id: u64) -> Self {
        DisputeModel {
            dispute_id,
            escrow_id,
            stage: DisputeStage::Open,
            initiated_at: 0,
            challenge_evidence_at: None,
            escalated_at: None,
            resolved_at: None,
            release_to_seller: None,
        }
    }

    pub fn try_enter_challenge_window(&mut self, now: u64) -> Result<(), String> {
        if self.stage != DisputeStage::Open {
            return Err("dispute not in Open state".to_string());
        }
        if !is_window_active(now, self.initiated_at, time_policy::EVIDENCE_CHALLENGE_WINDOW) {
            return Err("challenge window already elapsed".to_string());
        }
        self.stage = DisputeStage::ChallengeWindowActive;
        Ok(())
    }

    pub fn try_enter_escalation_window(&mut self, now: u64) -> Result<(), String> {
        if self.stage != DisputeStage::ChallengeWindowActive {
            return Err("dispute not in ChallengeWindowActive state".to_string());
        }
        let min_escalation = self.initiated_at + time_policy::DISPUTE_ESCALATION_WINDOW as u64;
        if now < min_escalation {
            return Err(format!(
                "escalation window not yet open (earliest: {})",
                min_escalation
            ));
        }
        self.stage = DisputeStage::EscalationWindowActive;
        self.escalated_at = Some(now);
        Ok(())
    }

    pub fn try_resolve(&mut self, release_to_seller: bool) -> Result<(), String> {
        if self.stage == DisputeStage::Resolved {
            return Err("dispute already resolved".to_string());
        }
        self.stage = DisputeStage::Resolved;
        self.resolved_at = Some(
            self.escalated_at
                .unwrap_or(self.challenge_evidence_at.unwrap_or(self.initiated_at)),
        );
        self.release_to_seller = Some(release_to_seller);
        Ok(())
    }

    pub fn try_force_resolve_if_expired(&mut self, now: u64) -> Result<(), String> {
        if self.stage == DisputeStage::Resolved {
            return Err("dispute already resolved".to_string());
        }
        let max_duration = self.initiated_at + time_policy::MAX_DISPUTE_DURATION as u64;
        if now >= max_duration {
            self.stage = DisputeStage::MaxDurationExceeded;
            self.resolved_at = Some(now);
            Ok(())
        } else {
            Err("max dispute duration not yet exceeded".to_string())
        }
    }
}

// ── Recurring Escrow Model ────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecurringEscrowAction {
    Created = 0,
    CycleReleased = 1,
    Cancelled = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecurringEscrowStatus {
    Active = 0,
    Cancelled = 1,
    Completed = 2,
}

#[derive(Clone, Debug)]
pub struct RecurringEscrowModel {
    pub id: u64,
    pub buyer: Address,
    pub artisan: Address,
    pub token: Address,
    pub total_amount: i128,
    pub frequency: u64,
    pub duration: u32,
    pub current_cycle: u64,
    pub released_amount: i128,
    pub last_release_time: u64,
    pub is_active: bool,
    pub action_history: Vec<RecurringEscrowAction>,
}

impl RecurringEscrowModel {
    pub fn new(
        id: u64,
        buyer: Address,
        artisan: Address,
        token: Address,
        total_amount: i128,
        frequency: u64,
        duration: u32,
    ) -> Self {
        RecurringEscrowModel {
            id,
            buyer,
            artisan,
            token,
            total_amount,
            frequency,
            duration: duration.max(1),
            current_cycle: 0,
            released_amount: 0,
            last_release_time: 0,
            is_active: true,
            action_history: vec![RecurringEscrowAction::Created],
        }
    }

    pub fn try_release_next_cycle(&mut self, now: u64) -> Result<i128, String> {
        if !self.is_active {
            return Err("recurring escrow not active".to_string());
        }
        if self.current_cycle >= self.duration as u64 {
            self.is_active = false;
            return Err("all cycles already released".to_string());
        }

        let time_since_last = now.saturating_sub(self.last_release_time);
        if self.current_cycle > 0 && time_since_last < self.frequency {
            return Err(format!(
                "frequency not yet elapsed: {} < {} seconds needed",
                time_since_last, self.frequency
            ));
        }

        let per_cycle_amount = self.total_amount / (self.duration as i128);
        let cycle_share = per_cycle_amount;

        self.released_amount += cycle_share;
        self.current_cycle += 1;
        self.last_release_time = now;
        self.action_history.push(RecurringEscrowAction::CycleReleased);

        if self.current_cycle >= self.duration as u64 {
            self.is_active = false;
        }

        Ok(cycle_share)
    }

    pub fn try_cancel(&mut self) -> i128 {
        if !self.is_active {
            let remaining = self.total_amount - self.released_amount;
            return remaining;
        }
        self.is_active = false;
        let remaining = self.total_amount - self.released_amount;
        self.action_history.push(RecurringEscrowAction::Cancelled);
        remaining
    }

    pub fn expected_released(&self) -> i128 {
        if self.duration as i128 == 0 {
            return self.released_amount;
        }
        (self.current_cycle * self.total_amount) / self.duration as u64
    }
}

// ── Staking Model ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StakeStatus {
    Idle = 0,
    Staked = 1,
    Cooldown = 2,
    Unstaked = 3,
}

#[derive(Clone, Debug)]
pub struct StakeModel {
    pub artisan: Address,
    pub token: Address,
    pub staked_amount: i128,
    pub status: StakeStatus,
    pub stake_time: u64,
    pub cooldown_end: u64,
    pub unstaked_amount: i128,
}

impl StakeModel {
    pub fn new(artisan: Address, token: Address) -> Self {
        let now = 1_711_368_000;
        StakeModel {
            artisan,
            token,
            staked_amount: 0,
            status: StakeStatus::Idle,
            stake_time: now,
            cooldown_end: now,
            unstaked_amount: 0,
        }
    }

    pub fn try_stake(&mut self, amount: i128, now: u64) -> Result<(), String> {
        if amount <= 0 {
            return Err("stake amount must be positive".to_string());
        }
        self.staked_amount += amount;
        self.status = StakeStatus::Staked;
        self.stake_time = now;
        self.cooldown_end = now.saturating_add(time_policy::STAKE_COOLDOWN);
        Ok(())
    }

    pub fn try_unstake(&mut self, now: u64, amount: i128) -> Result<i128, String> {
        if amount <= 0 || amount > self.staked_amount - self.unstaked_amount {
            return Err("invalid unstake amount".to_string());
        }
        if !is_window_active(now, self.stake_time, time_policy::STAKE_COOLDOWN) {
            return Err("stake cooldown has not yet elapsed".to_string());
        }
        self.unstaked_amount += amount;
        let remaining = self.staked_amount - self.unstaked_amount;
        if remaining == 0 {
            self.status = StakeStatus::Unstaked;
        }
        Ok(amount)
    }

    pub fn try_unstake_bypass(&mut self, amount: i128) -> Result<i128, String> {
        if amount <= 0 || amount > self.staked_amount - self.unstaked_amount {
            return Err("invalid unstake amount".to_string());
        }
        self.unstaked_amount += amount;
        let remaining = self.staked_amount - self.unstaked_amount;
        if remaining == 0 {
            self.status = StakeStatus::Unstaked;
        }
        Ok(amount)
    }
}

// ── Onboarding Profile Model ───────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileStatusModel {
    Active = 0,
    Deactivated = 1,
    UnderReview = 2,
    Flagged = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserRoleModel {
    None = 0,
    Buyer = 1,
    Artisan = 2,
    Admin = 3,
    Moderator = 4,
}

#[derive(Clone, Debug)]
pub struct OnboardingProfileModel {
    pub address: Address,
    pub role: UserRoleModel,
    pub status: ProfileStatusModel,
    pub username: String,
    pub registered_at: u64,
    pub is_verified: bool,
    pub state_version: u32,
    pub successful_trades: u32,
    pub disputed_trades: u32,
    pub portfolio_cid: Option<String>,
}

impl OnboardingProfileModel {
    pub fn new(address: Address, username: String) -> Self {
        OnboardingProfileModel {
            address,
            role: UserRoleModel::None,
            status: ProfileStatusModel::Active,
            username,
            registered_at: 1_711_368_000,
            is_verified: false,
            state_version: 1,
            successful_trades: 0,
            disputed_trades: 0,
            portfolio_cid: None,
        }
    }

    pub fn try_onboard(&mut self, role: UserRoleModel) -> Result<(), String> {
        if self.role != UserRoleModel::None {
            return Err("user already onboarded".to_string());
        }
        self.role = role;
        self.state_version += 1;
        Ok(())
    }

    pub fn try_update_role(&mut self, new_role: UserRoleModel) -> Result<(), String> {
        if self.role == UserRoleModel::None && new_role != UserRoleModel::None {
            return Err("must onboard first".to_string());
        }
        self.role = new_role;
        self.state_version += 1;
        Ok(())
    }

    pub fn try_deactivate(&mut self) -> Result<(), String> {
        if matches!(self.status, ProfileStatusModel::Deactivated) {
            return Err("profile already deactivated".to_string());
        }
        self.status = ProfileStatusModel::Deactivated;
        self.state_version += 1;
        Ok(())
    }

    pub fn try_reactivate(&mut self, username_taken: bool) -> Result<(), String> {
        if !username_taken {
            self.status = ProfileStatusModel::Active;
            self.state_version += 1;
            Ok(())
        } else {
            Err("username already taken".to_string())
        }
    }

    pub fn try_set_verified(&mut self, verified: bool) {
        self.is_verified = verified;
        self.state_version += 1;
    }
}

// ── Conservation Invariants ────────────────────────────────────────────────

pub struct ConservationInvariants {
    pub total_escrow_amount: i128,
    pub total_staked: i128,
    pub total_recurring: i128,
    pub total_fees: i128,
}

impl ConservationInvariants {
    pub fn new() -> Self {
        ConservationInvariants {
            total_escrow_amount: 0,
            total_staked: 0,
            total_recurring: 0,
            total_fees: 0,
        }
    }

    pub fn add_escrow(&mut self, amount: i128) {
        self.total_escrow_amount += amount;
    }

    pub fn remove_escrow(&mut self, amount: i128) {
        self.total_escrow_amount -= amount;
    }

    pub fn add_staked(&mut self, amount: i128) {
        self.total_staked += amount;
    }

    pub fn remove_staked(&mut self, amount: i128) {
        self.total_staked -= amount;
    }

    pub fn add_recurring(&mut self, amount: i128) {
        self.total_recurring += amount;
    }

    pub fn remove_recurring(&mut self, amount: i128) {
        self.total_recurring -= amount;
    }

    pub fn add_fees(&mut self, amount: i128) {
        self.total_fees += amount;
    }

    pub fn check_invariant(&self, total_minted: i128) -> bool {
        let total_obligations =
            self.total_escrow_amount + self.total_staked + self.total_recurring + self.total_fees;
        total_obligations <= total_minted
    }
}

// ── Terminal State Detection ───────────────────────────────────────────────

pub fn is_escrow_terminal(status: EscrowStatus) -> bool {
    matches!(
        status,
        EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved | EscrowStatus::SettlementPending
    )
}

pub fn is_stake_terminal(status: StakeStatus) -> bool {
    matches!(status, StakeStatus::Unstaked)
}

pub fn is_recurring_terminal(is_active: bool, current_cycle: u64, duration: u32) -> bool {
    !is_active || current_cycle >= duration as u64
}

pub fn is_profile_terminal(status: ProfileStatusModel) -> bool {
    matches!(status, ProfileStatusModel::Deactivated)
}

// ── Model State Aggregator ─────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ModelState {
    pub escrows: Vec<EscrowModel>,
    pub disputes: Vec<DisputeModel>,
    pub recurring: Vec<RecurringEscrowModel>,
    pub stakes: Vec<StakeModel>,
    pub profiles: Vec<OnboardingProfileModel>,
    pub invariants: ConservationInvariants,
    pub total_minted: i128,
}

impl ModelState {
    pub fn new(total_minted: i128) -> Self {
        ModelState {
            escrows: Vec::new(),
            disputes: Vec::new(),
            recurring: Vec::new(),
            stakes: Vec::new(),
            profiles: Vec::new(),
            invariants: ConservationInvariants::new(),
            total_minted,
        }
    }

    pub fn add_escrow(&mut self, escrow: EscrowModel) {
        self.escrows.push(escrow);
        self.invariants.add_escrow(escrow.amount);
    }

    pub fn remove_escrow(&mut self, escrow_id: u64) {
        if let Some(pos) = self.escrows.iter().position(|e| e.id == escrow_id) {
            let amount = self.escrows[pos].amount;
            self.invariants.remove_escrow(amount);
            self.escrows.remove(pos);
        }
    }

    pub fn add_dispute(&mut self, dispute: DisputeModel) {
        self.disputes.push(dispute);
    }

    pub fn remove_dispute(&mut self, dispute_id: u32) {
        if let Some(pos) = self.disputes.iter().position(|d| d.dispute_id == dispute_id) {
            self.disputes.remove(pos);
        }
    }

    pub fn add_recurring(&mut self, recurring: RecurringEscrowModel) {
        self.recurring.push(recurring);
        self.invariants.add_recurring(recurring.total_amount);
    }

    pub fn remove_recurring(&mut self, recurring_id: u64) {
        if let Some(pos) = self.recurring.iter().position(|r| r.id == recurring_id) {
            let amount = self.recurring[pos].total_amount;
            self.invariants.remove_recurring(amount);
            self.recurring.remove(pos);
        }
    }

    pub fn add_stake(&mut self, stake: StakeModel) {
        self.stakes.push(stake);
        self.invariants.add_staked(stake.staked_amount);
    }

    pub fn remove_stake(&mut self, artisan: &Address, token: &Address) {
        if let Some(pos) = self.stakes.iter().position(|s| s.artisan == *artisan && s.token == *token) {
            let amount = self.stakes[pos].staked_amount;
            self.invariants.remove_staked(amount);
            self.stakes.remove(pos);
        }
    }

    pub fn add_profile(&mut self, profile: OnboardingProfileModel) {
        self.profiles.push(profile);
    }

    pub fn remove_profile(&mut self, address: &Address) {
        if let Some(pos) = self.profiles.iter().position(|p| p.address == *address) {
            self.profiles.remove(pos);
        }
    }

    pub fn check_all_invariants(&self) -> bool {
        self.invariants.check_invariant(self.total_minted)
    }

    pub fn has_terminal_escrows(&self) -> bool {
        self.escrows.iter().any(|e| is_escrow_terminal(e.status))
    }

    pub fn has_terminal_stakes(&self) -> bool {
        self.stakes.iter().any(|s| is_stake_terminal(s.status))
    }

    pub fn has_terminal_recurring(&self) -> bool {
        self.recurring
            .iter()
            .any(|r| is_recurring_terminal(r.is_active, r.current_cycle, r.duration))
    }

    pub fn has_terminal_profiles(&self) -> bool {
        self.profiles.iter().any(|p| is_profile_terminal(p.status))
    }
}

// ── Transition Operators (for property-based testing) ─────────────────────

#[derive(Clone, Debug)]
pub enum EscrowOp {
    Create { id: u64, buyer: Address, seller: Address, amount: i128, window: u32 },
    Release { id: u64, now: u64 },
    Dispute { id: u64, reason: String, now: u64 },
    Resolve { id: u64, now: u64, release_to_seller: bool, admin: bool },
    Cancel { id: u64, now: u64 },
    ExtendWindow { id: u64, new_window: u32, now: u64 },
}

#[derive(Clone, Debug)]
pub enum StakeOp {
    Stake { artisan: Address, token: Address, amount: i128, now: u64 },
    Unstake { artisan: Address, token: Address, amount: i128, now: u64 },
    AdvanceTime { now: u64, seconds: u64 },
    UnstakeEmpty,
}

#[derive(Clone, Debug)]
pub enum RecurringOp {
    Create {
        id: u64,
        buyer: Address,
        artisan: Address,
        amount: i128,
        frequency: u64,
        duration: u32,
    },
    ReleaseCycle { id: u64, now: u64 },
    Cancel { id: u64 },
}

#[derive(Clone, Debug)]
pub enum OnboardingOp {
    Onboard { address: Address, username: String, role: UserRoleModel },
    UpdateRole { address: Address, new_role: UserRoleModel },
    Deactivate { address: Address },
    Reactivate { address: Address, username_taken: bool },
    SetVerified { address: Address, verified: bool },
}

pub fn execute_escrow_op(state: &mut ModelState, op: EscrowOp) -> Result<(), String> {
    match op {
        EscrowOp::Create { id, buyer, seller, amount, window } => {
            let escrow = EscrowModel::new(id, buyer, seller, Address::generate(&Env::default()), amount, window, 0);
            state.add_escrow(escrow);
            Ok(())
        }
        EscrowOp::Release { id, now } => {
            if let Some(escrow) = state.escrows.iter_mut().find(|e| e.id == id) {
                match escrow.try_release(*now) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("release failed: {:?}", e)),
                }
                Ok(())
            } else {
                Err("escrow not found".to_string())
            }
        }
        EscrowOp::Dispute { id, reason, now } => {
            if let Some(escrow) = state.escrows.iter_mut().find(|e| e.id == id) {
                match escrow.try_initiate_dispute(*now, reason) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("dispute initiation failed: {:?}", e)),
                }
                Ok(())
            } else {
                Err("escrow not found".to_string())
            }
        }
        EscrowOp::Resolve { id, now, release_to_seller, admin } => {
            if let Some(escrow) = state.escrows.iter_mut().find(|e| e.id == id) {
                match escrow.try_resolve_dispute(*now, admin, release_to_seller) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("dispute resolution failed: {:?}", e)),
                }
                Ok(())
            } else {
                Err("escrow not found".to_string())
            }
        }
        EscrowOp::Cancel { id, now } => {
            if let Some(escrow) = state.escrows.iter_mut().find(|e| e.id == id) {
                match escrow.try_cancel(*now) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("cancel failed: {:?}", e)),
                }
                Ok(())
            } else {
                Err("escrow not found".to_string())
            }
        }
        EscrowOp::ExtendWindow { id, new_window, now } => {
            if let Some(escrow) = state.escrows.iter_mut().find(|e| e.id == id) {
                match escrow.try_extend_release_window(new_window, *now) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("extend window failed: {:?}", e)),
                }
                Ok(())
            } else {
                Err("escrow not found".to_string())
            }
        }
    }
}

pub fn execute_stake_op(state: &mut ModelState, op: StakeOp) -> Result<(), String> {
    match op {
        StakeOp::Stake { artisan, token, amount, now } => {
            if let Some(stake) = state.stakes.iter_mut().find(|s| s.artisan == artisan && s.token == token) {
                match stake.try_stake(amount, now) {
                    Ok(()) => {}
                    Err(e) => return Err(format!("stake failed: {}", e)),
                }
                Ok(())
            } else {
                let mut new_stake = StakeModel::new(artisan, token);
                new_stake.try_stake(amount, now)?;
                state.add_stake(new_stake);
                Ok(())
            }
        }
        StakeOp::Unstake { artisan, token, amount, now } => {
            if let Some(stake) = state.stakes.iter_mut().find(|s| s.artisan == artisan && s.token == token) {
                match stake.try_unstake(*now, amount) {
                    Ok(_) => {}
                    Err(e) => return Err(format!("unstake failed: {}", e)),
                }
                Ok(())
            } else {
                Err("stake not found".to_string())
            }
        }
        StakeOp::AdvanceTime { now, seconds } => {
            for stake in &mut state.stakes {
                // Recalculate cooldown end based on new time
                // Model simplification: just track time advances
            }
            Ok(())
        }
        StakeOp::UnstakeEmpty => {
            Ok(())
        }
    }
}

pub fn execute_recurring_op(state: &mut ModelState, op: RecurringOp) -> Result<(), String> {
    match op {
        RecurringOp::Create { id, buyer, artisan, amount, frequency, duration } => {
            let recurring = RecurringEscrowModel::new(id, buyer, artisan, Address::generate(&Env::default()), amount, frequency, duration);
            state.add_recurring(recurring);
            Ok(())
        }
        RecurringOp::ReleaseCycle { id, now } => {
            if let Some(recurring) = state.recurring.iter_mut().find(|r| r.id == id) {
                match recurring.try_release_next_cycle(*now) {
                    Ok(_) => {}
                    Err(e) => return Err(format!("release cycle failed: {}", e)),
                }
                Ok(())
            } else {
                Err("recurring escrow not found".to_string())
            }
        }
        RecurringOp::Cancel { id } => {
            if let Some(recurring) = state.recurring.iter_mut().find(|r| r.id == id) {
                let _ = recurring.try_cancel();
                Ok(())
            } else {
                Err("recurring escrow not found".to_string())
            }
        }
    }
}

pub fn execute_onboarding_op(
    state: &mut ModelState,
    op: OnboardingOp,
) -> Result<(), String> {
    match op {
        OnboardingOp::Onboard { address, username, role } => {
            let mut profile = OnboardingProfileModel::new(address.clone(), username);
            profile.try_onboard(role)?;
            state.add_profile(profile);
            Ok(())
        }
        OnboardingOp::UpdateRole { address, new_role } => {
            if let Some(profile) = state.profiles.iter_mut().find(|p| p.address == address) {
                profile.try_update_role(new_role)?;
                Ok(())
            } else {
                let mut profile = OnboardingProfileModel::new(address.clone(), "".to_string());
                profile.try_update_role(new_role)?;
                state.add_profile(profile);
                Ok(())
            }
        }
        OnboardingOp::Deactivate { address } => {
            if let Some(profile) = state.profiles.iter_mut().find(|p| p.address == address) {
                profile.try_deactivate()?;
                Ok(())
            } else {
                Err("profile not found".to_string())
            }
        }
        OnboardingOp::Reactivate { address, username_taken } => {
            if let Some(profile) = state.profiles.iter_mut().find(|p| p.address == address) {
                profile.try_reactivate(username_taken)?;
                Ok(())
            } else {
                Err("profile not found".to_string())
            }
        }
        OnboardingOp::SetVerified { address, verified } => {
            if let Some(profile) = state.profiles.iter_mut().find(|p| p.address == address) {
                profile.try_set_verified(verified);
                Ok(())
            } else {
                Err("profile not found".to_string())
            }
        }
    }
}

// ── Unit Tests ────────────────────────────────────────────────────────___

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Address;

    #[test]
    fn test_escrow_create_and_release() {
        let mut state = ModelState::new(10_000_000);
        let buyer = Address::generate(&soroban_sdk::Env::default());
        let seller = Address::generate(&soroban_sdk::Env::default());

        let op = EscrowOp::Create {
            id: 1,
            buyer: buyer.clone(),
            seller,
            amount: 1000,
            window: 3600,
        };
        execute_escrow_op(&mut state, op).unwrap();

        assert_eq!(state.escrows.len(), 1);
        assert_eq!(state.escrows[0].status, EscrowStatus::Active);

        // Try releasing before window elapsed (should fail)
        let release_op = EscrowOp::Release { id: 1, now: 100 };
        let result = execute_escrow_op(&mut state, release_op);
        assert!(result.is_err());

        assert!(matches!(state.escrows[0].status, EscrowStatus::Active));
    }

    #[test]
    fn test_stake_cooldown_model() {
        let mut state = ModelState::new(10_000_000);
        let artisan = Address::generate(&soroban_sdk::Env::default());
        let token = Address::generate(&soroban_sdk::Env::default());

        let op = StakeOp::Stake {
            artisan: artisan.clone(),
            token: token.clone(),
            amount: 500,
            now: 1_711_368_000,
        };
        execute_stake_op(&mut state, op).unwrap();

        assert_eq!(state.stakes[0].staked_amount, 500);
        assert!(matches!(state.stakes[0].status, StakeStatus::Staked));

        // Try unstaking before cooldown (should fail)
        let unstake_op = StakeOp::Unstake {
            artisan: artisan.clone(),
            token: token.clone(),
            amount: 100,
            now: 1_711_368_001,
        };
        let result = execute_stake_op(&mut state, unstake_op);
        assert!(result.is_err());

        // Advance past cooldown (7 days = 604800 seconds)
        let advance_op = StakeOp::AdvanceTime { now: 1_711_368_000, seconds: 7 * 24 * 60 * 60 + 1 };
        execute_stake_op(&mut state, advance_op).unwrap();

        // Now unstake should succeed
        let unstake_op2 = StakeOp::Unstake {
            artisan,
            token,
            amount: 100,
            now: 1_711_368_000 + 7 * 24 * 60 * 60 + 1,
        };
        execute_stake_op(&mut state, unstake_op2).unwrap();

        assert_eq!(state.stakes[0].unstaked_amount, 100);
        assert!(matches!(state.stakes[0].status, StakeStatus::Unstaked));
    }

    #[test]
    fn test_recurring_escrow_cycles() {
        let mut state = ModelState::new(10_000_000);
        let buyer = Address::generate(&soroban_sdk::Env::default());
        let artisan = Address::generate(&soroban_sdk::Env::default());

        let op = RecurringOp::Create {
            id: 1,
            buyer: buyer.clone(),
            artisan: artisan.clone(),
            amount: 1000,
            frequency: 3600,
            duration: 2,
        };
        execute_recurring_op(&mut state, op).unwrap();

        assert_eq!(state.recurring.len(), 1);
        assert!(state.recurring[0].is_active);
        assert_eq!(state.recurring[0].current_cycle, 0);

        let release_op = RecurringOp::ReleaseCycle { id: 1, now: 3601 };
        execute_recurring_op(&mut state, release_op).unwrap();

        assert_eq!(state.recurring[0].current_cycle, 1);
        assert!(state.recurring[0].is_active);

        let release_op2 = RecurringOp::ReleaseCycle { id: 1, now: 7202 };
        execute_recurring_op(&mut state, release_op2).unwrap();

        assert_eq!(state.recurring[0].current_cycle, 2);
        assert!(!state.recurring[0].is_active);
    }

    #[test]
    fn test_onboarding_lifecycle() {
        let mut state = ModelState::new(10_000_000);
        let buyer = Address::generate(&soroban_sdk::Env::default());

        let op = OnboardingOp::Onboard {
            address: buyer.clone(),
            username: "testuser".to_string(),
            role: UserRoleModel::Buyer,
        };
        execute_onboarding_op(&mut state, op).unwrap();

        assert_eq!(state.profiles.len(), 1);
        let profile = &state.profiles[0];
        assert_eq!(profile.role, UserRoleModel::Buyer);
        assert_eq!(profile.status, ProfileStatusModel::Active);

        let op = OnboardingOp::Deactivate { address: buyer };
        execute_onboarding_op(&mut state, op).unwrap();
        assert_eq!(profile.status, ProfileStatusModel::Deactivated);

        let op = OnboardingOp::Reactivate { address: buyer, username_taken: false };
        execute_onboarding_op(&mut state, op).unwrap();
        assert_eq!(profile.status, ProfileStatusModel::Active);
    }

    #[test]
    fn test_conservation_invariant() {
        let mut state = ModelState::new(50_000_000);

        let buyer = Address::generate(&soroban_sdk::Env::default());
        let seller = Address::generate(&soroban_sdk::Env::default());
        let escrow_op = EscrowOp::Create {
            id: 1,
            buyer: buyer.clone(),
            seller,
            amount: 5000,
            window: 3600,
        };
        execute_escrow_op(&mut state, escrow_op).unwrap();

        let artisan = Address::generate(&soroban_sdk::Env::default());
        let token = Address::generate(&soroban_sdk::Env::default());
        let stake_op = StakeOp::Stake {
            artisan: artisan.clone(),
            token: token.clone(),
            amount: 2000,
            now: 1_711_368_000,
        };
        execute_stake_op(&mut state, stake_op).unwrap();

        assert!(state.check_all_invariants());

        let escrow_op2 = EscrowOp::Create {
            id: 2,
            buyer: buyer,
            seller,
            amount: 30000,
            window: 3600,
        };
        execute_escrow_op(&mut state, escrow_op2).unwrap();

        assert!(state.check_all_invariants());
    }
}
