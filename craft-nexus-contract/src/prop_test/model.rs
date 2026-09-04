//! Pure reference state machine for CraftNexus contracts.
#![allow(dead_code)]
//!
//! This module contains no Soroban SDK calls. It models the *documented*
//! state transitions so property tests can compare contract behaviour
//! against a ground-truth specification.
//!
//! # Deliberate simplifications vs. on-chain semantics
//!
//! | On-chain | Model | Reason |
//! |---|---|---|
//! | CEI pending states (ReleasePending/RefundPending/…) | Skipped directly to terminal | Pending states are internal re-entrancy guards, not part of the public API |
//! | `funded` flag | Model treats every created escrow as funded | Token mechanics are tested by integration tests |
//! | TTL / ledger archival | Not modelled | No ledger concept in pure Rust |
//! | Multi-sig upgrade threshold snapshots | Simplified to a single signer list | Full behaviour tested in upgrade_props.rs |
//! | Rate-limit counters on onboarding | Not modelled | Orthogonal to state machine correctness |

extern crate alloc;
use alloc::{collections::BTreeMap, string::String, vec::Vec};

// ── Escrow model ──────────────────────────────────────────────────────────────

/// Mirror of `EscrowStatus` without SDK dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEscrowStatus {
    Active,
    Released,
    Refunded,
    Disputed,
    Resolved,
}

impl ModelEscrowStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ModelEscrowStatus::Released | ModelEscrowStatus::Refunded | ModelEscrowStatus::Resolved
        )
    }
}

/// Lightweight escrow record in the reference model.
#[derive(Clone, Debug)]
pub struct ModelEscrow {
    pub id: u32,
    pub buyer: String,
    pub seller: String,
    pub token: String,
    pub amount: i128,
    pub status: ModelEscrowStatus,
    pub release_window: u64,
    pub created_at: u64,
    pub dispute_initiated_at: Option<u64>,
    /// Track whether a settlement has been recorded (prevents double-settle).
    pub settlement_finalized: bool,
}

impl ModelEscrow {
    pub fn new(
        id: u32,
        buyer: String,
        seller: String,
        token: String,
        amount: i128,
        release_window: u64,
        now: u64,
    ) -> Self {
        Self {
            id,
            buyer,
            seller,
            token,
            amount,
            status: ModelEscrowStatus::Active,
            release_window,
            created_at: now,
            dispute_initiated_at: None,
            settlement_finalized: false,
        }
    }

    /// Whether the release window has elapsed relative to `now`.
    pub fn window_elapsed(&self, now: u64) -> bool {
        crate::time_policy::is_window_elapsed(now, self.created_at, self.release_window)
    }

    /// Whether the maximum dispute duration has elapsed relative to `now`.
    pub fn dispute_expired(&self, now: u64, max_dispute_duration: u64) -> bool {
        match self.dispute_initiated_at {
            Some(t) => crate::time_policy::is_window_elapsed(now, t, max_dispute_duration),
            None => false,
        }
    }
}

// ── Staking model ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ModelStakeDeposit {
    pub amount: i128,
    pub cooldown_end: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ModelArtisanStake {
    pub total: i128,
    pub token: Option<String>,
    /// Ordered queue of individual deposits (FIFO for withdrawal).
    pub queue: Vec<ModelStakeDeposit>,
}

impl ModelArtisanStake {
    /// Earliest timestamp at which any deposit has matured, or None if queue is empty.
    pub fn earliest_maturity(&self) -> Option<u64> {
        self.queue.iter().map(|d| d.cooldown_end).min()
    }

    /// True if at least one deposit can be unstaked at `now`.
    pub fn has_matured(&self, now: u64) -> bool {
        self.queue
            .iter()
            .any(|d| crate::time_policy::is_deadline_reached(now, d.cooldown_end))
    }

    /// Sum of matured amounts at `now`.
    pub fn matured_amount(&self, now: u64) -> i128 {
        self.queue
            .iter()
            .filter(|d| crate::time_policy::is_deadline_reached(now, d.cooldown_end))
            .map(|d| d.amount)
            .sum()
    }
}

// ── Upgrade model ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelUpgradeStatus {
    None,
    Proposed { upgrade_at: u64 },
    Executed,
}

// ── Onboarding model ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelUserRole {
    None,
    Buyer,
    Artisan,
    Admin,
    Arbitrator,
    Moderator,
}

#[derive(Clone, Debug)]
pub struct ModelUserProfile {
    pub address: String,
    pub role: ModelUserRole,
    pub verified: bool,
    pub active: bool,
    pub reputation_score: u32,
    pub active_contracts: u32,
}

// ── Platform model ────────────────────────────────────────────────────────────

/// Full mutable state of the reference model.
#[derive(Clone, Debug)]
pub struct ModelState {
    // Escrows
    pub escrows: BTreeMap<u32, ModelEscrow>,
    pub next_escrow_id: u32,

    // Staking (artisan address → stake data)
    pub stakes: BTreeMap<String, ModelArtisanStake>,

    // Upgrade
    pub upgrade_status: ModelUpgradeStatus,
    pub last_cancel_at: Option<u64>,

    // Platform
    pub is_paused: bool,
    pub platform_fee_bps: u32,
    pub min_stake_required: i128,
    pub max_dispute_duration: u64,
    pub stake_cooldown: u64,

    // Onboarding (address → profile)
    pub profiles: BTreeMap<String, ModelUserProfile>,

    // Fund conservation: token → total locked
    pub locked: BTreeMap<String, i128>,

    // Upgrade cooldown (seconds)
    pub wasm_upgrade_cooldown: u64,
    /// Monotonically increasing nonce; incremented on cancel.
    pub upgrade_nonce: u32,
}

impl ModelState {
    pub fn new() -> Self {
        Self {
            escrows: BTreeMap::new(),
            next_escrow_id: 1,
            stakes: BTreeMap::new(),
            upgrade_status: ModelUpgradeStatus::None,
            last_cancel_at: None,
            is_paused: false,
            platform_fee_bps: 500,
            min_stake_required: 0,
            max_dispute_duration: 30 * 24 * 60 * 60,
            stake_cooldown: 7 * 24 * 60 * 60,
            profiles: BTreeMap::new(),
            locked: BTreeMap::new(),
            wasm_upgrade_cooldown: 7 * 24 * 60 * 60,
            upgrade_nonce: 0,
        }
    }

    // ── Escrow transitions ────────────────────────────────────────────────────

    /// Create an escrow. Returns Err if the platform is paused.
    pub fn create_escrow(
        &mut self,
        buyer: String,
        seller: String,
        token: String,
        amount: i128,
        order_id: u32,
        release_window: u64,
        now: u64,
    ) -> Result<(), ModelError> {
        if self.is_paused {
            return Err(ModelError::ContractPaused);
        }
        if buyer == seller {
            return Err(ModelError::SameBuyerSeller);
        }
        if amount <= 0 {
            return Err(ModelError::AmountBelowMinimum);
        }
        if self.escrows.contains_key(&order_id) {
            return Err(ModelError::EscrowAlreadyExists);
        }
        let escrow = ModelEscrow::new(
            order_id,
            buyer.clone(),
            seller.clone(),
            token.clone(),
            amount,
            release_window,
            now,
        );
        *self.locked.entry(token).or_insert(0) += amount;
        self.escrows.insert(order_id, escrow);
        Ok(())
    }

    /// Release funds to seller.
    pub fn release_escrow(
        &mut self,
        order_id: u32,
        caller: &str,
        now: u64,
    ) -> Result<(), ModelError> {
        if self.is_paused {
            return Err(ModelError::ContractPaused);
        }
        let escrow = self
            .escrows
            .get_mut(&order_id)
            .ok_or(ModelError::EscrowNotFound)?;

        if escrow.status != ModelEscrowStatus::Active {
            return Err(ModelError::InvalidEscrowState);
        }
        // Only buyer or auto-release is modelled; admin path skipped for brevity
        if caller != escrow.buyer && !escrow.window_elapsed(now) {
            return Err(ModelError::Unauthorized);
        }
        let fee = (escrow.amount * self.platform_fee_bps as i128) / 10_000;
        let net = escrow.amount - fee;
        let token = escrow.token.clone();
        escrow.status = ModelEscrowStatus::Released;
        escrow.settlement_finalized = true;

        let locked = self.locked.entry(token).or_insert(0);
        *locked = locked.saturating_sub(escrow.amount);
        let _ = net; // verified by invariant tests
        Ok(())
    }

    /// Refund funds to buyer (admin, buyer, or post-deadline).
    pub fn refund_escrow(
        &mut self,
        order_id: u32,
        caller: &str,
        admin: &str,
        __now: u64,
    ) -> Result<(), ModelError> {
        if self.is_paused {
            return Err(ModelError::ContractPaused);
        }
        let escrow = self
            .escrows
            .get_mut(&order_id)
            .ok_or(ModelError::EscrowNotFound)?;

        if escrow.status != ModelEscrowStatus::Active {
            return Err(ModelError::InvalidEscrowState);
        }
        if caller != admin && caller != escrow.buyer {
            return Err(ModelError::Unauthorized);
        }
        let token = escrow.token.clone();
        escrow.status = ModelEscrowStatus::Refunded;
        escrow.settlement_finalized = true;

        let locked = self.locked.entry(token).or_insert(0);
        *locked = locked.saturating_sub(escrow.amount);
        Ok(())
    }

    /// Open a dispute (buyer or seller).
    pub fn dispute_escrow(
        &mut self,
        order_id: u32,
        caller: &str,
        now: u64,
    ) -> Result<(), ModelError> {
        if self.is_paused {
            return Err(ModelError::ContractPaused);
        }
        let escrow = self
            .escrows
            .get_mut(&order_id)
            .ok_or(ModelError::EscrowNotFound)?;

        if escrow.status != ModelEscrowStatus::Active {
            return Err(ModelError::InvalidEscrowState);
        }
        if caller != escrow.buyer && caller != escrow.seller {
            return Err(ModelError::Unauthorized);
        }
        escrow.status = ModelEscrowStatus::Disputed;
        escrow.dispute_initiated_at = Some(now);
        Ok(())
    }

    /// Resolve a dispute (arbitrator).
    pub fn resolve_dispute(
        &mut self,
        order_id: u32,
        caller: &str,
        arbitrator: &str,
        release_to_seller: bool,
        now: u64,
    ) -> Result<(), ModelError> {
        let escrow = self
            .escrows
            .get_mut(&order_id)
            .ok_or(ModelError::EscrowNotFound)?;

        if escrow.status != ModelEscrowStatus::Disputed {
            return Err(ModelError::NotInDispute);
        }
        if caller != arbitrator {
            return Err(ModelError::Unauthorized);
        }
        // Arbitrator cannot resolve after max_dispute_duration
        if escrow.dispute_expired(now, self.max_dispute_duration) {
            return Err(ModelError::ArbitratorDeadlineExceeded);
        }
        if escrow.settlement_finalized {
            return Err(ModelError::SettlementAlreadyFinalized);
        }
        let token = escrow.token.clone();
        escrow.status = ModelEscrowStatus::Resolved;
        escrow.settlement_finalized = true;

        let locked = self.locked.entry(token).or_insert(0);
        *locked = locked.saturating_sub(escrow.amount);
        let _ = release_to_seller;
        Ok(())
    }

    /// Force-resolve an expired dispute (anyone, after deadline).
    pub fn resolve_expired_dispute(&mut self, order_id: u32, now: u64) -> Result<(), ModelError> {
        let escrow = self
            .escrows
            .get_mut(&order_id)
            .ok_or(ModelError::EscrowNotFound)?;

        if escrow.status != ModelEscrowStatus::Disputed {
            return Err(ModelError::NotInDispute);
        }
        if !escrow.dispute_expired(now, self.max_dispute_duration) {
            return Err(ModelError::DisputeNotExpired);
        }
        if escrow.settlement_finalized {
            return Err(ModelError::SettlementAlreadyFinalized);
        }
        let token = escrow.token.clone();
        escrow.status = ModelEscrowStatus::Resolved;
        escrow.settlement_finalized = true;

        let locked = self.locked.entry(token).or_insert(0);
        *locked = locked.saturating_sub(escrow.amount);
        Ok(())
    }

    // ── Staking transitions ───────────────────────────────────────────────────

    /// Stake `amount` of `token` for `artisan` with a cooldown end of `now + stake_cooldown`.
    pub fn stake(
        &mut self,
        artisan: String,
        token: String,
        amount: i128,
        now: u64,
    ) -> Result<(), ModelError> {
        if amount <= 0 {
            return Err(ModelError::AmountBelowMinimum);
        }
        let cooldown = self.stake_cooldown;
        let stake = self.stakes.entry(artisan).or_default();
        if let Some(ref existing_token) = stake.token {
            if *existing_token != token {
                return Err(ModelError::StakeTokenMismatch);
            }
        } else {
            stake.token = Some(token);
        }
        stake.total += amount;
        stake.queue.push(ModelStakeDeposit {
            amount,
            cooldown_end: now.saturating_add(cooldown),
        });
        Ok(())
    }

    /// Unstake matured deposits for `artisan`. Returns the amount released.
    pub fn unstake(
        &mut self,
        artisan: &str,
        token: &str,
        amount: i128,
        now: u64,
    ) -> Result<i128, ModelError> {
        let stake = self
            .stakes
            .get_mut(artisan)
            .ok_or(ModelError::InsufficientStake)?;

        if let Some(ref t) = stake.token {
            if t != token {
                return Err(ModelError::StakeTokenMismatch);
            }
        }

        // Verify at least one deposit has matured
        if !stake.has_matured(now) {
            return Err(ModelError::StakeCooldownActive);
        }

        // Greedy withdrawal from matured deposits
        let mut remaining = amount;
        let mut released = 0i128;
        let mut new_queue = Vec::new();
        for deposit in stake.queue.drain(..) {
            if remaining > 0 && crate::time_policy::is_deadline_reached(now, deposit.cooldown_end) {
                let take = remaining.min(deposit.amount);
                released += take;
                remaining -= take;
                if deposit.amount > take {
                    new_queue.push(ModelStakeDeposit {
                        amount: deposit.amount - take,
                        cooldown_end: deposit.cooldown_end,
                    });
                }
            } else {
                new_queue.push(deposit);
            }
        }
        stake.queue = new_queue;
        stake.total -= released;

        if released == 0 {
            return Err(ModelError::InsufficientStake);
        }
        Ok(released)
    }

    // ── Upgrade transitions ───────────────────────────────────────────────────

    /// Propose a WASM upgrade. Returns Err if one is already pending or
    /// the cancel-repropose cooldown is still active.
    pub fn propose_upgrade(&mut self, now: u64) -> Result<(), ModelError> {
        if self.upgrade_status != ModelUpgradeStatus::None {
            return Err(ModelError::UpgradeProposalExists);
        }
        // Cancel-repropose cooldown check
        if let Some(cancelled_at) = self.last_cancel_at {
            if crate::time_policy::is_window_active(
                now,
                cancelled_at,
                crate::time_policy::CANCEL_REPROPOSE_COOLDOWN,
            ) {
                return Err(ModelError::UpgradeCooldownActive);
            }
        }
        let upgrade_at = now.saturating_add(self.wasm_upgrade_cooldown);
        self.upgrade_status = ModelUpgradeStatus::Proposed { upgrade_at };
        Ok(())
    }

    /// Cancel a pending upgrade proposal.
    pub fn cancel_upgrade(&mut self, now: u64) -> Result<(), ModelError> {
        match self.upgrade_status {
            ModelUpgradeStatus::Proposed { .. } => {
                self.upgrade_status = ModelUpgradeStatus::None;
                self.last_cancel_at = Some(now);
                self.upgrade_nonce = self.upgrade_nonce.saturating_add(1);
                Ok(())
            }
            _ => Err(ModelError::NoUpgradeProposed),
        }
    }

    /// Execute an upgrade (after cooldown).
    pub fn execute_upgrade(&mut self, now: u64) -> Result<(), ModelError> {
        match self.upgrade_status {
            ModelUpgradeStatus::Proposed { upgrade_at } => {
                // Time policy: upgrade is ready when now >= upgrade_at (inclusive end)
                if crate::time_policy::is_deadline_pending(now, upgrade_at) {
                    return Err(ModelError::UpgradeCooldownActive);
                }
                self.upgrade_status = ModelUpgradeStatus::Executed;
                Ok(())
            }
            ModelUpgradeStatus::None => Err(ModelError::NoUpgradeProposed),
            ModelUpgradeStatus::Executed => Err(ModelError::NoUpgradeProposed),
        }
    }

    // ── Platform transitions ──────────────────────────────────────────────────

    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }

    // ── Onboarding transitions ────────────────────────────────────────────────

    pub fn onboard_user(&mut self, address: String, role: ModelUserRole) -> Result<(), ModelError> {
        if self.is_paused {
            return Err(ModelError::ContractPaused);
        }
        if self.profiles.contains_key(&address) {
            return Err(ModelError::AlreadyOnboarded);
        }
        if matches!(role, ModelUserRole::None | ModelUserRole::Admin) {
            return Err(ModelError::Unauthorized);
        }
        self.profiles.insert(
            address.clone(),
            ModelUserProfile {
                address,
                role,
                verified: false,
                active: true,
                reputation_score: 0,
                active_contracts: 0,
            },
        );
        Ok(())
    }

    pub fn verify_user(&mut self, address: &str) -> Result<(), ModelError> {
        let profile = self
            .profiles
            .get_mut(address)
            .ok_or(ModelError::UserNotFound)?;
        profile.verified = true;
        Ok(())
    }

    pub fn deactivate_profile(&mut self, address: &str) -> Result<(), ModelError> {
        let profile = self
            .profiles
            .get_mut(address)
            .ok_or(ModelError::UserNotFound)?;
        if profile.active_contracts > 0 {
            return Err(ModelError::HasActiveContracts);
        }
        profile.active = false;
        Ok(())
    }

    pub fn reactivate_profile(&mut self, address: &str) -> Result<(), ModelError> {
        let profile = self
            .profiles
            .get_mut(address)
            .ok_or(ModelError::UserNotFound)?;
        if profile.active {
            return Err(ModelError::AlreadyActive);
        }
        profile.active = true;
        Ok(())
    }

    // ── Invariant helpers ─────────────────────────────────────────────────────

    /// All locked amounts are non-negative.
    pub fn check_locked_non_negative(&self) -> bool {
        self.locked.values().all(|v| *v >= 0)
    }

    /// All active escrow amounts sum to the locked amount for their token.
    pub fn check_fund_conservation(&self) -> Result<(), String> {
        let mut expected: BTreeMap<String, i128> = BTreeMap::new();
        for escrow in self.escrows.values() {
            if !escrow.status.is_terminal() {
                *expected.entry(escrow.token.clone()).or_insert(0) += escrow.amount;
            }
        }
        for (token, exp) in &expected {
            let actual = self.locked.get(token).copied().unwrap_or(0);
            if actual != *exp {
                return Err(alloc::format!(
                    "fund conservation violation for token {}: expected {} locked, got {}",
                    token,
                    exp,
                    actual
                ));
            }
        }
        Ok(())
    }

    /// No escrow transitions out of a terminal state.
    pub fn check_no_terminal_re_entry(&self) -> Result<(), String> {
        // This is enforced structurally by the transition functions returning
        // InvalidEscrowState for terminal escrows; we verify it holds in the model
        // by checking that all terminal escrows have settlement_finalized=true.
        for escrow in self.escrows.values() {
            if escrow.status.is_terminal() && !escrow.settlement_finalized {
                return Err(alloc::format!(
                    "escrow {} is terminal but settlement_finalized=false",
                    escrow.id
                ));
            }
        }
        Ok(())
    }

    /// Staking invariant: total equals sum of queue amounts.
    pub fn check_stake_queue_consistency(&self) -> Result<(), String> {
        for (artisan, stake) in &self.stakes {
            let sum: i128 = stake.queue.iter().map(|d| d.amount).sum();
            if sum != stake.total {
                return Err(alloc::format!(
                    "artisan {} stake total {} != queue sum {}",
                    artisan,
                    stake.total,
                    sum
                ));
            }
        }
        Ok(())
    }

    /// Upgrade nonce monotonicity: can never decrease.
    pub fn upgrade_nonce(&self) -> u32 {
        self.upgrade_nonce
    }
}

impl Default for ModelState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Model error ───────────────────────────────────────────────────────────────

/// Errors the reference model can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    ContractPaused,
    SameBuyerSeller,
    AmountBelowMinimum,
    EscrowNotFound,
    EscrowAlreadyExists,
    InvalidEscrowState,
    Unauthorized,
    NotInDispute,
    DisputeNotExpired,
    ArbitratorDeadlineExceeded,
    SettlementAlreadyFinalized,
    InsufficientStake,
    StakeCooldownActive,
    StakeTokenMismatch,
    UpgradeProposalExists,
    UpgradeCooldownActive,
    NoUpgradeProposed,
    AlreadyOnboarded,
    UserNotFound,
    HasActiveContracts,
    AlreadyActive,
}
#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum Op {
        CreateEscrow { buyer: usize, seller: usize, token: usize, amount: i128, order_id: u32, release_window: u64, now: u64 },
        ReleaseEscrow { order_id: u32, caller: usize, now: u64 },
        RefundEscrow { order_id: u32, caller: usize, admin: usize, now: u64 },
        DisputeEscrow { order_id: u32, caller: usize, now: u64 },
        ResolveDispute { order_id: u32, caller: usize, arbitrator: usize, release_to_seller: bool, now: u64 },
        ResolveExpiredDispute { order_id: u32, now: u64 },
        Stake { artisan: usize, token: usize, amount: i128, now: u64 },
        Unstake { artisan: usize, token: usize, amount: i128, now: u64 },
        ProposeUpgrade { now: u64 },
        CancelUpgrade { now: u64 },
        ExecuteUpgrade { now: u64 },
        SetPaused { paused: bool },
        OnboardUser { address: usize, role: ModelUserRole },
        VerifyUser { address: usize },
        DeactivateProfile { address: usize },
        ReactivateProfile { address: usize },
    }

    fn actor(i: usize) -> String {
        alloc::format!("actor-{}", i % 4)
    }

    fn token_name(i: usize) -> String {
        alloc::format!("token-{}", i % 3)
    }

    struct FuzzRng {
        state: u64,
    }

    impl FuzzRng {
        fn new(seed: &[u8]) -> Self {
            let mut state = 0x9E3779B97F4A7C15u64;
            for &b in seed {
                state ^= u64::from(b);
                state = state.wrapping_mul(0x100000001B3u64).rotate_left(23);
            }
            FuzzRng { state }
        }

        fn next_u64(&mut self) -> u64 {
            let mut z = self.state.wrapping_add(0x9E3779B97F4A7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^= z >> 31;
            self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
            z
        }

        fn next_below(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }

        fn next_i128_below(&mut self, n: i128) -> i128 {
            (self.next_u64() as i128) % n
        }

        fn next_bool(&mut self) -> bool {
            self.next_u64() & 1 == 1
        }
    }

    fn generate_ops(seed: &[u8]) -> Vec<Op> {
        let mut rng = FuzzRng::new(seed);
        let count = (rng.next_u64() % 32) as usize;
        let mut ops = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = rng.next_below(16);
            let op = match kind {
                0 => Op::CreateEscrow {
                    buyer: rng.next_below(4) as usize,
                    seller: rng.next_below(4) as usize,
                    token: rng.next_below(3) as usize,
                    amount: rng.next_i128_below(2001) - 1000,
                    order_id: rng.next_below(1 << 32) as u32,
                    release_window: rng.next_below(10000),
                    now: rng.next_below(10000),
                },
                1 => Op::ReleaseEscrow {
                    order_id: rng.next_below(1 << 32) as u32,
                    caller: rng.next_below(4) as usize,
                    now: rng.next_below(10000),
                },
                2 => Op::RefundEscrow {
                    order_id: rng.next_below(1 << 32) as u32,
                    caller: rng.next_below(4) as usize,
                    admin: rng.next_below(4) as usize,
                    now: rng.next_below(10000),
                },
                3 => Op::DisputeEscrow {
                    order_id: rng.next_below(1 << 32) as u32,
                    caller: rng.next_below(4) as usize,
                    now: rng.next_below(10000),
                },
                4 => Op::ResolveDispute {
                    order_id: rng.next_below(1 << 32) as u32,
                    caller: rng.next_below(4) as usize,
                    arbitrator: rng.next_below(4) as usize,
                    release_to_seller: rng.next_bool(),
                    now: rng.next_below(10000),
                },
                5 => Op::ResolveExpiredDispute {
                    order_id: rng.next_below(1 << 32) as u32,
                    now: rng.next_below(10000),
                },
                6 => Op::Stake {
                    artisan: rng.next_below(4) as usize,
                    token: rng.next_below(3) as usize,
                    amount: rng.next_i128_below(2001) - 1000,
                    now: rng.next_below(10000),
                },
                7 => Op::Unstake {
                    artisan: rng.next_below(4) as usize,
                    token: rng.next_below(3) as usize,
                    amount: rng.next_i128_below(2001) - 1000,
                    now: rng.next_below(10000),
                },
                8 => Op::ProposeUpgrade { now: rng.next_below(10000) },
                9 => Op::CancelUpgrade { now: rng.next_below(10000) },
                10 => Op::ExecuteUpgrade { now: rng.next_below(10000) },
                11 => Op::SetPaused { paused: rng.next_bool() },
                12 => Op::OnboardUser {
                    address: rng.next_below(4) as usize,
                    role: match rng.next_below(5) {
                        0 => ModelUserRole::Buyer,
                        1 => ModelUserRole::Artisan,
                        2 => ModelUserRole::Admin,
                        3 => ModelUserRole::Arbitrator,
                        _ => ModelUserRole::Moderator,
                    },
                },
                13 => Op::VerifyUser { address: rng.next_below(4) as usize },
                14 => Op::DeactivateProfile { address: rng.next_below(4) as usize },
                _ => Op::ReactivateProfile { address: rng.next_below(4) as usize },
            };
            ops.push(op);
        }
        ops
    }

    fn run_op(model: &mut ModelState, op: Op) -> Result<(), ModelError> {
        match op {
            Op::CreateEscrow { buyer, seller, token, amount, order_id, release_window, now } => {
                model.create_escrow(actor(buyer), actor(seller), token_name(token), amount, order_id, release_window, now)
            }
            Op::ReleaseEscrow { order_id, caller, now } => {
                model.release_escrow(order_id, &actor(caller), now)
            }
            Op::RefundEscrow { order_id, caller, admin, now } => {
                model.refund_escrow(order_id, &actor(caller), &actor(admin), now)
            }
            Op::DisputeEscrow { order_id, caller, now } => {
                model.dispute_escrow(order_id, &actor(caller), now)
            }
            Op::ResolveDispute { order_id, caller, arbitrator, release_to_seller, now } => {
                model.resolve_dispute(order_id, &actor(caller), &actor(arbitrator), release_to_seller, now)
            }
            Op::ResolveExpiredDispute { order_id, now } => {
                model.resolve_expired_dispute(order_id, now)
            }
            Op::Stake { artisan, token, amount, now } => {
                model.stake(actor(artisan), token_name(token), amount, now)
            }
            Op::Unstake { artisan, token, amount, now } => {
                model.unstake(&actor(artisan), &token_name(token), amount, now).map(|_| ())
            }
            Op::ProposeUpgrade { now } => model.propose_upgrade(now),
            Op::CancelUpgrade { now } => model.cancel_upgrade(now),
            Op::ExecuteUpgrade { now } => model.execute_upgrade(now),
            Op::SetPaused { paused } => {
                model.set_paused(paused);
                Ok(())
            }
            Op::OnboardUser { address, role } => model.onboard_user(actor(address), role),
            Op::VerifyUser { address } => model.verify_user(&actor(address)),
            Op::DeactivateProfile { address } => model.deactivate_profile(&actor(address)),
            Op::ReactivateProfile { address } => model.reactivate_profile(&actor(address)),
        }
    }

    fn check_invariants(model: &ModelState) -> Result<(), String> {
        if !model.check_locked_non_negative() {
            return Err(alloc::format!("locked amount became negative"));
        }
        model.check_fund_conservation()?;
        model.check_no_terminal_re_entry()?;
        model.check_stake_queue_consistency()?;
        Ok(())
    }

    proptest! {
        #[test]
        fn random_sequences_preserve_invariants(seed in prop::collection::vec(any::<u8>(), 0..64)) {
            let ops = generate_ops(&seed);
            let mut model = ModelState::new();
            for op in &ops {
                let before = model.clone();
                let result = run_op(&mut model, op.clone());

                if result.is_err() {
                    prop_assert_eq!(
                        format!("{:?}", model),
                        format!("{:?}", before),
                        "failed op {:?} mutated model",
                        op
                    );
                }

                if let Err(msg) = check_invariants(&model) {
                    prop_assert!(false, "invariant broken after {:?}: {}", op, msg);
                }
            }
        }
    }
}
