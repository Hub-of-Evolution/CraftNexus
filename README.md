# CraftNexus

## Overview



https://github.com/user-attachments/assets/b02f665f-1ea2-4682-ab97-67c237b3ac01



CraftNexus is a global digital marketplace that enables artisans to sell **handcrafted products** and **educational courses** directly to buyers worldwide. The platform leverages **Stellar’s low-cost payment infrastructure** to provide instant, borderless, and fair payments without unnecessary complexity.

CraftNexus exists to empower creators in emerging markets by removing banking barriers, reducing transaction fees, and giving artisans direct ownership of their income.

---

## Problem Statement

Millions of artisans globally face structural barriers:

- Limited access to international banking
- High cross-border payment fees
- Dependence on intermediaries that reduce earnings
- Difficulty monetizing skills digitally

Traditional marketplaces solve distribution but fail at **fair global payments**.

---

## Solution

CraftNexus provides:

- A unified marketplace for **physical artisan goods** and **digital courses**
- **USDC-based payments on Stellar** for stability and speed
- Simple wallets instead of bank accounts
- A creator-first revenue model

Blockchain is used strictly where it adds value: **payments and settlement**.

---

## Core Features

### 1. Artisan Storefronts

- Public artisan profile
- Product listings (physical goods)
- Course listings (video, guides, workshops)
- Pricing displayed in local equivalents

### 2. Course Marketplace

- Pay-to-unlock access
- On-demand or scheduled content
- Progress tracking
- Downloadable resources

### 3. Payments (Stellar)

- USDC as the primary transaction currency
- XLM used transparently for network fees
- Near-instant settlement (~5 seconds)
- Near-zero transaction costs

### 4. Trust & Quality

- Verified artisan profiles
- Buyer reviews and ratings
- Purchase history
- Optional escrow-style release for physical items

---

## Why Stellar

CraftNexus aligns directly with Stellar’s mission to create **equitable access to the global financial system**.

- Low fees enable micro-transactions
- Stablecoin-native (USDC)
- Proven use in remittances and fintech
- Strong support for emerging markets

---

## User Journeys

### Buyer Journey

1. Discover artisan or course
2. View product details and reviews
3. Pay with USDC (wallet or on-ramp)
4. Instant confirmation
5. Receive product or unlock course

### Artisan Journey

1. Create account and wallet
2. Set up storefront
3. List products or courses
4. Receive payments instantly in USDC
5. Withdraw locally via anchor or hold digitally

---


## Revenue Model

- Platform commission per sale (5–10%)
- Featured artisan listings
- Course promotion tools
- Optional SaaS tools for artisans

---

## Grant & Accelerator Positioning (Stellar)

### Why CraftNexus Fits Stellar Programs

- Real-world payments use case
- Financial inclusion focus
- Emerging-market creator economy
- Stablecoin adoption (USDC)
- Low-speculation, utility-first design


---

## MVP Scope

- Artisan profiles
- Product & course listings
- USDC payment flow via Stellar network
- Stellar wallet integration (Freighter)
- Smart contract escrow for secure payments
- Basic reviews.
- Course delivery

## Stellar Integration & Smart Contracts

CraftNexus is built with **native Stellar integration** and **Soroban Smart Contracts** for secure, on-chain escrow functionality.

### Implemented Features

✅ **Stellar SDK Integration**
- Full Stellar SDK integration for payments
- USDC token support on Stellar network
- Network-agnostic (Testnet/Mainnet)

✅ **Wallet Integration**
- Freighter wallet support (browser extension)
- Non-custodial wallet management
- Secure key management

✅ **Payment Service**
- USDC payment processing
- Commission splitting (5% platform fee)
- Transaction tracking and verification
- Balance checking

✅ **Smart Contract Escrow**
- Soroban-based escrow contract (Rust)
- Secure payment holding for physical goods
- Automatic release after delivery window
- Buyer-controlled release
- Refund functionality

### Smart Contract Details

**Contract Location:** `craft-nexus-contract/`

**Key Functions:**
- `create_escrow` - Create escrow for order
- `release_funds` - Release funds to seller
- `auto_release` - Auto-release after time window
- `refund` - Refund to buyer for disputes
- `get_escrow` - Query escrow status

**Contract Address:**
- **Testnet**: Deploy using `scripts/deploy-contract.sh`
- **Mainnet**: TBD

See [craft-nexus-contract/README.md](craft-nexus-contract/README.md) for deployment instructions.

---


# Payments Architecture & Trust Model

CraftNexus uses the Stellar network strictly for **payments, settlement, and value transfer**, ensuring speed, transparency, and financial inclusion without introducing unnecessary blockchain complexity.

## Stablecoin Strategy

* **USDC on Stellar** is the primary transaction currency across the platform.
* Prices are displayed in local currency equivalents for user clarity, while settlement occurs in USDC.
* This provides:

  * Price stability for artisans
  * Predictable earnings
  * Protection against local currency volatility
* **XLM** is used only for network fees and is abstracted from end users.

CraftNexus does **not** introduce a native token.

---

## Wallet & Account Model

* Each artisan is provisioned with a **non-custodial Stellar wallet** at onboarding.
* Wallet creation and signing are abstracted through a simplified UX to support non-technical users.
* Users retain full ownership of their funds at all times.

This model removes the need for:

* Traditional bank accounts
* International wire transfers
* Third-party payout intermediaries

---

## Anchor & Cash-Out Strategy

CraftNexus integrates with **existing Stellar anchors** to enable fiat on-ramps and off-ramps.

### Initial Focus Regions

* Africa
* Latin America
* Southeast Asia

Artisans can:

* Hold USDC digitally
* Convert to local fiat via supported anchors
* Use USDC for other Stellar-based services

Anchor selection prioritizes:

* Regulatory compliance
* Regional availability
* Low conversion fees

---

## Payment Flow

1. Buyer initiates purchase (product or course)
2. USDC payment is sent on Stellar
3. Transaction settles in ~5 seconds
4. Artisan receives funds instantly in their wallet
5. Platform commission is automatically deducted

All transactions are:

* Transparent
* Auditable
* Final upon settlement

---

## Escrow & Delivery Protection

CraftNexus implements an **escrow-style payment release model** for physical goods to protect both buyers and artisans.

### Digital Goods & Courses

* Funds are released immediately upon successful payment
* Course access is unlocked instantly

### Physical Products

* Payment is held temporarily at the platform level
* Funds are released when:

  * Delivery is confirmed by the buyer, or
  * A predefined delivery window elapses without dispute

This approach balances:

* Buyer trust
* Artisan cash-flow reliability
* Operational simplicity for the MVP

**✅ IMPLEMENTED**: CraftNexus uses **Soroban Smart Contracts** for fully on-chain escrow functionality. All escrow operations are executed on the Stellar blockchain, providing transparency and security without intermediaries.

---


## Technical Philosophy

CraftNexus follows a **utility-first blockchain design**:

* Blockchain is used only where it provides clear user value
* No speculative mechanics
* No financial abstraction layers
* No unnecessary tokenization

The goal is to make blockchain **invisible to users**, while delivering its benefits transparently.

---

## Impact Alignment

CraftNexus directly advances:

* Financial inclusion
* Global creator monetization
* Stablecoin adoption for real commerce
* Economic empowerment in emerging markets

By lowering payment friction and eliminating banking barriers, CraftNexus enables artisans to **own their income, reach global buyers, and scale sustainably**.

---

## Grant & Ecosystem Readiness

CraftNexus is designed to meet the criteria of:

* Stellar Community Fund
* Ecosystem accelerators
* Impact-driven Web3 programs

The platform demonstrates:

* ✅ **Real-world usage of Stellar** - Live payments and smart contracts
* ✅ **Stellar Smart Contracts (Soroban)** - On-chain escrow implementation
* ✅ **Native wallet integration** - Freighter and Stellar ecosystem tools
* ✅ **Stablecoin adoption** - USDC payments on Stellar network
* ✅ **Clear protocol dependency** - Built entirely on Stellar infrastructure
* ✅ **Sustainable business economics** - Low-fee micro-transactions
* ✅ **Measurable social impact** - Financial inclusion for emerging markets


---


##  Installation & Local Setup

Follow the steps below to run CraftNexus locally on your machine.

### Prerequisites

Ensure you have the following installed:

- Node.js (v18 or later recommended)
- npm
- Git
- Rust (for smart contract development - optional)
- Soroban CLI (for deploying contracts - optional)

**For Full Setup (Including Contracts):**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Add Rust WASM target
rustup target add wasm32-unknown-unknown
```

**Verify your setup:**
```bash
node -v
npm -v
git --version
rustc --version  # Optional
soroban --version  # Optional
```

## Step 1: Fork the Repository

Visit the official repository:
1. https://github.com/EvolutionalHub/CraftNexus
2. Click Fork to create a copy under your GitHub account.
---

## Step 2: Clone Your Fork

Clone your forked repository:
```bash
git clone https://github.com/<your-github-username>/CraftNexus.git
```

Navigate into the project directory:
```bash
cd CraftNexus
```

## Step 3: Install Dependencies
```bash
cd craft-nexus
npm install
```

This will install:
- Next.js and React dependencies
- **@stellar/stellar-sdk** - Stellar blockchain SDK
- **@stellar/freighter-api** - Freighter wallet integration

## Step 4: Configure Environment Variables

Create a `.env.local` file in the `craft-nexus` directory:

```env
# Stellar Network Configuration
NEXT_PUBLIC_STELLAR_NETWORK=TESTNET

# Escrow Smart Contract Address (deploy contract first - see below)
NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=

# Platform Commission Wallet (optional)
NEXT_PUBLIC_PLATFORM_WALLET=
```

## Step 5: Deploy Smart Contract (Optional)

To deploy the escrow smart contract:

```bash
# Build the contract
cd ../craft-nexus-contract
soroban contract build

# Deploy to testnet (requires Stellar account secret key)
cd ../craft-nexus
./scripts/deploy-contract.sh testnet YOUR_SECRET_KEY

# Copy the contract address to .env.local
```

Or use the deployment script:
```bash
cd craft-nexus
chmod +x scripts/deploy-contract.sh
./scripts/deploy-contract.sh testnet YOUR_SECRET_KEY
```

## Step 6: Start the Development Server
```bash
npm run dev
```

## Step 7: Open in Browser
Visit the application at: http://localhost:3000

## Step 8: Connect Wallet

1. Install [Freighter Wallet](https://freighter.app) browser extension
2. Create or import a Stellar account
3. Fund your account with testnet XLM from [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test)
4. Connect your wallet in the CraftNexus app

---


## Contribution Guide

We welcome contributions from developers, designers, and Web3 enthusiasts.

### Contribution Workflow

1.Fork the repository

2.Clone your fork locally

3.Create a new branch using the naming convention below
```bash git checkout -b feat:fixed-navbar```

Branch Naming Convention:
-feat: – New features or improvements
-fix: – Bug fixes
-refactor: – Code refactoring
-docs: – Documentation updates

Examples:
-feat:add-course-filter
-fix:wallet-connection-issue
-docs:update-readme

---


##Commit & Push Changes

```bash
git add .
git commit -m "feat: fixed navigation bar responsiveness"
git push origin feat:fixed-navbar
```
---

## Open a Pull Request (PR)

1. Open a Pull Request from your branch to main
2. Clearly describe:
-What you changed
-Why you changed it
-Screenshots or demos (if UI-related)
all PR's are reviewd before merging
---

## Reviews & Community Communication

If your PR is ready and you are looking for a review, join the official CraftNexus Telegram group and notify the maintainers.

Official Telegram Group:
https://t.me/c/2334943542/1
All contributor updates, reviews, and announcements are communicated there.

---

## Support the Project

If you find CraftNexus useful:

Give the repository a star ⭐

Share it with others who support artisan empowerment and Web3 innovation

Your support helps the project grow and attract contributors, partners, and grants.
---


## Vision

CraftNexus aims to become the default global platform where craftsmanship and knowledge are traded freely—without borders, banks, or intermediaries.

---

## Technical Implementation

### Architecture

```
craft-nexus/
├── lib/stellar/          # Stellar integration services
│   ├── config.ts        # Network configuration
│   ├── wallet.ts        # Wallet management
│   ├── payments.ts      # Payment processing
│   └── contracts.ts     # Smart contract interface
├── components/
│   └── molecules/
│       └── ConnectWalletModal.tsx  # Stellar wallet UI
└── scripts/
    └── deploy-contract.sh  # Contract deployment script

craft-nexus-contract/     # Soroban smart contract (separate directory)
├── src/lib.rs           # Contract source (Rust)
├── Cargo.toml           # Rust dependencies
└── README.md            # Contract documentation
```

### Key Technologies

- **Frontend**: Next.js 15, React 19, TypeScript
- **Blockchain**: Stellar SDK, Soroban Smart Contracts
- **Wallet**: Freighter API
- **Payments**: USDC on Stellar network

### Smart Contract Security

- Funds are held in escrow contract
- Only buyer can release funds (after delivery confirmation)
- Auto-release after configurable time window
- Refund functionality for disputes
- All operations are on-chain and transparent

## Development Workflow

### Testing Payments

1. Connect Freighter wallet with testnet account
2. Fund account with testnet USDC (or use friendbot for XLM)
3. Create test order
4. Process payment through Stellar network
5. Verify transaction on [Stellar Explorer](https://stellar.expert/explorer/testnet)

### Testing Smart Contracts

```bash
# Build contract
cd ../craft-nexus-contract
soroban contract build

# Run tests (add test file first)
soroban contract test

# Deploy and interact
soroban contract deploy --wasm target/.../escrow.wasm --source YOUR_KEY
```

## Stellar Ecosystem Integration

CraftNexus integrates with:

- ✅ **Stellar SDK** - Core blockchain operations
- ✅ **Freighter Wallet** - Browser wallet integration
- ✅ **Soroban Smart Contracts** - On-chain escrow
- 🔄 **SEP-24** - Anchor integration (planned)
- 🔄 **SEP-10** - Web authentication (planned)
- 🔄 **Stellar Anchors** - Fiat on/off-ramps (planned)

## Status

CraftNexus MVP is **ready for hackathon demonstration** with:

- ✅ Full Stellar blockchain integration
- ✅ Smart contract escrow system
- ✅ Wallet connectivity (Freighter)
- ✅ USDC payment processing
- 🔄 Complete marketplace UI (in progress)
- 🔄 Anchor integrations (future work)

Contributors, partners, and ecosystem supporters are welcome!

---

## License

This project is open-source and available under the MIT License.
