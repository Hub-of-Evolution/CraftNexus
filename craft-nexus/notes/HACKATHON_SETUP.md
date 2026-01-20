# CraftNexus Hackathon Quick Start Guide

## 🚀 Quick Setup for Hackathon Demo

### Prerequisites Check
```bash
node -v  # Should be v18+
npm -v
```

### 1. Install Dependencies
```bash
cd craft-nexus
npm install
```

### 2. Configure Environment
Create `.env.local`:
```env
NEXT_PUBLIC_STELLAR_NETWORK=TESTNET
NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS=
```

### 3. Start Dev Server
```bash
npm run dev
```

### 4. Test Wallet Connection

1. **Install Freighter**: https://freighter.app
2. **Create Testnet Account**:
   - Open Freighter
   - Create new account
   - Switch to Testnet network
   - Get testnet XLM from: https://laboratory.stellar.org/#account-creator?network=test
3. **Connect in App**: Click "Connect Wallet" button

### 5. Deploy Smart Contract (Optional)

```bash
# Install Rust first if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Add WASM target
rustup target add wasm32-unknown-unknown

# Build contract
cd ../craft-nexus-contract
soroban contract build

# Deploy (replace YOUR_SECRET_KEY with testnet account secret)
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source YOUR_SECRET_KEY \
  --network testnet

# Copy contract address to .env.local
```

## 🎯 Demo Checklist

### What to Show Judges

✅ **Stellar Integration**
- [ ] Wallet connection (Freighter)
- [ ] Network configuration (Testnet)
- [ ] Account balance display

✅ **Smart Contract**
- [ ] Contract deployment proof
- [ ] Escrow creation
- [ ] Contract address verification

✅ **Payment Flow**
- [ ] USDC payment simulation
- [ ] Transaction hash display
- [ ] Stellar Explorer link

✅ **Code Quality**
- [ ] Clean TypeScript code
- [ ] Proper error handling
- [ ] Smart contract Rust code

## 📋 Talking Points

### Why Stellar?
- **Low fees**: Near-zero transaction costs enable micro-payments
- **Speed**: ~5 second settlement
- **USDC support**: Stable payments for artisans
- **Smart contracts**: On-chain escrow with Soroban

### What Makes This Different?
- **Real smart contract**: Not just a UI mockup
- **Native integration**: Built with Stellar SDK, not generic Web3
- **Financial inclusion**: Designed for emerging markets
- **Production-ready architecture**: Proper separation of concerns

### Technical Highlights
- Soroban smart contracts (Rust)
- TypeScript payment service
- Wallet abstraction layer
- Configurable network (Testnet/Mainnet)

## 🔗 Useful Links

- **Stellar Explorer (Testnet)**: https://stellar.expert/explorer/testnet
- **Freighter Wallet**: https://freighter.app
- **Stellar Laboratory**: https://laboratory.stellar.org
- **Soroban Docs**: https://soroban.stellar.org/docs

## 🐛 Troubleshooting

**Wallet won't connect:**
- Ensure Freighter is installed and unlocked
- Check network matches (Testnet)
- Refresh page and try again

**Contract deployment fails:**
- Verify account has XLM for fees
- Check network configuration
- Ensure Rust/Soroban CLI installed correctly

**Payment fails:**
- Verify account has USDC balance (or testnet equivalent)
- Check transaction memo requirements
- Review browser console for errors

## 📝 Next Steps for Full MVP

1. Complete marketplace UI
2. Add product/course listing pages
3. Implement order management
4. Add review/rating system
5. Integrate with Stellar anchors for fiat conversion

---

**Ready for Hackathon! 🎉**
