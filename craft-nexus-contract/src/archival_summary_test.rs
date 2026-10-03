#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn get_archival_summary_handles_missing_and_terminal_record() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    assert_eq!(client.get_archival_summary(&42), None);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let summary = ArchivalSummary {
        order_id: 42,
        escrow: Escrow {
            version: 1,
            id: 42,
            batch_id: None,
            buyer,
            seller,
            token,
            amount: 100,
            status: EscrowStatus::Released,
            release_window: 0,
            created_at: 0,
            ipfs_hash: None,
            metadata_hash: None,
            dispute_reason: None,
            dispute_initiated_at: None,
            funded: true,
            funding_deadline: None,
            service_agreement_hash: None,
        },
        finalized_at: 1,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ArchivalSummary(42), &summary);
    });

    assert_eq!(client.get_archival_summary(&42), Some(summary));
}
