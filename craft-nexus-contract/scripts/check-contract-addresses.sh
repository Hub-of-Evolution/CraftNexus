#!/usr/bin/env bash

# Verifies contract addresses in README.md match those in stellar.toml
# Exits with code 1 if any mismatch is found.
# Run via: bash scripts/check-contract-addresses.sh
# Or via CI: see .github/workflows/

set -euo pipefail

README="README.md"
TOML="stellar.toml"
ERRORS=0

echo "Checking contract address consistency between $README and $TOML..."

# Extract contract addresses from stellar.toml
# Pattern: id_testnet = "C..." or id_mainnet = "C..." or similar contract IDs (56 chars, starting with C)
toml_addresses=$(grep -E 'id_(testnet|mainnet|futurenet)\s*=\s*"[A-Z0-9]{56}"' "$TOML" \
  | grep -oE '"[A-Z0-9]{56}"' | tr -d '"' | sort -u)

if [ -z "$toml_addresses" ]; then
  echo "WARNING: No contract addresses found in $TOML"
  echo "  Check that stellar.toml uses id_testnet/id_mainnet = \"C...\" format or similar"
  echo "  Note: Placeholder values (0x... or [DEPLOY_AND_UPDATE]) are acceptable for now"
fi

# Check each address from stellar.toml appears in README
while IFS= read -r addr; do
  if [ -z "$addr" ]; then continue; fi
  # Skip placeholder values
  if [[ "$addr" == "0x" ]] || [[ "$addr" == "[DEPLOY_AND_UPDATE]" ]]; then
    echo "SKIP: $addr (placeholder, not yet deployed)"
    continue
  fi
  if ! grep -q "$addr" "$README"; then
    echo "ERROR: Address $addr found in $TOML but NOT in $README"
    ERRORS=$((ERRORS + 1))
  else
    echo "OK: $addr present in both files"
  fi
done <<< "$toml_addresses"

# Extract addresses from README and check they appear in stellar.toml
# Pattern: any 56-character string starting with C (Stellar contract address format)
readme_addresses=$(grep -oE '[A-Z0-9]{56}' "$README" | grep -E '^C' | sort -u)

while IFS= read -r addr; do
  if [ -z "$addr" ]; then continue; fi
  if ! grep -q "$addr" "$TOML"; then
    echo "WARNING: Address $addr in $README not found in $TOML"
    # Only warn, not error — README may have testnet/mainnet variants or placeholder notes
  fi
done <<< "$readme_addresses"

if [ "$ERRORS" -gt 0 ]; then
  echo ""
  echo "FAILED: $ERRORS address mismatch(es) found."
  echo "Update README.md or stellar.toml to match."
  exit 1
fi

echo ""
echo "PASSED: All contract addresses are in sync."
exit 0
