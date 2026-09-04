#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Cancelled,
    Completed,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RecurringEscrow {
    pub recipient: Address,
    pub token: Address,
    pub balance: i128,
    pub cycle_amount: i128,
    pub status: EscrowStatus,
}

#[contract]
pub struct CraftNexusEscrow;

#[contractimpl]
impl CraftNexusEscrow {
    /// Cancels the recurring escrow and refunds the remaining balance exactly once.
    pub fn cancel_escrow(env: Env, escrow_id: u64, caller: Address) {
        caller.require_auth();

        let mut escrow: RecurringEscrow = env.storage().persistent().get(&escrow_id).expect("Escrow not found");

        if escrow.status == EscrowStatus::Cancelled {
            panic!("Escrow is already cancelled");
        }

        // Make cancellation terminal
        escrow.status = EscrowStatus::Cancelled;
        
        let refund_amount = escrow.balance;
        escrow.balance = 0;

        env.storage().persistent().set(&escrow_id, &escrow);

        // Refund remaining balance exactly once
        if refund_amount > 0 {
            let token_client = token::Client::new(&env, &escrow.token);
            token_client.transfer(&env.current_contract_address(), &caller, &refund_amount);
        }

        env.events().publish((Symbol::new(&env, "escrow_cancelled"), escrow_id), refund_amount);
    }

    /// Releases the next cycle amount to the recipient.
    pub fn release_next_cycle(env: Env, escrow_id: u64) {
        let mut escrow: RecurringEscrow = env.storage().persistent().get(&escrow_id).expect("Escrow not found");

        // 🎯 FIX: Validate status immediately before calculating or transferring the next cycle
        if escrow.status == EscrowStatus::Cancelled {
            panic!("Rejected: Escrow has been cancelled");
        }

        if escrow.balance < escrow.cycle_amount {
            panic!("Insufficient remaining balance for next cycle");
        }

        // Deduct balance and update state BEFORE external token call (prevent reentrancy)
        escrow.balance -= escrow.cycle_amount;
        env.storage().persistent().set(&escrow_id, &escrow);

        // Token call
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&env.current_contract_address(), &escrow.recipient, &escrow.cycle_amount);

        // Release event
        env.events().publish((Symbol::new(&env, "cycle_released"), escrow_id), escrow.cycle_amount);
    }
}