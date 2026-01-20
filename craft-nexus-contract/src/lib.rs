#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Env, Symbol, Vec,
    token, Map, BytesN,
};

const ESCROW: Symbol = symbol_short!("ESCROW");
const BUYER: Symbol = symbol_short!("BUYER");
const SELLER: Symbol = symbol_short!("SELLER");
const TOKEN: Symbol = symbol_short!("TOKEN");
const AMOUNT: Symbol = symbol_short!("AMOUNT");
const STATUS: Symbol = symbol_short!("STATUS");
const CREATED_AT: Symbol = symbol_short!("CREATED");
const RELEASE_WINDOW: Symbol = symbol_short!("WINDOW");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Pending = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub release_window: u64, // Time in seconds before auto-release
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
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
        release_window: Option<u64>,
    ) -> Escrow {
        buyer.require_auth();
        
        // Default to 7 days if not specified
        let window = release_window.unwrap_or(604800u64);
        let created_at = env.ledger().timestamp();

        let escrow = Escrow {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Pending,
            created_at,
            release_window: window,
        };

        // Store escrow by order_id
        env.storage()
            .persistent()
            .set(&(ESCROW, order_id), &escrow);

        // Transfer funds from buyer to contract
        let client = token::Client::new(&env, &token);
        client.transfer(&buyer, &env.current_contract_address(), &amount);

        escrow
    }

    /// Release funds to seller (buyer confirms delivery)
    /// 
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn release_funds(env: Env, order_id: u32) {
        let buyer: Address = env.invoker();
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found");

        // Only buyer can release funds
        assert!(
            escrow.buyer == buyer,
            "Only buyer can release funds"
        );
        
        assert!(
            escrow.status == EscrowStatus::Pending,
            "Escrow already processed"
        );

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage()
            .persistent()
            .set(&(ESCROW, order_id), &escrow);

        // Transfer funds to seller
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(&env.current_contract_address(), &escrow.seller, &escrow.amount);
    }

    /// Auto-release funds after release window (seller can call)
    /// 
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn auto_release(env: Env, order_id: u32) {
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found");

        assert!(
            escrow.status == EscrowStatus::Pending,
            "Escrow already processed"
        );

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - escrow.created_at;

        assert!(
            elapsed >= escrow.release_window,
            "Release window not yet elapsed"
        );

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage()
            .persistent()
            .set(&(ESCROW, order_id), &escrow);

        // Transfer funds to seller
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(&env.current_contract_address(), &escrow.seller, &escrow.amount);
    }

    /// Refund funds to buyer (for disputes or cancellations)
    /// 
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `authorized_address` - Address authorized to refund (platform or buyer)
    pub fn refund(env: Env, order_id: u32, authorized_address: Address) {
        authorized_address.require_auth();
        
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found");

        // Only buyer or platform can refund
        assert!(
            escrow.buyer == authorized_address || 
            authorized_address == env.current_contract_address(), // Platform check
            "Not authorized to refund"
        );

        assert!(
            escrow.status == EscrowStatus::Pending,
            "Escrow already processed"
        );

        // Update status
        escrow.status = EscrowStatus::Refunded;
        env.storage()
            .persistent()
            .set(&(ESCROW, order_id), &escrow);

        // Refund to buyer
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(&env.current_contract_address(), &escrow.buyer, &escrow.amount);
    }

    /// Get escrow details
    /// 
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn get_escrow(env: Env, order_id: u32) -> Escrow {
        env.storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found")
    }

    /// Check if escrow can be auto-released
    /// 
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn can_auto_release(env: Env, order_id: u32) -> bool {
        let escrow: Escrow = env
            .storage()
            .persistent()
            .get(&(ESCROW, order_id))
            .expect("Escrow not found");

        if escrow.status != EscrowStatus::Pending {
            return false;
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - escrow.created_at;

        elapsed >= escrow.release_window
    }
}
