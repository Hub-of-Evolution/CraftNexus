#!/bin/bash
# Formal contract safety gate (#1148).
#
# Combines compatibility, authorization, accounting, resource, and
# state-machine evidence into a single fail-closed release gate.
# A non-zero exit means the release/upgrade MUST NOT proceed.
#
# Usage:
#   ./scripts/safety_gate.sh
#
# Optional environment:
#   PROP_SEED                 Hex seed forwarded to property tests (default: 0xCAFEF00DDEADBEEF)
#   SAFETY_GATE_REPORT        Report path (default: target/safety-gate-report.json)
#   SKIP_WASM_BUILD           Set to 1 to skip native/WASM artifact validation
#   MAX_WASM_SIZE_BYTES       WASM size ceiling (default: 65536)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT}"

PROP_SEED="${PROP_SEED:-0xCAFEF00DDEADBEEF}"
REPORT_PATH="${SAFETY_GATE_REPORT:-${ROOT}/target/safety-gate-report.json}"
WASM_TARGET="${WASM_TARGET:-wasm32v1-none}"
WASM_ARTIFACT="${WASM_ARTIFACT:-target/${WASM_TARGET}/release/craft_nexus_contract.wasm}"
MAX_WASM_SIZE_BYTES="${MAX_WASM_SIZE_BYTES:-65536}"
SKIP_WASM_BUILD="${SKIP_WASM_BUILD:-0}"

SOURCE_STATE="$(git -C "${ROOT}/.." rev-parse --short HEAD 2>/dev/null || echo unknown)"
FAILED_INVARIANT=""
FAILED_SUITE=""
STATUS="pass"

mkdir -p "$(dirname "${REPORT_PATH}")"

log() {
    echo "▸ $*"
}

fail_suite() {
    FAILED_SUITE="$1"
    FAILED_INVARIANT="$2"
    STATUS="fail"
    echo "✗ Safety gate failed: suite=${FAILED_SUITE} invariant=${FAILED_INVARIANT} seed=${PROP_SEED} source=${SOURCE_STATE}"
    write_report
    exit 1
}

write_report() {
    local artifact="n/a"
    if [ -f "${WASM_ARTIFACT}" ]; then
        artifact="${WASM_ARTIFACT}"
    fi
    cat > "${REPORT_PATH}" <<EOF
{
  "status": "${STATUS}",
  "artifact": "${artifact}",
  "source_state": "${SOURCE_STATE}",
  "failed_suite": "${FAILED_SUITE}",
  "failed_invariant": "${FAILED_INVARIANT}",
  "reproducible_seed": "${PROP_SEED}",
  "suites": ["native_unit", "property_invariants", "settlement_disputes", "recurring_escrow", "staking", "onboarding", "recovery_admin", "upgrades", "reconciliation", "wasm_validation"]
}
EOF
    echo "Safety-gate report written to ${REPORT_PATH}"
}

run_suite() {
    local name="$1"
    local invariant="$2"
    shift 2
    log "Running suite ${name} (${invariant})..."
    if ! "$@"; then
        fail_suite "${name}" "${invariant}"
    fi
}

export PROP_SEED

log "Contract safety gate starting"
log "source_state=${SOURCE_STATE} seed=${PROP_SEED}"

run_suite "native_unit" "compile_and_host_tests" \
    cargo test --lib -- --nocapture

run_suite "property_invariants" "fund_conservation,no_double_settlement,fee_allocation" \
    cargo test --lib prop_ -- --nocapture

run_suite "settlement_disputes" "expired_dispute_policy,mutual_exclusion" \
    cargo test --lib expired_dispute -- --nocapture

run_suite "recurring_escrow" "recurring_id_and_lifecycle" \
    cargo test --lib recurring -- --nocapture

run_suite "staking" "stake_cooldown_and_conservation" \
    cargo test --lib staking -- --nocapture

run_suite "onboarding" "onboarding_state_machine" \
    cargo test --lib onboarding -- --nocapture

run_suite "recovery_admin" "admin_revision_idempotency" \
    cargo test --lib admin_idempotency -- --nocapture

run_suite "upgrades" "duplicate_approval,compatibility_manifest" \
    cargo test --lib upgrade -- --nocapture

run_suite "reconciliation" "accounting_reconciliation" \
    cargo test --lib reconcil -- --nocapture

if [ "${SKIP_WASM_BUILD}" != "1" ]; then
    if ! rustup target list --installed | grep -qx "${WASM_TARGET}"; then
        log "Installing Rust target ${WASM_TARGET}"
        rustup target add "${WASM_TARGET}"
    fi
    log "Building WASM artifact..."
    if ! RUSTFLAGS="-C opt-level=z -C lto -C panic=abort" cargo build --target "${WASM_TARGET}" --release --locked; then
        fail_suite "wasm_validation" "native_wasm_build"
    fi
    if [ ! -f "${WASM_ARTIFACT}" ]; then
        fail_suite "wasm_validation" "missing_wasm_artifact"
    fi
    WASM_SIZE_BYTES="$(wc -c < "${WASM_ARTIFACT}" | tr -d '[:space:]')"
    log "WASM artifact ${WASM_ARTIFACT} (${WASM_SIZE_BYTES} bytes, limit ${MAX_WASM_SIZE_BYTES})"
    if [ "${WASM_SIZE_BYTES}" -gt "${MAX_WASM_SIZE_BYTES}" ]; then
        fail_suite "wasm_validation" "wasm_size_limit"
    fi
fi

write_report
echo "✓ Safety gate passed. artifact=${WASM_ARTIFACT:-n/a} source=${SOURCE_STATE} seed=${PROP_SEED}"
exit 0
