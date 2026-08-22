#!/bin/bash
set -euo pipefail

# Issue #680: Add Soroban testnet deployment script with environment variable validation

DRY_RUN=false
POSITIONAL_ARGS=()

for arg in "$@"; do
  case $arg in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    *)
      POSITIONAL_ARGS+=("$arg")
      shift
      ;;
  esac
done

set -- "${POSITIONAL_ARGS[@]:-}"

NETWORK=${1:-testnet}
SOURCE_ACCOUNT=${2:-${STELLAR_SECRET_KEY:-}}
WASM_TARGET=${WASM_TARGET:-wasm32v1-none}
WASM_ARTIFACT=${WASM_ARTIFACT:-target/${WASM_TARGET}/release/craft_nexus_contract.wasm}

# Load .env or .env.local if present
if [ -f ".env" ]; then
    set -a
    source .env
    set +a
elif [ -f ".env.local" ]; then
    set -a
    source .env.local
    set +a
fi

# Determine default RPC_URL and NETWORK_PASSPHRASE based on network if not explicitly set
if [ "$NETWORK" = "testnet" ]; then
    RPC_URL=${RPC_URL:-"https://soroban-testnet.stellar.org:443"}
    NETWORK_PASSPHRASE=${NETWORK_PASSPHRASE:-"Test SDF Network ; September 2015"}
elif [ "$NETWORK" = "mainnet" ]; then
    RPC_URL=${RPC_URL:-"https://soroban-rpc.mainnet.stellar.org:443"}
    NETWORK_PASSPHRASE=${NETWORK_PASSPHRASE:-"Public Global Stellar Network ; September 2015"}
else
    echo "❌ Error: Invalid network '$NETWORK'. Supported networks: 'testnet' or 'mainnet'."
    exit 1
fi

# Environment & parameter validation
echo "🔍 Validating deployment environment..."

if [ -z "$SOURCE_ACCOUNT" ]; then
    echo "❌ Error: Source account / private key missing!"
    echo "Usage: ./scripts/deploy.sh [testnet|mainnet] <SOURCE_ACCOUNT> [--dry-run]"
    echo "Or export STELLAR_SECRET_KEY in environment or .env file (see .env.example)."
    exit 1
fi

if [ -z "$RPC_URL" ]; then
    echo "❌ Error: RPC_URL is required but empty."
    exit 1
fi

if [ -z "$NETWORK_PASSPHRASE" ]; then
    echo "❌ Error: NETWORK_PASSPHRASE is required but empty."
    exit 1
fi

echo "  Network: $NETWORK"
echo "  RPC URL: $RPC_URL"
echo "  Network Passphrase: $NETWORK_PASSPHRASE"
echo "  Source Account: $SOURCE_ACCOUNT"
echo "  Artifact: $WASM_ARTIFACT"
echo "  Dry Run Mode: $DRY_RUN"

if [ "$DRY_RUN" = true ]; then
    echo ""
    echo "🧪 DRY RUN SUMMARY: All environment variables and configuration settings validated successfully!"
    echo "   No smart contract deployment was performed on-chain."
    exit 0
fi

echo "🚀 Starting deployment to $NETWORK..."

# Build contract artifacts
./scripts/build.sh

# Configure network in Stellar CLI if not present
stellar network add \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    "$NETWORK" 2>/dev/null || true

# Deploy
echo "Deploying contract..."
CONTRACT_ID=$(stellar contract deploy \
    --wasm "$WASM_ARTIFACT" \
    --source-account "$SOURCE_ACCOUNT" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --network "$NETWORK")

echo ""
echo "Contract deployed successfully!"
echo "Contract ID: $CONTRACT_ID"
echo ""
echo "Add this to your .env.local:"
echo "NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=$CONTRACT_ID"
