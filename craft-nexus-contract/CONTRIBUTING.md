# Contributing to CraftNexus Smart Contracts

## Contract Address Updates

When deploying new contract versions or updating contract addresses:

### Before Making Changes

1. Ensure both `README.md` and `stellar.toml` are up-to-date in your current branch
2. Run the local check to verify they're in sync:
   ```bash
   bash scripts/check-contract-addresses.sh
   ```

### Making Address Changes

Contract addresses **must be updated in BOTH files**:

1. **README.md** - Update the "Contract Addresses" section with the new contract IDs
2. **stellar.toml** - Update the corresponding `id_testnet`, `id_mainnet`, or `id_futurenet` fields

Example update:
```md
# In README.md
### Testnet
- **Escrow Contract**: `CABC...XYZ` (56 characters, starts with C)
```

```toml
# In stellar.toml
[contracts.main]
id_futurenet = "CABC...XYZ"
```

### After Making Changes

1. Run the verification script locally:
   ```bash
   bash scripts/check-contract-addresses.sh
   ```
   You should see:
   ```
   PASSED: All contract addresses are in sync.
   ```

2. Commit and push your changes

3. The CI workflow `.github/workflows/contract-address-check.yml` will automatically verify that:
   - All addresses in `stellar.toml` appear in `README.md`
   - All contract addresses in `README.md` exist in `stellar.toml`
   - No mismatches are present

### If the CI Check Fails

The workflow will fail with an error message like:
```
ERROR: Address CABC...XYZ found in stellar.toml but NOT in README.md
```

**To fix:**
- Add the missing address to the other file
- Ensure the address format is correct (56 characters, starts with C for Stellar contract IDs)
- Re-run the local check and commit again

## Contract Development

For general development guidelines, see `README.md` in the root of this directory.

## Deployment

Deployment instructions are documented in `README.md` under the "Deployment" section. Always ensure addresses are properly synchronized after deployment using the check script.
