# Event Schema Versioning

CraftNexus lifecycle event payloads use `schema_version` as the first field.
Current schema version: `1`.

## Compatibility Rules

- Field ordering is canonical and follows the Rust `contracttype` declaration order.
- `schema_version` must stay the first payload field for every lifecycle event.
- Version `1` schemas are append-only. Do not rename, remove, or reorder fields without incrementing `LIFECYCLE_EVENT_SCHEMA_VERSION`.
- Indexers must read `schema_version` before decoding the rest of the payload.
- Additive fields in a future version must be documented in `test_snapshots/*_event.json` and handled by upgrade tests before deployment.

## Version 1 Lifecycle Events

| Event | Fields |
| --- | --- |
| `EscrowEvent` | `schema_version`, `escrow_id`, `action`, `buyer`, `seller`, `amount`, `token`, `timestamp` |
| `EscrowResolvedEvent` | `schema_version`, `escrow_id`, `buyer`, `seller`, `arbitrator`, `amount`, `token`, `timestamp` |
| `ReputationUpdateEvent` | `schema_version`, `address`, `successful_delta`, `disputed_delta`, `metrics_sales_delta`, `metrics_amount`, `token`, `timestamp` |
| `ConfigUpdatedEvent` | `schema_version`, `field_name`, `old_value`, `new_value` |
| `ArtisanFeeTierUpdatedEvent` | `schema_version`, `artisan`, `fee_bps` |
| `TokensStakedEvent` | `schema_version`, `artisan`, `token`, `amount` |
| `TokensUnstakedEvent` | `schema_version`, `artisan`, `token`, `amount` |
| `MetadataVerifiedEvent` | `schema_version`, `order_id`, `verifier`, `timestamp` |
| `PlatformPausedEvent` | `schema_version`, `initiator`, `timestamp` |
| `PlatformUnpausedEvent` | `schema_version`, `initiator`, `timestamp` |
| `RecurringEscrowEvent` | `schema_version`, `id`, `action`, `buyer`, `artisan`, `amount`, `timestamp` |
| `UpgradeProposalEvent` | `schema_version`, `action`, `wasm_hash`, `admin`, `timestamp`, `upgrade_at` |
| `UserOnboardedEvent` | `schema_version`, `user`, `username`, `role` |

## Migration Notes

Existing consumers that assumed unversioned payloads must migrate to a versioned
decoder. During an upgrade, indexers should accept previously recorded
unversioned events as legacy data and decode new events by branching on
`schema_version == 1`. New contract emissions use the versioned payloads only.

Before any future contract upgrade changes an event payload, add compatibility
coverage that proves existing version `1` consumers can still read version `1`
events and that upgraded consumers branch correctly for the new version.
