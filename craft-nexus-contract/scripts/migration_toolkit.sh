#!/bin/bash
set -euo pipefail

# Migration toolkit for CraftNexusContract (#944).
#
# Wraps the on-chain migration primitives (pre_migration_check,
# backup_platform_config, rollback_platform_config, get_version) so a
# migration runbook step is a single command instead of a hand-typed
# `stellar contract invoke`. See docs/versioned-state-migration.md for the
# full runbook this toolkit supports.
#
# Usage:
#   ./scripts/migration_toolkit.sh <command> [args...]
#
# Commands:
#   version                          Print the current on-chain contract version.
#   check <expected_version>         Fail unless the contract is at <expected_version>.
#   backup                           Snapshot PlatformConfig; prints the backup id.
#   list-backups                     List retained PlatformConfig backups.
#   rollback <backup_id>             Restore PlatformConfig from <backup_id>.
#
# Required environment variables:
#   CONTRACT_ID   Deployed contract address.
#   SOURCE        Admin identity/key to sign with (stellar CLI --source).
#   NETWORK       Network name configured in the stellar CLI (default: testnet).

NETWORK=${NETWORK:-testnet}

usage() {
    echo "Usage: $0 <version|check|backup|list-backups|rollback> [args...]"
    echo ""
    echo "Required env vars: CONTRACT_ID, SOURCE. Optional: NETWORK (default: testnet)."
    exit 1
}

require_env() {
    if [ -z "${CONTRACT_ID:-}" ]; then
        echo "❌ CONTRACT_ID is not set."
        exit 1
    fi
    if [ -z "${SOURCE:-}" ]; then
        echo "❌ SOURCE is not set."
        exit 1
    fi
}

COMMAND=${1:-}
shift || true

case "$COMMAND" in
    version)
        stellar contract invoke \
            --id "$CONTRACT_ID" \
            --network "$NETWORK" \
            --send=no \
            -- \
            get_version
        ;;
    check)
        require_env
        EXPECTED_VERSION=${1:-}
        if [ -z "$EXPECTED_VERSION" ]; then
            echo "Usage: $0 check <expected_version>"
            exit 1
        fi
        echo "🔎 Verifying contract is at version $EXPECTED_VERSION before migrating..."
        stellar contract invoke \
            --id "$CONTRACT_ID" \
            --source "$SOURCE" \
            --network "$NETWORK" \
            -- \
            pre_migration_check --expected_version "$EXPECTED_VERSION"
        echo "✅ Version check passed."
        ;;
    backup)
        require_env
        echo "💾 Snapshotting PlatformConfig before migration..."
        BACKUP_ID=$(stellar contract invoke \
            --id "$CONTRACT_ID" \
            --source "$SOURCE" \
            --network "$NETWORK" \
            -- \
            backup_platform_config)
        echo "✅ Backup taken. backup_id=$BACKUP_ID"
        echo "   Keep this id — pass it to 'rollback' if the migration needs to be undone."
        ;;
    list-backups)
        stellar contract invoke \
            --id "$CONTRACT_ID" \
            --network "$NETWORK" \
            --send=no \
            -- \
            get_platform_config_backups
        ;;
    rollback)
        require_env
        BACKUP_ID=${1:-}
        if [ -z "$BACKUP_ID" ]; then
            echo "Usage: $0 rollback <backup_id>"
            exit 1
        fi
        echo "⏪ Rolling back PlatformConfig to backup $BACKUP_ID..."
        stellar contract invoke \
            --id "$CONTRACT_ID" \
            --source "$SOURCE" \
            --network "$NETWORK" \
            -- \
            rollback_platform_config --backup_id "$BACKUP_ID"
        echo "✅ Rollback complete."
        ;;
    *)
        usage
        ;;
esac
