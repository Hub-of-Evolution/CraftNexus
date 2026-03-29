# Cross-Chain Energy Trading Bridge 🌉

The CraftNexus Cross-Chain Bridge is a secure, multi-layer infrastructure component facilitating interoperability of energy trading assets natively mapping across Stellar, Polygon, and Ethereum.

## 🏗️ Architecture

The bridge does not blindly map wrapper contracts (which expand inflation vectors). Instead, it relies on a **Liquidity Pool Escrow Mechanism**.
Users who bridge outbound contribute liquidity securely against existing pools. This releases natively verified, audited wrappers or native assets on the destination network leveraging distributed, multi-cloud validators ensuring continuous quorum consensus.

### Component Overview
1. **ICrossChainBridge**: Defacto definitions preventing internal drift.
2. **BridgeStructure**: Core data tracking (Pools, Receipts, Support definitions).
3. **BridgeLib**: Mathematical purity handling dynamic exponential fees, preventing network draining effectively.
4. **MultiChainValidator**: Employs Replay protection enforcing increasing nonce hashes uniquely via chain pairings and strictly validating threshold signatures dynamically.

## 🛡️ Security Vectors Mitigated

- **Double-Spending/Replays**: Every executed cross-chain request invokes `assertNonceIsUnique`.
- **Malicious Interception**: Network payload validations compute a strict internal hash dynamically encompassing all transfer data (`nonce:src:dest:sender:hash`). Without `SIGNATURE_THRESHOLD` (e.g. 3 of 4) approved signed Oracle nodes confirming this hash identically, the destination fails.
- **Liquidity Exhaustion**: The bridge employs an exponential sliding fee scale natively. If available liquidity falls < 20%, operations incur rapidly increasing penalties (capping naturally to enforce stable levels before hard blocking > 99.5% used).
- **Slippage Enforcement**: User-defined minimum acceptable limitations (`minOutput`) natively prevent excessive fees or malicious manipulation intercepting transactions. 

## ⚙️ Gas & Performance Optimizations

1. All transaction components enforce packed data structures avoiding extraneous string instantiations.
2. Replay checks map `Set<number>` configurations efficiently looking up historical execution natively O(1).
3. Internal events tracking locally bypasses expensive log-firing loops preserving processing speeds significantly.

## 🚀 Deployment

The system initializes off-chain locally to monitor environments utilizing:
```bash
ts-node scripts/deploy_bridge.ts
```

Set `.env` variables required mapping your node infrastructures and oracle providers exactly.
