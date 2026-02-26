#!/bin/bash
set -e

echo "🚀 Installing Stellar CLI..."

# Install Stellar CLI
cargo install --locked stellar-cli

# Verify installation
if command -v stellar &> /dev/null; then
    echo "✅ Stellar CLI installed successfully!"
    stellar --version
else
    echo "❌ Installation failed. Please check your Rust installation."
    exit 1
fi

# Ensure WASM target is installed
echo "📦 Checking WASM target..."
if rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "✅ WASM target already installed"
else
    echo "📥 Installing WASM target..."
    rustup target add wasm32-unknown-unknown
fi

echo ""
echo "✨ Setup complete! You can now build the contract with:"
echo "   ./scripts/build.sh"
