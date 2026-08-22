#!/bin/bash
set -e

STELLAR_CLI_VERSION="${STELLAR_CLI_VERSION:-25.1.0}"
WASM_TARGET="${WASM_TARGET:-wasm32v1-none}"

echo "Installing Stellar CLI v${STELLAR_CLI_VERSION}..."

# Install Stellar CLI (pinned version for reproducibility)
if command -v stellar &> /dev/null; then
    INSTALLED_VERSION=$(stellar --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "0.0.0")
    if [ "$INSTALLED_VERSION" = "$STELLAR_CLI_VERSION" ]; then
        echo "Stellar CLI v${STELLAR_CLI_VERSION} already installed."
    else
        echo "Upgrading stellar-cli from v${INSTALLED_VERSION} to v${STELLAR_CLI_VERSION}..."
        cargo install --locked stellar-cli --version "${STELLAR_CLI_VERSION}"
    fi
else
    cargo install --locked stellar-cli --version "${STELLAR_CLI_VERSION}"
fi

# Verify installation
if command -v stellar &> /dev/null; then
    echo "Stellar CLI installed successfully!"
    stellar --version
else
    echo "Installation failed. Please check your Rust installation."
    exit 1
fi

# Ensure WASM target is installed
echo "Checking WASM target..."
if rustup target list --installed | grep -qx "${WASM_TARGET}"; then
    echo "WASM target '${WASM_TARGET}' already installed"
else
    echo "Installing WASM target: ${WASM_TARGET}..."
    rustup target add "${WASM_TARGET}"
fi

echo ""
echo "Setup complete! You can now build the contract with:"
echo "   ./scripts/build.sh"
