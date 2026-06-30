#!/bin/bash
set -euo pipefail

echo "Building CraftNexus contract with optimized settings..."

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "${SCRIPT_DIR}/.."

WASM_TARGET="${WASM_TARGET:-wasm32v1-none}"
WASM_ARTIFACT="${WASM_ARTIFACT:-target/${WASM_TARGET}/release/craft_nexus_contract.wasm}"
MAX_WASM_SIZE_BYTES="${MAX_WASM_SIZE_BYTES:-65536}"
RUN_TESTS="${RUN_TESTS:-1}"
GENERATE_CONTRACT_ID="${GENERATE_CONTRACT_ID:-0}"
STELLAR_NETWORK="${STELLAR_NETWORK:-futurenet}"
BUILD_RUSTFLAGS="${BUILD_RUSTFLAGS:--C opt-level=z -C lto -C panic=abort}"

if ! rustup target list --installed | grep -qx "${WASM_TARGET}"; then
    echo "Installing missing Rust target: ${WASM_TARGET}"
    rustup target add "${WASM_TARGET}"
fi

echo "Running optimized release build..."
RUSTFLAGS="${BUILD_RUSTFLAGS}" cargo build --target "${WASM_TARGET}" --release

if [ ! -f "${WASM_ARTIFACT}" ]; then
    echo "Error: build succeeded but artifact is missing: ${WASM_ARTIFACT}"
    exit 1
fi

# Apply wasm-opt for additional size reduction if available
if command -v wasm-opt >/dev/null 2>&1; then
    echo "Applying wasm-opt optimization..."
    wasm-opt -Oz -o "${WASM_ARTIFACT}.opt" "${WASM_ARTIFACT}"
    if [ -f "${WASM_ARTIFACT}.opt" ]; then
        mv "${WASM_ARTIFACT}.opt" "${WASM_ARTIFACT}"
        echo "wasm-opt optimization completed"
    fi
else
    echo "Note: wasm-opt not found in PATH. Install it for additional size reduction: npm install -g wasm-opt"
fi

WASM_SIZE_BYTES="$(wc -c < "${WASM_ARTIFACT}" | tr -d '[:space:]')"
echo "Built artifact: ${WASM_ARTIFACT}"
echo "Contract size: ${WASM_SIZE_BYTES} bytes (limit: ${MAX_WASM_SIZE_BYTES} bytes)"

if [ "${WASM_SIZE_BYTES}" -gt "${MAX_WASM_SIZE_BYTES}" ]; then
    echo "Error: Contract size (${WASM_SIZE_BYTES}) exceeds limit (${MAX_WASM_SIZE_BYTES})"
    exit 1
fi

if [ "${RUN_TESTS}" = "1" ]; then
    echo "Running contract tests..."
    cargo test -- --nocapture
fi

if [ "${GENERATE_CONTRACT_ID}" = "1" ]; then
    STELLAR_BIN=""
    if command -v stellar >/dev/null 2>&1; then
        STELLAR_BIN="stellar"
    elif command -v soroban >/dev/null 2>&1; then
        STELLAR_BIN="soroban"
    elif [ -x "./.local-bin/stellar-cli-bin" ]; then
        STELLAR_BIN="./.local-bin/stellar-cli-bin"
    fi

    if [ -n "${STELLAR_BIN}" ]; then
        echo "Generating contract ID for network: ${STELLAR_NETWORK}"
        "${STELLAR_BIN}" contract id generate --network "${STELLAR_NETWORK}"
    else
        echo "Skipping contract ID generation: Stellar CLI not found."
    fi
fi

echo "Build and validation completed successfully."
