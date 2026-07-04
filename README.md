# CraftNexus: Decentralized Artisanal Marketplace

CraftNexus is a modern, decentralized web3 marketplace designed for artisans to showcase and sell handcrafted goods and educational courses globally. The platform utilizes Stellar's **Soroban** smart contracts to secure transactions, handle user onboarding, and manage trustless escrow payments.


https://github.com/user-attachments/assets/58de2761-6137-4f2b-a8a0-6c9c57ebea46


---

## System Architecture

CraftNexus consists of two main components:

1. **Frontend (`/craft-nexus-forntend`)**: A Next.js (v15) application built with React 19, TypeScript, and TailwindCSS (v4). It integrates with the **Freighter Wallet** to interact with Stellar's Testnet.
2. **Smart Contracts (`/craft-nexus-contract`)**: A suite of Stellar Soroban smart contracts written in Rust:
   - **Onboarding Contract**: Manages user profiles and roles (Buyer, Artisan, Admin, Arbitrator).
   - **Escrow Contract**: Handles secure fund holding, dispute resolution, refunds, and platform fees.

---

## Repository Directory Structure

```text
CraftNexus/
├── craft-nexus-forntend/    # Next.js web application
├── craft-nexus-contract/    # Soroban smart contracts (Rust)
├── .gitignore               # Ignored files (Python scripts, caches, etc.)
└── README.md                # Root documentation (this file)
```

---

## Getting Started

### Prerequisites

Ensure you have the following installed on your machine:
* [Node.js](https://nodejs.org/) (v18 or higher)
* [Rust](https://www.rust-lang.org/) (v1.70 or higher) and `cargo`
* [Git](https://git-scm.com/)
* [Freighter Wallet browser extension](https://www.freighter.app/)

---

## Smart Contract Setup & Execution

### 1. Installation & Environment Configuration
Navigate to the contract directory and install the necessary WebAssembly (WASM) build target and the **Stellar CLI**:

```bash
cd craft-nexus-contract

# Add the WASM target
rustup target add wasm32-unknown-unknown

# Install the Stellar CLI
cargo install --locked stellar-cli
```

### 2. Build the Contracts
To compile optimized WASM artifacts and validate their sizes:

```bash
# Using the helper build script (compiles, validates WASM size, and runs tests)
./scripts/build.sh

# Or manual release build
RUSTFLAGS="-C opt-level=z -C lto -C panic=abort" cargo build --target wasm32v1-none --release --locked
```
The compiled output will be generated at `target/wasm32v1-none/release/craft_nexus_contract.wasm`.

### 3. Run Unit Tests
To execute the contract test suite:

```bash
cargo test -- --nocapture
```

### 4. Deploy to Stellar Testnet
First, add the Testnet configuration to your Stellar CLI:

```bash
stellar network add --rpc-url https://soroban-testnet.stellar.org:443 --network-passphrase "Test SDF Network ; September 2015" testnet
```

Then deploy the contract:

```bash
# Automated deployment using the provided script
./scripts/deploy.sh testnet <YOUR_IDENTITY_NAME>

# Manual deployment
stellar contract deploy \
  --wasm target/wasm32v1-none/release/craft_nexus_contract.wasm \
  --source <YOUR_IDENTITY_NAME_OR_SECRET_KEY> \
  --network testnet
```
Note the **Contract ID** outputted from the deployment command.

---

## Frontend Setup & Running

### 1. Install Dependencies
Navigate to the frontend directory and install NPM packages:

```bash
cd craft-nexus-forntend
npm install
```

### 2. Setup Environment Variables
Create a `.env.local` file from the example configuration:

```bash
cp .env.example .env.local
```

Open `.env.local` and update the following settings:

```env
# Stellar Network Configuration
NEXT_PUBLIC_STELLAR_NETWORK=TESTNET

# Deployed Smart Contract ID (from the deployment step above)
NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=C...

# Platform Commission Wallet Address (optional)
NEXT_PUBLIC_PLATFORM_WALLET=G...
```

### 3. Start the Development Server
Run the local Next.js dev server:

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) in your web browser.

### 4. Connect and Fund Your Wallet
1. Open the Freighter Wallet extension and switch the network to **Testnet**.
2. Go to the [Stellar Laboratory Friendbot](https://laboratory.stellar.org/#account-creator?network=test) and create/fund your testnet account with XLM.
3. Click "Connect Wallet" on the CraftNexus interface to start buying or selling!

---

## 🔗 Reference Documentation

For detailed information, refer to the individual component guides:
- [Smart Contract Reference Guide](file:///c:/Users/HomePC/Desktop/CraftNexus/CraftNexus/craft-nexus-contract/README.md)
- [Frontend Developer Guide](file:///c:/Users/HomePC/Desktop/CraftNexus/CraftNexus/craft-nexus-forntend/README.md)
