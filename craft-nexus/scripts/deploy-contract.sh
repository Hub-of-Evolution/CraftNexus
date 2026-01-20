#!/bin/bash
# Deploy CraftNexus Escrow Contract to Stellar Testnet/Mainnet
# Usage: ./scripts/deploy-contract.sh [testnet|mainnet] [SECRET_KEY]
# Note: This script should be run from craft-nexus directory
#       Contract is located at ../craft-nexus-contract

set -e

NETWORK=${1:-testnet}
SECRET_KEY=${2:-""}

if [ -z "$SECRET_KEY" ]; then
    echo "Error: Secret key required"
    echo "Usage: ./scripts/deploy-contract.sh [testnet|mainnet] [SECRET_KEY]"
    exit 1
fi

echo "Deploying contract to $NETWORK..."

# Set network configuration
if [ "$NETWORK" = "mainnet" ]; then
    RPC_URL="https://soroban-rpc.mainnet.stellar.org"
    NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
else
    RPC_URL="https://soroban-testnet.stellar.org"
    NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
fi

# Get script directory and navigate to contract
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
CONTRACT_DIR="$PROJECT_ROOT/../craft-nexus-contract"

if [ ! -d "$CONTRACT_DIR" ]; then
    echo "Error: Contract directory not found at $CONTRACT_DIR"
    echo "Make sure craft-nexus-contract folder exists in the root directory"
    exit 1
fi

# Build contract
echo "Building contract..."
cd "$CONTRACT_DIR"
soroban contract build

# Deploy contract
echo "Deploying contract..."
CONTRACT_ID=$(soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
    --source "$SECRET_KEY" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --network "$NETWORK")

echo ""
echo "✅ Contract deployed successfully!"
echo "Contract ID: $CONTRACT_ID"
echo ""
echo "Add this to your craft-nexus/.env.local:"
echo "NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=$CONTRACT_ID"
