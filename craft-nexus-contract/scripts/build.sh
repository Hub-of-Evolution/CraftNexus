#!/bin/bash
set -e

echo "🔨 Building CraftNexus Escrow Contract..."

# Check if stellar CLI is installed
if ! command -v stellar &> /dev/null; then
    echo "❌ Stellar CLI not found. Please run: ./scripts/install-stellar-cli.sh"
    exit 1
fi

# Build the contract
stellar contract build

# Check if build was successful
if [ -f "target/wasm32-unknown-unknown/release/craft_nexus_contract.wasm" ]; then
    echo "✅ Contract built successfully!"
    echo "📦 WASM file: target/wasm32-unknown-unknown/release/craft_nexus_contract.wasm"
    
    # Show file size
    SIZE=$(du -h target/wasm32-unknown-unknown/release/craft_nexus_contract.wasm | cut -f1)
    echo "📊 Size: $SIZE"
else
    echo "❌ Build failed. WASM file not found."
    exit 1
fi
