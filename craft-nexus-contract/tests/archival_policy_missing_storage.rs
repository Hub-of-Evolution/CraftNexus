use craft_nexus_contract::{CraftNexusContract, CraftNexusContractClient};
use soroban_sdk::Env;

const DEFAULT_ARCHIVAL_RETENTION_WINDOW: u64 = 90 * 24 * 60 * 60;
const DEFAULT_ARCHIVAL_COMPACTION_BATCH: u32 = 25;

#[test]
fn get_archival_policy_returns_defaults_when_storage_is_missing() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // No archival policy has been written for this contract. The public getter
    // must stay usable for fresh deployments and partially migrated state.
    let policy = client.get_archival_policy();

    assert_eq!(policy.retention_window, DEFAULT_ARCHIVAL_RETENTION_WINDOW);
    assert_eq!(
        policy.compaction_batch_size,
        DEFAULT_ARCHIVAL_COMPACTION_BATCH
    );
}


#[test]
fn get_dispute_final_deadline_missing_order_returns_error() {
    let env = Env::default();
    let contract_id = env.register(CraftNexusContract, ());   // use the contract name your other tests use
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // This order was never created.
    let result = client.try_get_dispute_final_deadline(&9999u32);

    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));       // same variant as Edit 2
}