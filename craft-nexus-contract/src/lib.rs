#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Bytes,
    BytesN, Env, String, Symbol,
};

mod test;
// Onboarding is a separate logical contract; only one `#[contract]` may be linked per WASM
// artifact. Keep it in this crate for host tests (`cargo test`) but omit from guest builds.
#[cfg(not(target_family = "wasm"))]
pub mod onboarding;

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    /// Unauthorized operation
    Unauthorized = 1,
    /// Escrow not found
    EscrowNotFound = 2,
    /// Invalid escrow state for operation
    InvalidEscrowState = 3,
    /// Username already exists
    UsernameAlreadyExists = 4,
    /// Token not whitelisted
    TokenNotWhitelisted = 5,
    /// Amount below minimum
    AmountBelowMinimum = 6,
    /// Release window too long
    ReleaseWindowTooLong = 7,
    /// Not in dispute state
    NotInDispute = 8,
    /// User already onboarded
    AlreadyOnboarded = 9,
    /// Invalid fee amount
    InvalidFee = 10,
    /// Buyer and seller cannot be the same
    SameBuyerSeller = 11,
    /// Platform not initialized
    PlatformNotInitialized = 12,
    /// Release window not yet elapsed
    ReleaseWindowNotElapsed = 13,
    /// Batch operation error
    BatchOperationFailed = 14,
    /// Contract is paused
    ContractPaused = 15,
}

const ESCROW: Symbol = symbol_short!("ESCROW");
const PLATFORM_FEE: Symbol = symbol_short!("PLAT_FEE");
const PLATFORM_WALLET: Symbol = symbol_short!("PLAT_WAL");
const TOTAL_FEES: Symbol = symbol_short!("TOT_FEES");
const ADMIN: Symbol = symbol_short!("ADMIN");

/// Default platform fee in basis points (500 = 5%)
const DEFAULT_PLATFORM_FEE_BPS: u32 = 500;
/// Maximum platform fee in basis points (10000 = 100%)
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10% max
/// Current version of the contract for upgradeability
const CURRENT_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Escrow(u32),
    BuyerEscrows(Address),
    SellerEscrows(Address),
    MinEscrowAmount(Address),
    ContractVersion,
    /// Custom fee tier for an artisan (basis points)
    ArtisanFeeTier(Address),
    /// Referral reward percentage in basis points
    ReferralRewardBps,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Resolved = 4,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    ReleaseToSeller = 0,
    RefundToBuyer = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32, // Time in seconds before auto-release
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreatedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub release_window: u32,
    pub timestamp: u64,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundsReleasedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundsRefundedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowDisputedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub dispute_reason: String,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowResolvedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub resolution: Resolution,
    pub timestamp: u64,
}

/// Event emitted for each successful escrow in a batch creation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchEscrowCreatedEvent {
    pub batch_id: u64,
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

/// Event emitted for each successful release in a batch operation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchFundsReleasedEvent {
    pub batch_id: u64,
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowMetadata {
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
}

/// Parameters for batch escrow creation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreateParams {
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub order_id: u32,
    pub release_window: Option<u32>,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
}

/// Platform configuration data
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformConfig {
    pub platform_fee_bps: u32,    // Platform fee in basis points (500 = 5%)
    pub platform_wallet: Address, // Wallet address to receive fees
    pub admin: Address,           // Admin address for management
    pub arbitrator: Address,      // Arbitrator for dispute resolution
    pub is_paused: bool,          // Circuit breaker (#96)
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Validate IPFS CID format (v0 and v1 with multibase prefixes).
    ///
    /// Supports:
    /// - CIDv0: 46-char Base58btc starting with "Qm"
    /// - CIDv1 base32lower (prefix 'b'): lowercase a-z + 2-7
    /// - CIDv1 base16lower (prefix 'f'): lowercase hex 0-9 + a-f
    /// - CIDv1 base58btc  (prefix 'z'): Base58 alphabet
    fn validate_ipfs_cid(cid: &String) -> bool {
        let len = cid.len() as usize;
        if len == 0 || len > 128 {
            return false;
        }

        let mut buf = [0u8; 128];
        cid.copy_into_slice(&mut buf[0..len]);
        let cid_bytes = &buf[0..len];

        // CIDv0: exactly 46 chars, starts with "Qm", Base58btc alphabet
        let is_v0 = len == 46
            && cid_bytes[0] == b'Q'
            && cid_bytes[1] == b'm'
            && cid_bytes.iter().all(|b| {
                matches!(
                    *b,
                    b'1'..=b'9'
                        | b'A'..=b'H'
                        | b'J'..=b'N'
                        | b'P'..=b'Z'
                        | b'a'..=b'k'
                        | b'm'..=b'z'
                )
            });

        if is_v0 {
            return true;
        }

        // CIDv1: minimum 3 chars (multibase prefix + version byte + codec)
        if len < 3 {
            return false;
        }

        let prefix = cid_bytes[0];
        let payload = &cid_bytes[1..];

        match prefix {
            // base32lower (most common CIDv1 encoding)
            b'b' => payload
                .iter()
                .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7')),
            // base16lower (hex)
            b'f' => payload
                .iter()
                .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f')),
            // base58btc
            b'z' => payload.iter().all(|b| {
                matches!(
                    *b,
                    b'1'..=b'9'
                        | b'A'..=b'H'
                        | b'J'..=b'N'
                        | b'P'..=b'Z'
                        | b'a'..=b'k'
                        | b'm'..=b'z'
                )
            }),
            _ => false,
        }
    }

    fn validate_optional_ipfs_hash(ipfs_hash: &Option<String>) {
        if let Some(cid) = ipfs_hash {
            assert!(Self::validate_ipfs_cid(cid), "Invalid IPFS CID");
        }
    }

    fn validate_optional_metadata_hash(metadata_hash: &Option<Bytes>) {
        if let Some(hash) = metadata_hash {
            assert!(hash.len() == 32, "metadata_hash must be 32 bytes");
        }
    }

    fn get_admin(env: &Env) -> Result<Address, Error> {
        env.storage()
            .persistent()
            .get(&ADMIN)
            .ok_or(Error::PlatformNotInitialized)
    }

    fn emit_escrow_created(env: &Env, event: EscrowCreatedEvent) {
        env.events()
            .publish((Symbol::new(env, "escrow_created"), event.escrow_id), event);
    }

    fn emit_funds_released(env: &Env, event: FundsReleasedEvent) {
        env.events()
            .publish((Symbol::new(env, "funds_released"), event.escrow_id), event);
    }

    fn emit_funds_refunded(env: &Env, event: FundsRefundedEvent) {
        env.events()
            .publish((Symbol::new(env, "funds_refunded"), event.escrow_id), event);
    }

    fn emit_escrow_disputed(env: &Env, event: EscrowDisputedEvent) {
        env.events().publish(
            (Symbol::new(env, "escrow_disputed"), event.escrow_id),
            event,
        );
    }

    fn emit_escrow_resolved(env: &Env, event: EscrowResolvedEvent) {
        env.events().publish(
            (Symbol::new(env, "escrow_resolved"), event.escrow_id),
            event,
        );
    }

    fn emit_batch_escrow_created(env: &Env, event: BatchEscrowCreatedEvent) {
        env.events().publish(
            (Symbol::new(env, "batch_escrow_created"), event.batch_id),
            event,
        );
    }

    fn emit_batch_funds_released(env: &Env, event: BatchFundsReleasedEvent) {
        env.events().publish(
            (Symbol::new(env, "batch_funds_released"), event.batch_id),
            event,
        );
    }

    pub fn check_min_amount(env: &Env, token: Address, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        let min_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MinEscrowAmount(token))
            .unwrap_or(0); // If not set, allow any positive amount

        if amount < min_amount {
            return Err(Error::AmountBelowMinimum);
        }

        Ok(())
    }

    /// Initialize the contract with platform configuration
    ///
    /// # Arguments
    /// * `platform_wallet` - Address that will receive platform fees
    /// * `admin` - Admin address for managing platform settings
    /// * `platform_fee_bps` - Platform fee in basis points (default 500 = 5%)
    pub fn initialize(
        env: Env,
        platform_wallet: Address,
        admin: Address,
        arbitrator: Address,
        platform_fee_bps: u32,
    ) {
        admin.require_auth();

        // Validate fee is within bounds
        assert!(platform_fee_bps <= MAX_PLATFORM_FEE_BPS, "Fee too high");

        if !(platform_fee_bps <= MAX_PLATFORM_FEE_BPS) {
            env.panic_with_error(Error::InvalidFee);
        }

        let config = PlatformConfig {
            platform_fee_bps,
            platform_wallet: platform_wallet.clone(),
            admin: admin.clone(),
            arbitrator: arbitrator.clone(),
            is_paused: false,
        };

        env.storage().persistent().set(&PLATFORM_FEE, &config);
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_FEE, 1000, 518400);
        env.storage()
            .persistent()
            .set(&PLATFORM_WALLET, &platform_wallet);
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_WALLET, 1000, 518400);
        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().extend_ttl(&ADMIN, 1000, 518400);

        // Initialize total fees to 0
        let zero: i128 = 0;
        env.storage().persistent().set(&TOTAL_FEES, &zero);
        env.storage()
            .persistent()
            .extend_ttl(&TOTAL_FEES, 1000, 518400);

        // Initialize contract version to 1
        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &1u32);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ContractVersion, 1000, 518400);
    }
    /// Create a new escrow for an order
    ///
    /// # Arguments
    /// * `buyer` - Address of the buyer
    /// * `seller` - Address of the seller
    /// * `token` - Token contract address (USDC)
    /// * `amount` - Amount to escrow
    /// * `order_id` - Unique order identifier
    /// * `release_window` - Time in seconds before auto-release (default 7 days = 604800)
    pub fn create_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        release_window: Option<u32>,
    ) -> Escrow {
        Self::create_escrow_with_metadata(
            env,
            buyer,
            seller,
            token,
            amount,
            order_id,
            release_window,
            None,
            None,
        )
    }

    /// Create a new escrow for an order and attach off-chain metadata.
    pub fn create_escrow_with_metadata(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        release_window: Option<u32>,
        ipfs_hash: Option<String>,
        metadata_hash: Option<Bytes>,
    ) -> Escrow {
        Self::check_not_paused(&env);
        buyer.require_auth();

        // Validate amount is positive and above minimum
        if let Err(e) = Self::check_min_amount(&env, token.clone(), amount) {
            env.panic_with_error(e);
        }

        // Validate buyer and seller are different
        if !(buyer != seller) {
            env.panic_with_error(Error::SameBuyerSeller);
        }

        // Default to 7 days if not specified
        let window = release_window.unwrap_or(604800u32);
        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(&ipfs_hash);
        Self::validate_optional_metadata_hash(&metadata_hash);

        let escrow = Escrow {
            id: order_id as u64,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Active,
            release_window: window,
            created_at,
            ipfs_hash: ipfs_hash.clone(),
            metadata_hash: metadata_hash.clone(),
            dispute_reason: None,
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);

        // Update buyer's escrow list for indexing
        let buyer_key = DataKey::BuyerEscrows(buyer.clone());
        let mut buyer_escrows: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&buyer_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        buyer_escrows.push_back(order_id as u64);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&buyer_key, 1000, 518400);

        // Update seller's escrow list for indexing
        let seller_key = DataKey::SellerEscrows(seller.clone());
        let mut seller_escrows: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&seller_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        seller_escrows.push_back(order_id as u64);
        env.storage().persistent().set(&seller_key, &seller_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&seller_key, 1000, 518400);

        // Transfer funds from buyer to contract
        let client = token::Client::new(&env, &token);
        client.transfer(&buyer, &env.current_contract_address(), &amount);

        let event = EscrowCreatedEvent {
            escrow_id: order_id as u64,
            buyer: buyer.clone(),
            seller: seller.clone(),
            amount,
            token: token.clone(),
            release_window: window,
            timestamp: env.ledger().timestamp(),
            ipfs_hash,
            metadata_hash,
        };
        Self::emit_escrow_created(&env, event);

        escrow
    }

    /// Get escrows for a specific buyer with pagination.
    pub fn get_escrows_by_buyer(
        env: Env,
        buyer: Address,
        page: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let key = DataKey::BuyerEscrows(buyer);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(&key, 1000, 518400);
        }

        let start = page * limit;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(soroban_sdk::Vec::new(&env));
        }

        let end = (start + limit).min(len);
        Ok(escrow_ids.slice(start..end))
    }

    /// Get escrows for a specific seller with pagination.
    pub fn get_escrows_by_seller(
        env: Env,
        seller: Address,
        page: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let key = DataKey::SellerEscrows(seller);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(&key, 1000, 518400);
        }

        let start = page * limit;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(soroban_sdk::Vec::new(&env));
        }

        let end = (start + limit).min(len);
        Ok(escrow_ids.slice(start..end))
    }

    /// Get platform configuration
    fn get_platform_config(env: &Env) -> PlatformConfig {
        let config = env.storage().persistent().get(&PLATFORM_FEE);
        if !(config.is_some()) {
            env.panic_with_error(Error::PlatformNotInitialized);
        }
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_FEE, 1000, 518400);
        config.unwrap()
    }

    fn get_arbitrator(env: &Env) -> Address {
        Self::get_platform_config(env).arbitrator
    }

    /// Calculate platform fee for a given amount
    fn calculate_fee(amount: i128, fee_bps: u32) -> i128 {
        (amount * (fee_bps as i128)) / 10000
    }

    /// Release funds to seller with platform fee deduction
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn release_funds(env: Env, order_id: u32) {
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if !(escrow_opt.is_some()) {
            env.panic_with_error(Error::EscrowNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);
        let mut escrow: Escrow = escrow_opt.unwrap();

        // Only buyer can release funds
        escrow.buyer.require_auth();

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(Error::InvalidEscrowState);
        }

        // Get platform config
        let config = Self::get_platform_config(&env);

        // Calculate platform fee
        let fee_amount = Self::calculate_fee(escrow.amount, config.platform_fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Transfer platform fee to platform wallet
        let token_client = token::Client::new(&env, &escrow.token);
        if fee_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &config.platform_wallet,
                &fee_amount,
            );

            // Update total fees collected
            let mut total_fees: i128 = env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0);
            total_fees += fee_amount;
            env.storage().persistent().set(&TOTAL_FEES, &total_fees);
        }

        // Transfer remaining funds to seller
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );

        Self::emit_funds_released(
            &env,
            FundsReleasedEvent {
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Auto-release funds after release window (seller can call)
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn auto_release(env: Env, order_id: u32) {
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if !(escrow_opt.is_some()) {
            env.panic_with_error(Error::EscrowNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);
        let mut escrow: Escrow = escrow_opt.unwrap();

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - (escrow.created_at as u64);

        if !(elapsed >= escrow.release_window as u64) {
            env.panic_with_error(Error::ReleaseWindowNotElapsed);
        }

        // Get platform config
        let config = Self::get_platform_config(&env);

        // Calculate platform fee
        let fee_amount = Self::calculate_fee(escrow.amount, config.platform_fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Transfer platform fee to platform wallet
        let token_client = token::Client::new(&env, &escrow.token);
        if fee_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &config.platform_wallet,
                &fee_amount,
            );

            // Update total fees collected
            let mut total_fees: i128 = env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0);
            total_fees += fee_amount;
            env.storage().persistent().set(&TOTAL_FEES, &total_fees);
            env.storage()
                .persistent()
                .extend_ttl(&TOTAL_FEES, 1000, 518400);
        }

        // Transfer remaining funds to seller
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );

        Self::emit_funds_released(
            &env,
            FundsReleasedEvent {
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Upgrade the contract's WASM code (admin only)
    ///
    /// # Arguments
    /// * `new_wasm_hash` - The SHA-256 hash of the new WASM code
    pub fn update_wasm(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);

        // Update version in storage
        let current_version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &(current_version + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ContractVersion, 1000, 518400);

        Ok(())
    }

    pub fn get_version(env: Env) -> u32 {
        let version = env
            .storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0);
        if env.storage().persistent().has(&DataKey::ContractVersion) {
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::ContractVersion, 1000, 518400);
        }
        version
    }

    /// Refund funds to buyer (admin only)
    ///
    /// # Arguments
    /// * `escrow_id` - Escrow/Order identifier
    pub fn refund(env: Env, escrow_id: u64) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let order_id = escrow_id as u32;
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let mut escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Active {
            return Err(Error::InvalidEscrowState);
        }

        // Update status
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Refund to buyer
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        Self::emit_funds_refunded(
            &env,
            FundsRefundedEvent {
                escrow_id,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Ok(())
    }

    fn release_funds_to_seller(env: &Env, escrow: &Escrow) {
        let config = Self::get_platform_config(env);
        let fee_amount = Self::calculate_fee(escrow.amount, config.platform_fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        let token_client = token::Client::new(env, &escrow.token);
        if fee_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &config.platform_wallet,
                &fee_amount,
            );
            let mut total_fees: i128 = env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0);
            total_fees += fee_amount;
            env.storage().persistent().set(&TOTAL_FEES, &total_fees);
        }

        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );
    }

    fn refund_funds_to_buyer(env: &Env, escrow: &Escrow) {
        let token_client = token::Client::new(env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );
    }

    /// Get escrow details
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn get_escrow(env: Env, order_id: u32) -> Escrow {
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if !(escrow_opt.is_some()) {
            env.panic_with_error(Error::EscrowNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);
        escrow_opt.unwrap()
    }

    /// Get escrow metadata fields only.
    pub fn get_escrow_metadata(env: Env, order_id: u32) -> EscrowMetadata {
        let escrow = Self::get_escrow(env, order_id);
        EscrowMetadata {
            ipfs_hash: escrow.ipfs_hash,
            metadata_hash: escrow.metadata_hash,
        }
    }

    /// Check if escrow can be auto-released
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn can_auto_release(env: Env, order_id: u32) -> bool {
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if !(escrow_opt.is_some()) {
            env.panic_with_error(Error::EscrowNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);
        let escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Active {
            return false;
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - (escrow.created_at as u64);

        elapsed >= escrow.release_window as u64
    }

    /// Dispute an escrow
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `dispute_reason` - Reason for dispute
    /// * `authorized_address` - Address authorized to dispute (buyer or seller)
    pub fn dispute_escrow(
        env: Env,
        order_id: u32,
        dispute_reason: String,
        authorized_address: Address,
    ) {
        authorized_address.require_auth();

        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if !(escrow_opt.is_some()) {
            env.panic_with_error(Error::EscrowNotFound);
        }
        env.storage()
            .persistent()
            .extend_ttl(&(ESCROW, order_id), 1000, 518400);
        let mut escrow: Escrow = escrow_opt.unwrap();

        // Allow buyer or seller to dispute
        if !(escrow.buyer == authorized_address || escrow.seller == authorized_address) {
            env.panic_with_error(Error::Unauthorized);
        }

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(Error::InvalidEscrowState);
        }

        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = Some(dispute_reason.clone());
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        Self::emit_escrow_disputed(
            &env,
            EscrowDisputedEvent {
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                dispute_reason,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Resolve disputed escrow (arbitrator only)
    pub fn resolve_dispute(env: Env, order_id: u32, resolution: Resolution) {
        let arbitrator = Self::get_arbitrator(&env);
        arbitrator.require_auth();

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found");

        assert!(
            escrow.status == EscrowStatus::Disputed,
            "Escrow not in dispute"
        );

        match resolution {
            Resolution::ReleaseToSeller => {
                Self::release_funds_to_seller(&env, &escrow);
            }
            Resolution::RefundToBuyer => {
                Self::refund_funds_to_buyer(&env, &escrow);
            }
        }

        escrow.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        Self::emit_escrow_resolved(
            &env,
            EscrowResolvedEvent {
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                resolution,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Update platform fee percentage (admin only)
    ///
    /// # Arguments
    /// * `new_fee_bps` - New fee in basis points
    pub fn update_platform_fee(env: Env, new_fee_bps: u32) {
        let config = Self::get_platform_config(&env);
        config.admin.require_auth();

        if !(new_fee_bps <= MAX_PLATFORM_FEE_BPS) {
            env.panic_with_error(Error::InvalidFee);
        }

        let new_config = PlatformConfig {
            platform_fee_bps: new_fee_bps,
            platform_wallet: config.platform_wallet,
            admin: config.admin,
            arbitrator: config.arbitrator,
            is_paused: config.is_paused,
        };

        env.storage().persistent().set(&PLATFORM_FEE, &new_config);
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_FEE, 1000, 518400);
    }

    /// Update platform wallet address (admin only)
    ///
    /// # Arguments
    /// * `new_wallet` - New platform wallet address
    pub fn update_platform_wallet(env: Env, new_wallet: Address) {
        let config = Self::get_platform_config(&env);
        config.admin.require_auth();

        let new_config = PlatformConfig {
            platform_fee_bps: config.platform_fee_bps,
            platform_wallet: new_wallet,
            admin: config.admin,
            arbitrator: config.arbitrator,
            is_paused: config.is_paused,
        };

        env.storage().persistent().set(&PLATFORM_FEE, &new_config);
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_FEE, 1000, 518400);
    }

    /// Set the minimum escrow amount for a specific token (admin only)
    ///
    /// # Arguments
    /// * `token` - Token address
    /// * `min_amount` - Minimum amount in smallest unit
    pub fn set_min_escrow_amount(env: Env, token: Address, min_amount: i128) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::MinEscrowAmount(token), &min_amount);
        Ok(())
    }

    /// Get current platform fee percentage
    pub fn get_platform_fee(env: Env) -> u32 {
        let config = Self::get_platform_config(&env);
        config.platform_fee_bps
    }

    /// Get platform wallet address
    pub fn get_platform_wallet(env: Env) -> Address {
        let config = Self::get_platform_config(&env);
        config.platform_wallet
    }

    /// Get total fees collected by platform
    pub fn get_total_fees_collected(env: Env) -> i128 {
        let fees = env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0);
        if env.storage().persistent().has(&TOTAL_FEES) {
            env.storage()
                .persistent()
                .extend_ttl(&TOTAL_FEES, 1000, 518400);
        }
        fees
    }

    /// Calculate the fee for a given amount (for display purposes)
    ///
    /// # Arguments
    /// * `amount` - The escrow amount
    pub fn calculate_fee_for_amount(env: Env, amount: i128) -> i128 {
        let config = Self::get_platform_config(&env);
        Self::calculate_fee(amount, config.platform_fee_bps)
    }

    /// Calculate net amount seller will receive
    ///
    /// # Arguments
    /// * `amount` - The escrow amount
    pub fn calculate_seller_net_amount(env: Env, amount: i128) -> i128 {
        let fee = Self::calculate_fee_for_amount(env, amount);
        amount - fee
    }

    /// Validate escrow parameters for batch creation
    fn validate_escrow_params(env: &Env, params: &EscrowCreateParams) -> Result<(), Error> {
        // Validate amount is positive
        if params.amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        // Check minimum amount
        if let Err(e) = Self::check_min_amount(env, params.token.clone(), params.amount) {
            return Err(e);
        }

        // Validate buyer and seller are different
        if params.buyer == params.seller {
            return Err(Error::SameBuyerSeller);
        }

        // Validate IPFS hash if provided
        if let Some(ref ipfs) = params.ipfs_hash {
            if !Self::validate_ipfs_cid(ipfs) {
                return Err(Error::InvalidFee); // Use invalid fee as proxy for invalid CID
            }
        }

        // Validate metadata hash if provided
        if let Some(ref hash) = params.metadata_hash {
            if hash.len() != 32 {
                return Err(Error::InvalidFee); // Use invalid fee as proxy for invalid hash
            }
        }

        Ok(())
    }

    /// Create a single escrow from parameters (internal helper)
    fn create_single_escrow(env: &Env, params: EscrowCreateParams) -> Result<u64, Error> {
        // Validate first
        Self::validate_escrow_params(env, &params)?;

        // Default to 7 days if not specified
        let window = params.release_window.unwrap_or(604800u32);
        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;

        // Validate metadata
        Self::validate_optional_ipfs_hash(&params.ipfs_hash);
        Self::validate_optional_metadata_hash(&params.metadata_hash);

        let escrow = Escrow {
            id: params.order_id as u64,
            buyer: params.buyer.clone(),
            seller: params.seller.clone(),
            token: params.token.clone(),
            amount: params.amount,
            status: EscrowStatus::Active,
            release_window: window,
            created_at,
            ipfs_hash: params.ipfs_hash.clone(),
            metadata_hash: params.metadata_hash.clone(),
            dispute_reason: None,
        };

        env.storage()
            .persistent()
            .set(&(ESCROW, params.order_id), &escrow);

        // Update buyer's escrow list for indexing
        let buyer_key = DataKey::BuyerEscrows(params.buyer.clone());
        let mut buyer_escrows: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&buyer_key)
            .unwrap_or(soroban_sdk::Vec::new(env));
        buyer_escrows.push_back(params.order_id as u64);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);

        // Update seller's escrow list for indexing
        let seller_key = DataKey::SellerEscrows(params.seller.clone());
        let mut seller_escrows: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&seller_key)
            .unwrap_or(soroban_sdk::Vec::new(env));
        seller_escrows.push_back(params.order_id as u64);
        env.storage().persistent().set(&seller_key, &seller_escrows);

        // Transfer funds from buyer to contract
        let client = token::Client::new(env, &params.token);
        client.transfer(
            &params.buyer,
            &env.current_contract_address(),
            &params.amount,
        );

        // Emit individual event
        let event = EscrowCreatedEvent {
            escrow_id: params.order_id as u64,
            buyer: params.buyer.clone(),
            seller: params.seller.clone(),
            amount: params.amount,
            token: params.token.clone(),
            release_window: window,
            timestamp: env.ledger().timestamp(),
            ipfs_hash: params.ipfs_hash,
            metadata_hash: params.metadata_hash,
        };
        Self::emit_escrow_created(env, event);

        Ok(params.order_id as u64)
    }

    /// Create multiple escrows in a batch operation
    ///
    /// Validates all escrows first before processing any to ensure atomic behavior.
    ///
    /// # Arguments
    /// * `escrows` - Vector of escrow creation parameters
    /// * `batch_id` - Unique identifier for this batch operation
    pub fn create_batch_escrow(
        env: Env,
        batch_id: u64,
        escrows: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let mut results = soroban_sdk::Vec::new(&env);

        // Collect all params first for validation
        let mut params_list: soroban_sdk::Vec<EscrowCreateParams> = soroban_sdk::Vec::new(&env);
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                params_list.push_back(params);
            }
        }

        // Validate all first
        for i in 0..params_list.len() {
            if let Some(params) = params_list.get(i) {
                Self::validate_escrow_params(&env, &params)?;
            }
        }

        // Then create all - buyer must authorize for each
        // Since the batch needs buyer auth, we require the first buyer to authorize
        // and use their auth for all escrows (in practice, each escrow would have its own auth)
        for i in 0..params_list.len() {
            if let Some(params) = params_list.get(i) {
                // Require auth from the buyer of the first escrow
                if i == 0 {
                    params.buyer.require_auth();
                }

                // Validate again to ensure still valid
                Self::validate_escrow_params(&env, &params)?;

                match Self::create_single_escrow(&env, params) {
                    Ok(id) => {
                        // Emit batch event
                        let escrow_opt: Option<Escrow> =
                            env.storage().persistent().get(&(ESCROW, id as u32));
                        if let Some(escrow) = escrow_opt {
                            Self::emit_batch_escrow_created(
                                &env,
                                BatchEscrowCreatedEvent {
                                    batch_id,
                                    escrow_id: id,
                                    buyer: escrow.buyer,
                                    seller: escrow.seller,
                                    amount: escrow.amount,
                                    token: escrow.token,
                                    timestamp: env.ledger().timestamp(),
                                },
                            );
                        }
                        results.push_back(id);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Release multiple escrows in a batch operation
    ///
    /// Validates all escrows first before processing any.
    ///
    /// # Arguments
    /// * `order_ids` - Vector of order IDs to release
    /// * `batch_id` - Unique identifier for this batch operation
    /// * `authorized_address` - Address releasing the funds (buyer)
    pub fn release_batch_funds(
        env: Env,
        batch_id: u64,
        order_ids: soroban_sdk::Vec<u32>,
        authorized_address: Address,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        authorized_address.require_auth();

        let mut results = soroban_sdk::Vec::new(&env);

        // Validate all escrows first
        for i in 0..order_ids.len() {
            if let Some(order_id) = order_ids.get(i) {
                let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));

                if escrow_opt.is_none() {
                    return Err(Error::EscrowNotFound);
                }

                let escrow: Escrow = escrow_opt.unwrap();

                // Check status
                if escrow.status != EscrowStatus::Active {
                    return Err(Error::InvalidEscrowState);
                }

                // Check authorization (buyer must match)
                if escrow.buyer != authorized_address {
                    return Err(Error::Unauthorized);
                }
            }
        }

        // Then process all releases
        for i in 0..order_ids.len() {
            if let Some(order_id) = order_ids.get(i) {
                let escrow_opt: Option<Escrow> =
                    env.storage().persistent().get(&(ESCROW, order_id));
                if escrow_opt.is_some() {
                    env.storage()
                        .persistent()
                        .extend_ttl(&(ESCROW, order_id), 1000, 518400);
                }

                if let Some(mut escrow) = escrow_opt {
                    // Get platform config
                    let config = Self::get_platform_config(&env);

                    // Calculate platform fee
                    let fee_amount = Self::calculate_fee(escrow.amount, config.platform_fee_bps);
                    let seller_amount = escrow.amount - fee_amount;

                    // Update status
                    escrow.status = EscrowStatus::Released;
                    env.storage().persistent().set(&(ESCROW, order_id), &escrow);

                    // Transfer platform fee to platform wallet
                    let token_client = token::Client::new(&env, &escrow.token);
                    if fee_amount > 0 {
                        token_client.transfer(
                            &env.current_contract_address(),
                            &config.platform_wallet,
                            &fee_amount,
                        );

                        // Update total fees collected
                        let mut total_fees: i128 =
                            env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0);
                        total_fees += fee_amount;
                        env.storage().persistent().set(&TOTAL_FEES, &total_fees);
                    }

                    // Transfer remaining funds to seller
                    token_client.transfer(
                        &env.current_contract_address(),
                        &escrow.seller,
                        &seller_amount,
                    );

                    // Emit individual release event
                    Self::emit_funds_released(
                        &env,
                        FundsReleasedEvent {
                            escrow_id: order_id as u64,
                            buyer: escrow.buyer.clone(),
                            seller: escrow.seller.clone(),
                            amount: escrow.amount,
                            token: escrow.token.clone(),
                            timestamp: env.ledger().timestamp(),
                        },
                    );

                    // Emit batch event
                    Self::emit_batch_funds_released(
                        &env,
                        BatchFundsReleasedEvent {
                            batch_id,
                            escrow_id: order_id as u64,
                            buyer: escrow.buyer.clone(),
                            seller: escrow.seller.clone(),
                            amount: escrow.amount,
                            token: escrow.token.clone(),
                            timestamp: env.ledger().timestamp(),
                        },
                    );

                    results.push_back(order_id as u64);
                }
            }
        }

        Ok(results)
    }

    // ── Circuit Breaker (#96) ───────────────────────────────────────

    /// Check that the contract is not paused. Panics with ContractPaused if it is.
    fn check_not_paused(env: &Env) {
        if let Some(config) = env
            .storage()
            .persistent()
            .get::<Symbol, PlatformConfig>(&PLATFORM_FEE)
        {
            if config.is_paused {
                env.panic_with_error(Error::ContractPaused);
            }
        }
    }

    /// Admin pauses or unpauses the contract.
    pub fn set_paused(env: Env, paused: bool) {
        let admin = Self::get_admin(&env).unwrap();
        admin.require_auth();

        let mut config = Self::get_platform_config(&env);
        config.is_paused = paused;
        env.storage().persistent().set(&PLATFORM_FEE, &config);
        env.storage()
            .persistent()
            .extend_ttl(&PLATFORM_FEE, 1000, 518400);
    }

    /// View: check if contract is paused.
    pub fn is_paused(env: Env) -> bool {
        let config = Self::get_platform_config(&env);
        config.is_paused
    }

    // ── Tiered Artisan Fees (#98) ───────────────────────────────────

    /// Admin assigns a custom fee tier (in basis points) for an artisan.
    pub fn set_artisan_fee_tier(env: Env, artisan: Address, fee_bps: u32) {
        let admin = Self::get_admin(&env).unwrap();
        admin.require_auth();

        assert!(fee_bps <= MAX_PLATFORM_FEE_BPS, "Fee tier too high");

        env.storage()
            .persistent()
            .set(&DataKey::ArtisanFeeTier(artisan), &fee_bps);
    }

    /// Get the effective fee basis points for a seller.
    /// Returns artisan-specific tier if set, otherwise platform default.
    pub fn get_effective_fee_bps(env: Env, seller: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, u32>(&DataKey::ArtisanFeeTier(seller))
            .unwrap_or_else(|| {
                let config = Self::get_platform_config(&env);
                config.platform_fee_bps
            })
    }

    // ── Referral Rewards (#105) ─────────────────────────────────────

    /// Admin sets the referral reward percentage (basis points of the platform fee).
    pub fn set_referral_reward_bps(env: Env, bps: u32) {
        let admin = Self::get_admin(&env).unwrap();
        admin.require_auth();
        assert!(bps <= 5000, "Referral reward cannot exceed 50% of fee");
        env.storage()
            .persistent()
            .set(&DataKey::ReferralRewardBps, &bps);
    }

    /// Get the referral reward basis points.
    pub fn get_referral_reward_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, u32>(&DataKey::ReferralRewardBps)
            .unwrap_or(0)
    }
}
