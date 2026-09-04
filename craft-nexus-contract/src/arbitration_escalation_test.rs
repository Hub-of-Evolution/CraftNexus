#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Bytes, BytesN, Env, IntoVal, String, Symbol,
};

/// Recompute the on-chain `content_digest` for an evidence URI exactly as the
/// contract does (SHA-256 over the first ≤256 payload bytes). Lets tests assert
/// the persisted digest is stable, verifiable metadata (#1077).
fn evidence_digest(env: &Env, uri: &String) -> BytesN<32> {
    let len = (uri.len() as usize).min(256);
    let mut buf = [0u8; 256];
    uri.copy_into_slice(&mut buf[0..len]);
    let bytes = Bytes::from_slice(env, &buf[0..len]);
    env.crypto().sha256(&bytes).into()
}

fn setup(
    env: &Env,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_700_000_000;
    });

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

    (
        client,
        buyer,
        seller,
        token_contract.address(),
        token_admin_client,
        admin,
    )
}

fn create_and_dispute(
    env: &Env,
    client: &CraftNexusContractClient<'static>,
    buyer: &Address,
    seller: &Address,
    token: &Address,
    token_admin: &token::StellarAssetClient<'static>,
    order_id: u32,
) {
    token_admin.mint(buyer, &100_000_000);
    client.create_escrow(buyer, seller, token, &50_000_000, &order_id, &None);
    client.dispute_escrow(&order_id, &Symbol::new(env, "Item_not_as_described"), buyer);
}

// ── Dispute Arbitration Escalation (#941) ──────────────────────────────

#[test]
fn test_escalate_dispute_too_early_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let result = client.try_escalate_dispute(&1, &buyer);
    assert!(result.is_err());
    assert!(client.get_dispute_escalation(&1).is_none());
}

#[test]
fn test_escalate_dispute_succeeds_after_window() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_DISPUTE_ESCALATION_WINDOW as u64 + 1;
    });

    client.escalate_dispute(&1, &buyer);

    let escalation = client
        .get_dispute_escalation(&1)
        .expect("escalation should be recorded");
    assert_eq!(escalation.escalated_by, buyer);
    assert_eq!(escalation.order_id, 1);

    // Auditable: verify the event was emitted.
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "dispute_escalated").into_val(&env),
            1u64.into_val(&env)
        ]
    );
}

#[test]
fn test_escalate_dispute_twice_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_DISPUTE_ESCALATION_WINDOW as u64 + 1;
    });
    client.escalate_dispute(&1, &buyer);

    let result = client.try_escalate_dispute(&1, &buyer);
    assert!(result.is_err());
}

#[test]
fn test_escalate_dispute_unauthorized_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_DISPUTE_ESCALATION_WINDOW as u64 + 1;
    });

    let stranger = Address::generate(&env);
    let result = client.try_escalate_dispute(&1, &stranger);
    assert!(result.is_err());
}

#[test]
fn test_escalate_dispute_not_disputed_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token, &50_000_000, &1, &None);

    let result = client.try_escalate_dispute(&1, &buyer);
    assert!(result.is_err());
}

#[test]
fn test_set_dispute_escalation_window() {
    let env = Env::default();
    let (client, _buyer, _seller, _token, _token_admin, _admin) = setup(&env);

    client.set_dispute_escalation_window(&(24 * 60 * 60));
    assert_eq!(
        client.get_platform_config().dispute_escalation_window,
        24 * 60 * 60
    );
}

#[test]
fn test_arbitrator_rotation_invalidates_active_assignment() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let old_assignment = client
        .get_dispute_assignment(&1)
        .expect("assignment should be persisted");
    assert_eq!(old_assignment.revision, 1);
    assert_eq!(client.get_arbitrator_assign_revision(), 1);

    let replacement = Address::generate(&env);
    client.update_arbitrator(&replacement);
    assert_eq!(client.get_arbitrator_assign_revision(), 2);
    assert_eq!(client.get_platform_config().arbitrator, replacement);

    // Rotation does not alter the escrow's financial context.
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.amount, 50_000_000);
    assert_eq!(escrow.token, token);

    let evidence = client.try_submit_evidence(
        &1,
        &buyer,
        &String::from_str(&env, "ipfs://stale-assignment"),
    );
    assert!(evidence.is_err());

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_DISPUTE_ESCALATION_WINDOW as u64 + 1;
    });
    assert!(client.try_escalate_dispute(&1, &buyer).is_err());
    assert!(client.get_dispute_escalation(&1).is_none());

    // Neither the former nor replacement arbitrator may decide an old revision.
    assert!(client
        .try_resolve_dispute(&1, &Resolution::RefundToBuyer, &old_assignment.arbitrator)
        .is_err());
    assert!(client
        .try_resolve_dispute(&1, &Resolution::RefundToBuyer, &replacement)
        .is_err());
    assert_eq!(client.get_escrow(&1).amount, 50_000_000);

    client.reassign_dispute(&1);
    let reassigned = client.get_dispute_assignment(&1).unwrap();
    assert_eq!(reassigned.arbitrator, replacement);
    assert_eq!(reassigned.revision, 2);
    client.submit_evidence(
        &1,
        &buyer,
        &String::from_str(&env, "ipfs://reassigned-evidence"),
    );
}

// ── Dispute Evidence Challenge Period (#942) ───────────────────────────

#[test]
fn test_submit_evidence_and_counter_evidence() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let evidence_id =
        client.submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://buyer-evidence"));
    assert_eq!(evidence_id, 0);

    let counter_id = client.submit_counter_evidence(
        &1,
        &seller,
        &String::from_str(&env, "ipfs://seller-rebuttal"),
        &evidence_id,
    );
    assert_eq!(counter_id, 1);

    let log = client.get_evidence(&1);
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(0).unwrap().submitter, buyer);
    assert_eq!(log.get(1).unwrap().submitter, seller);
    assert_eq!(log.get(1).unwrap().parent_evidence_id, Some(0));
}

#[test]
fn test_submit_counter_evidence_invalid_parent_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let result = client.try_submit_counter_evidence(
        &1,
        &seller,
        &String::from_str(&env, "ipfs://seller-rebuttal"),
        &999,
    );
    assert!(result.is_err());
}

#[test]
fn test_submit_evidence_unauthorized_fails() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let stranger = Address::generate(&env);
    let result =
        client.try_submit_evidence(&1, &stranger, &String::from_str(&env, "ipfs://not-a-party"));
    assert!(result.is_err());
}

#[test]
fn test_resolve_dispute_blocked_until_challenge_window_elapses() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let result = client.try_resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);
    assert!(result.is_err());

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_EVIDENCE_CHALLENGE_WINDOW as u64 + 1;
    });

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

// ── Rate Limiting for Sensitive Operations (#943) ──────────────────────

#[test]
fn test_dispute_creation_rate_limit_blocks_excess_calls() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    token_admin.mint(&buyer, &1_000_000_000);

    // Default limit is DEFAULT_RATE_LIMIT_MAX_CALLS (5) calls per window.
    for order_id in 1..=DEFAULT_RATE_LIMIT_MAX_CALLS {
        client.create_escrow(&buyer, &seller, &token, &1_000, &order_id, &None);
        client.dispute_escrow(&order_id, &Symbol::new(&env, "Reason"), &buyer);
    }

    let next_order_id = DEFAULT_RATE_LIMIT_MAX_CALLS + 1;
    client.create_escrow(&buyer, &seller, &token, &1_000, &next_order_id, &None);
    let result = client.try_dispute_escrow(&next_order_id, &Symbol::new(&env, "Reason"), &buyer);
    assert!(
        result.is_err(),
        "6th dispute within the window should be rate-limited"
    );
}

#[test]
fn test_dispute_creation_rate_limit_resets_after_window() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    token_admin.mint(&buyer, &1_000_000_000);

    for order_id in 1..=DEFAULT_RATE_LIMIT_MAX_CALLS {
        client.create_escrow(&buyer, &seller, &token, &1_000, &order_id, &None);
        client.dispute_escrow(&order_id, &Symbol::new(&env, "Reason"), &buyer);
    }

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_RATE_LIMIT_WINDOW as u64 + 1;
    });

    let next_order_id = DEFAULT_RATE_LIMIT_MAX_CALLS + 1;
    client.create_escrow(&buyer, &seller, &token, &1_000, &next_order_id, &None);
    // Should succeed now that the window has reset.
    client.dispute_escrow(&next_order_id, &Symbol::new(&env, "Reason"), &buyer);
}

#[test]
fn test_rate_limit_is_per_account() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    token_admin.mint(&buyer, &1_000_000_000);

    for order_id in 1..=DEFAULT_RATE_LIMIT_MAX_CALLS {
        client.create_escrow(&buyer, &seller, &token, &1_000, &order_id, &None);
        client.dispute_escrow(&order_id, &Symbol::new(&env, "Reason"), &buyer);
    }

    // A different escrow disputed by the seller (different account) should
    // not be affected by the buyer's rate limit.
    let next_order_id = DEFAULT_RATE_LIMIT_MAX_CALLS + 1;
    client.create_escrow(&buyer, &seller, &token, &1_000, &next_order_id, &None);
    client.dispute_escrow(&next_order_id, &Symbol::new(&env, "Reason"), &seller);
}

#[test]
fn test_set_rate_limit_config_disables_limiter() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    token_admin.mint(&buyer, &1_000_000_000);

    client.set_rate_limit_config(&0, &0);

    for order_id in 1..=(DEFAULT_RATE_LIMIT_MAX_CALLS + 3) {
        client.create_escrow(&buyer, &seller, &token, &1_000, &order_id, &None);
        client.dispute_escrow(&order_id, &Symbol::new(&env, "Reason"), &buyer);
    }
}

// ── Structured Dispute Evidence Storage & Expiry Policies (#927) ───────

#[test]
fn test_submit_evidence_bound_to_valid_dispute_session() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);

    // Create escrow without disputing it yet
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token, &50_000_000, &1, &None);

    // Submission fails when not in dispute
    let result =
        client.try_submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://evidence-1"));
    assert!(result.is_err());

    // Initiate dispute
    client.dispute_escrow(&1, &Symbol::new(&env, "Reason"), &buyer);
    let escrow = client.get_escrow(&1);
    let dispute_session_id = escrow.dispute_initiated_at.unwrap();

    // Submission succeeds during valid dispute session
    let ev_id = client.submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://evidence-1"));
    assert_eq!(ev_id, 0);

    let log = client.get_evidence(&1);
    assert_eq!(log.len(), 1);
    let entry = log.get(0).unwrap();
    assert_eq!(entry.dispute_session_id, dispute_session_id);
    assert!(!entry.is_invalidated);
}

#[test]
fn test_expired_evidence_automatically_invalidated() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    client.submit_evidence(
        &1,
        &buyer,
        &String::from_str(&env, "ipfs://evidence-expiring"),
    );

    // Verify valid before expiry
    let valid_before = client.get_valid_evidence(&1);
    assert_eq!(valid_before.len(), 1);
    assert!(!valid_before.get(0).unwrap().is_invalidated);

    // Advance time past DEFAULT_EVIDENCE_EXPIRY_WINDOW (7 days)
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_EVIDENCE_EXPIRY_WINDOW + 10;
    });

    // get_evidence marks expired entries as invalidated
    let log_after = client.get_evidence(&1);
    assert_eq!(log_after.len(), 1);
    assert!(log_after.get(0).unwrap().is_invalidated);

    // get_valid_evidence returns empty list
    let valid_after = client.get_valid_evidence(&1);
    assert_eq!(valid_after.len(), 0);
}

#[test]
fn test_prevent_evidence_reuse_across_disputes() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);

    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token, &50_000_000, &2, &None);
    client.dispute_escrow(&2, &Symbol::new(&env, "Reason2"), &buyer);

    let payload = String::from_str(&env, "ipfs://reused-evidence-payload");

    // First use on order 1 succeeds
    client.submit_evidence(&1, &buyer, &payload);

    // Second use of exact same payload on order 2 fails
    let result = client.try_submit_evidence(&2, &buyer, &payload);
    assert!(result.is_err());
}

// ── Structured Evidence Records & Expiry (#1077 / #1078) ───────────────

/// #1077 AC1 + AC3: every stored record is versioned, bound to exactly one
/// dispute session, and carries a content digest equal to the SHA-256 of the
/// payload — stable metadata indexers can rely on without recomputation.
#[test]
fn test_evidence_record_carries_version_and_content_digest() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let uri = String::from_str(&env, "ipfs://versioned-evidence");
    let id = client.submit_evidence(&1, &buyer, &uri);

    let escrow = client.get_escrow(&1);
    let dispute_session_id = escrow.dispute_initiated_at.unwrap();

    let log = client.get_evidence(&1);
    assert_eq!(log.len(), 1);
    let entry = log.get(0).unwrap();

    // Versioned for indexer schema stability (#1077 AC3).
    assert_eq!(entry.version, EVIDENCE_SCHEMA_VERSION);
    // Bound to exactly one dispute: (id, order, session) all pinned (#1077 AC1).
    assert_eq!(entry.id, id);
    assert_eq!(entry.order_id, 1);
    assert_eq!(entry.dispute_session_id, dispute_session_id);
    // Digest is the verifiable SHA-256 of the payload (#1077 AC3).
    assert_eq!(entry.content_digest, evidence_digest(&env, &uri));
}

/// #1077 AC2: a duplicate content digest has defined, deterministic behavior —
/// it is rejected with `EvidenceAlreadyUsed` on both the primary and the
/// counter-evidence submission paths, never silently double-stored.
#[test]
fn test_duplicate_evidence_digest_is_rejected_deterministically() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let uri = String::from_str(&env, "ipfs://same-bytes");
    client.submit_evidence(&1, &buyer, &uri);

    // Same payload again (identical digest) → defined rejection.
    let dup = client.try_submit_evidence(&1, &seller, &uri);
    assert!(matches!(dup, Err(Ok(Error::EvidenceAlreadyUsed))));

    // The digest guard also covers the counter-evidence path: a fresh, valid
    // parent still cannot smuggle a previously-used payload back in.
    let parent = client.submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://parent"));
    let dup_counter = client.try_submit_counter_evidence(&1, &seller, &uri, &parent);
    assert!(matches!(dup_counter, Err(Ok(Error::EvidenceAlreadyUsed))));
}

/// #1078 AC1 + AC3: counter-evidence against a parent is accepted one second
/// before the parent's expiry deadline and rejected with `EvidenceExpired` at
/// the exact deadline (inclusive-end convention).
#[test]
fn test_counter_evidence_parent_expiry_exact_boundary() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, _admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let submitted_at = env.ledger().timestamp();
    let parent = client.submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://parent"));

    let expiry = DEFAULT_EVIDENCE_EXPIRY_WINDOW;

    // t = submitted_at + expiry - 1 → parent STILL VALID → counter accepted.
    env.ledger().with_mut(|li| {
        li.timestamp = submitted_at + expiry - 1;
    });
    let counter_ok = client.submit_counter_evidence(
        &1,
        &seller,
        &String::from_str(&env, "ipfs://counter-before"),
        &parent,
    );
    assert_eq!(counter_ok, 1);

    // t = submitted_at + expiry → parent EXPIRED → counter rejected (#1078 AC1).
    env.ledger().with_mut(|li| {
        li.timestamp = submitted_at + expiry;
    });
    let counter_expired = client.try_submit_counter_evidence(
        &1,
        &seller,
        &String::from_str(&env, "ipfs://counter-after"),
        &parent,
    );
    assert!(matches!(counter_expired, Err(Ok(Error::EvidenceExpired))));
}

/// #1078 AC1 + AC2: resolving a dispute at the evidence-expiry deadline
/// finalizes successfully AND persists the expiry sweep. Proven by rewinding
/// the clock below the deadline afterwards: read-time invalidation cannot fire
/// (now < expires_at), so a still-invalidated record can only have come from
/// the sweep that ran at finalization.
#[test]
fn test_resolution_persists_expired_evidence_sweep() {
    let env = Env::default();
    let (client, buyer, seller, token, token_admin, admin) = setup(&env);
    create_and_dispute(&env, &client, &buyer, &seller, &token, &token_admin, 1);

    let submitted_at = env.ledger().timestamp();
    client.submit_evidence(&1, &buyer, &String::from_str(&env, "ipfs://to-expire"));

    // Advance to the exact expiry deadline: evidence is expired, yet still
    // inside the 30-day dispute window and past the 1-day challenge window.
    env.ledger().with_mut(|li| {
        li.timestamp = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;
    });

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);
    assert_eq!(client.get_escrow(&1).status, EscrowStatus::Resolved);

    // Rewind below the deadline. If the sweep only lived in get_evidence's read
    // path, the record would read back valid here; a persisted invalidation
    // survives the rewind, proving finalization swept it (#1078 AC1/AC2).
    env.ledger().with_mut(|li| {
        li.timestamp = submitted_at;
    });
    let log = client.get_evidence(&1);
    assert_eq!(log.len(), 1);
    assert!(
        log.get(0).unwrap().is_invalidated,
        "resolution must persist evidence expiry at finalization"
    );
}
