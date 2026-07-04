#!/bin/bash
# Deploy CraftNexus Escrow Contract to Stellar Testnet/Mainnet
# Usage: ./scripts/deploy-contract.sh [testnet|mainnet] [SECRET_KEY]
# Note: This script should be run from the craft-nexus directory.
#       It assumes the contract is located at ../craft-nexus-contract

set -e

# Configuration
NETWORK=${1:-testnet}
SECRET_KEY=${2:-""}

if [ -z "$SECRET_KEY" ]; then
    echo "❌ Error: Secret key required"
    echo "Usage: ./scripts/deploy-contract.sh [testnet|mainnet] [SECRET_KEY]"
    exit 1
fi

echo "🚀 Deploying CraftNexus Escrow contract to $NETWORK..."

# Set network configuration
if [ "$NETWORK" = "mainnet" ]; then
    RPC_URL="https://soroban-rpc.mainnet.stellar.org"
    NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
else
    RPC_URL="https://soroban-testnet.stellar.org"
    NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
fi

# Determine directories
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
CONTRACT_DIR="$PROJECT_ROOT/../craft-nexus-contract"

if [ ! -d "$CONTRACT_DIR" ]; then
    echo "❌ Error: Contract directory not found at $CONTRACT_DIR"
    echo "Please ensure the 'craft-nexus-contract' folder exists adjacent to 'craft-nexus'."
    exit 1
fi

# 1. Build the contract
echo "🛠️  Building contract..."
cd "$CONTRACT_DIR"
stellar contract build

WASM_PATH="target/wasm32-unknown-unknown/release/craft_nexus_contract.wasm"
if [ ! -f "$WASM_PATH" ]; then
    echo "❌ Error: WASM file not found at $WASM_PATH"
    exit 1
fi

# 2. Deploy the contract
echo "🌐 Deploying to Stellar $NETWORK..."
CONTRACT_ID=$(stellar contract deploy \
    --wasm "$WASM_PATH" \
    --source "$SECRET_KEY" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --network "$NETWORK")

# 3. Output results
echo ""
echo "✅ Contract deployed successfully!"
echo "----------------------------------------------------"
echo "Network:     $NETWORK"
echo "Contract ID: $CONTRACT_ID"
echo "----------------------------------------------------"
echo ""
echo "Next Step: Add this to your craft-nexus/.env.local (or .env):"
echo "NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=$CONTRACT_ID"
echo ""
