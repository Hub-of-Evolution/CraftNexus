# CraftNexus Escrow Smart Contract

Stellar Smart Contract (Soroban) for secure escrow payments on CraftNexus.

## Overview

This contract handles escrow functionality for marketplace transactions:
- Secure payment holding for physical goods
- Automatic release after delivery window
- Buyer-controlled release
- Refund functionality for disputes

## Prerequisites

- Rust 1.70.0 or later
- Stellar Soroban CLI
- Stellar account with testnet XLM (for deployment)

## Installation

### Install Soroban CLI

```bash
cargo install --locked --version 21.0.0 soroban-cli
```

### Install Rust Target

```bash
rustup target add wasm32-unknown-unknown
```

## Building the Contract

```bash
cd craft-nexus-contract
soroban contract build
```

This will create a WASM file in `target/wasm32-unknown-unknown/release/escrow.wasm`

## Testing (Testnet)

### Deploy Contract

```bash
# Set network to testnet
soroban config network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"

# Deploy contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source <YOUR_SECRET_KEY> \
  --network testnet
```

### Initialize Contract

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <YOUR_SECRET_KEY> \
  --network testnet \
  -- \
  create_escrow \
  --buyer <BUYER_ADDRESS> \
  --seller <SELLER_ADDRESS> \
  --token <USDC_TOKEN_ADDRESS> \
  --amount 1000000000 \
  --order_id 1 \
  --release_window 604800
```

## Contract Functions

### `create_escrow`
Create a new escrow for an order.

**Parameters:**
- `buyer`: Buyer's Stellar address
- `seller`: Seller's Stellar address  
- `token`: Token contract address (USDC)
- `amount`: Amount in stroops (1 USDC = 10,000,000 stroops)
- `order_id`: Unique order identifier
- `release_window`: Time in seconds before auto-release (default: 604800 = 7 days)

### `release_funds`
Release funds to seller (called by buyer after delivery confirmation).

**Parameters:**
- `order_id`: Order identifier

### `auto_release`
Auto-release funds after release window (seller can call).

**Parameters:**
- `order_id`: Order identifier

### `refund`
Refund funds to buyer (for disputes).

**Parameters:**
- `order_id`: Order identifier
- `authorized_address`: Address authorized to refund

### `get_escrow`
Get escrow details.

**Parameters:**
- `order_id`: Order identifier

### `can_auto_release`
Check if escrow can be auto-released.

**Parameters:**
- `order_id`: Order identifier

## Integration

See `craft-nexus/lib/stellar/contracts.ts` for TypeScript integration examples.

## Contract Address

- **Testnet**: `[DEPLOY_AND_UPDATE]`
- **Mainnet**: `[DEPLOY_AND_UPDATE]`
