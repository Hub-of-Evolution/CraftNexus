#![cfg(all(test, feature = "wasm-differential-tests"))]

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    xdr::ToXdr,
    Address, Env,
};

const CONTRACT_WASM: &[u8] = include_bytes!(env!("WASM_ARTIFACT"));

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    platform_fee_bps: u32,
    admin_revision: u32,
    invalid_fee_error: Option<u32>,
    events_xdr: Vec<String>,
}

#[derive(Debug)]
struct ScenarioResult {
    seed: u64,
    outcome: Outcome,
    cpu_instructions: u64,
    memory_bytes: u64,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn run_scenario(seed: u64, run_wasm: bool) -> ScenarioResult {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger()
        .with_mut(|ledger| ledger.timestamp = 1_711_368_000);

    let contract_id = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let contract_id_ref = Some(&contract_id);
    if run_wasm {
        env.register_contract_wasm(contract_id_ref, CONTRACT_WASM);
    } else {
        env.register_contract(contract_id_ref, CraftNexusContract);
    }
    let client = CraftNexusContractClient::new(&env, &contract_id);

    env.budget().reset_default();
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    let platform_fee_bps = (seed % 1_001) as u32;
    client.update_platform_fee(&platform_fee_bps);
    let invalid_fee_error = match client.try_update_platform_fee(&1_001) {
        Err(Ok(error)) => Some(error as u32),
        _ => None,
    };
    let cpu_instructions = env.budget().cpu_instruction_cost();
    let memory_bytes = env.budget().memory_bytes_cost();
    env.budget().reset_unlimited();

    let events_xdr = env
        .events()
        .all()
        .iter()
        .map(|(contract, topics, data)| {
            let mut bytes = contract.to_xdr(&env).to_alloc_vec();
            bytes.extend(topics.to_xdr(&env).to_alloc_vec());
            bytes.extend(data.to_xdr(&env).to_alloc_vec());
            hex(&bytes)
        })
        .collect();

    ScenarioResult {
        seed,
        outcome: Outcome {
            platform_fee_bps: client.get_platform_fee(),
            admin_revision: client.get_admin_revision(),
            invalid_fee_error,
            events_xdr,
        },
        cpu_instructions,
        memory_bytes,
    }
}

fn events_json(events: &[String]) -> String {
    format!(
        "[{}]",
        events
            .iter()
            .map(|event| format!("\"{event}\""))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn result_json(result: &ScenarioResult) -> String {
    format!(
        "{{\"seed\":\"0x{:016X}\",\"platform_fee_bps\":{},\"admin_revision\":{},\"invalid_fee_error\":{},\"events_xdr\":{},\"cpu_instructions\":{},\"memory_bytes\":{}}}",
        result.seed,
        result.outcome.platform_fee_bps,
        result.outcome.admin_revision,
        result
            .outcome
            .invalid_fee_error
            .map_or_else(|| "null".to_string(), |code| code.to_string()),
        events_json(&result.outcome.events_xdr),
        result.cpu_instructions,
        result.memory_bytes,
    )
}

fn write_report(seed: u64, results: &[(ScenarioResult, ScenarioResult)]) {
    let scenarios = results
        .iter()
        .map(|(native, wasm)| {
            format!(
                "{{\"native\":{},\"wasm\":{}}}",
                result_json(native),
                result_json(wasm)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let report = format!("{{\n  \"seed\":\"0x{seed:016X}\",\n  \"scenarios\":[{scenarios}]\n}}\n");
    let report_path = std::env::var("DIFFERENTIAL_REPORT")
        .unwrap_or_else(|_| "target/native-wasm-differential.json".to_string());
    if let Some(parent) = std::path::Path::new(&report_path).parent() {
        std::fs::create_dir_all(parent).expect("create differential report directory");
    }
    std::fs::write(report_path, report).expect("write differential test report");
}

#[test]
fn native_and_wasm_match_generated_scenarios() {
    let seed = std::env::var("PROP_SEED").unwrap_or_else(|_| "0xCAFEF00DDEADBEEF".to_string());
    let seed = u64::from_str_radix(seed.trim_start_matches("0x"), 16)
        .expect("PROP_SEED must be a hexadecimal u64");

    let results = (0..3)
        .map(|offset| {
            let scenario_seed = seed.wrapping_add(offset);
            (
                run_scenario(scenario_seed, false),
                run_scenario(scenario_seed, true),
            )
        })
        .collect::<Vec<_>>();

    write_report(seed, &results);

    for (native, wasm) in results {
        assert_eq!(
            native.outcome, wasm.outcome,
            "native/WASM mismatch for seed 0x{:016X}; see target/native-wasm-differential.json",
            native.seed
        );
        assert_eq!(
            native.outcome.invalid_fee_error,
            Some(Error::InvalidFee as u32)
        );
        assert_eq!(
            wasm.outcome.invalid_fee_error,
            Some(Error::InvalidFee as u32)
        );
        assert!(native.cpu_instructions > 0 && native.memory_bytes > 0);
        assert!(wasm.cpu_instructions > 0 && wasm.memory_bytes > 0);
    }
}
