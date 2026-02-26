#!/bin/bash
set -e

NETWORK=${1:-testnet}
SOURCE_ACCOUNT=${2}

if [ -z "$SOURCE_ACCOUNT" ]; then
    echo "Usage: ./scripts/deploy.sh [testnet|mainnet] <SOURCE_ACCOUNT>"
    echo "Example: ./scripts/deploy.sh testnet alice"
    exit 1
fi

echo "🚀 Deploying to $NETWORK..."

# Build first
./scripts/build.sh

# Configure network if not already done
if [ "$NETWORK" = "testnet" ]; then
    stellar network add \
        --rpc-url https://soroban-testnet.stellar.org:443 \
        --network-passphrase "Test SDF Network ; September 2015" \
        testnet 2>/dev/null || true
elif [ "$NETWORK" = "mainnet" ]; then
    stellar network add \
        --rpc-url https://soroban-rpc.mainnet.stellar.org:443 \
        --network-passphrase "Public Global Stellar Network ; September 2015" \
        mainnet 2>/dev/null || true
else
    echo "❌ Invalid network. Use 'testnet' or 'mainnet'"
    exit 1
fi

# Deploy
echo "📤 Deploying contract..."
CONTRACT_ID=$(stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/craft_nexus_contract.wasm \
    --source "$SOURCE_ACCOUNT" \
    --network "$NETWORK")

echo ""
echo "✅ Contract deployed successfully!"
echo "📝 Contract ID: $CONTRACT_ID"
echo ""
echo "Add this to your .env.local:"
echo "NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=$CONTRACT_ID"
