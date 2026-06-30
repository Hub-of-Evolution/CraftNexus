# Storage Reference

This file maps `DataKey` variants to storage type, TTL strategy, and conservative max entry size estimates. See docs/deprecated-storage.md for deprecated keys.

| Key Variant | Storage Type | TTL Strategy | Max Entry Size |
|---|---:|---|---:|
| `Escrow(u32)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `BuyerEscrows(Address)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `SellerEscrows(Address)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `MinEscrowAmount(Address)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `TotalFees(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `FeeTokenIndex` | Persistent | Extend on write | ~64B (small) |
| `FeeTokenConfig(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `ContractVersion` | Persistent | Extend on write | ~64B (small) |
| `PlatformConfig` | Persistent | Extend on write | ~64B (small) |
| `ArtisanFeeTier(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `ReferralRewardBps` | Persistent | Extend on write | ~64B (small) |
| `ArtisanStake(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `StakeCooldownEnd(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `ArtisanStakeQueue(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `ArtisanStakeQueueCount(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `ArtisanStakeQueueIndexed(Address, u32)` | Persistent | Extend on write | ~32-64B (small) |
| `PartialRefundProposal(u32)` | Persistent | Extend on write | ~64B (small) |
| `ReentryGuard` | Persistent | Extend on write | ~64B (small) |
| `PendingAdmin` | Persistent | Extend on write | ~64B (small) |
| `WasmUpgradeProposal` | Persistent | Extend on write | ~64B (small) |
| `MaxReleaseWindow` | Persistent | Extend on write | ~64B (small) |
| `OnboardingContractAddress` | Persistent | Extend on write | ~32-64B (small) |
| `WhitelistedTokens` | Persistent | Extend on write | Variable (up to several KB) |
| `WhitelistedTokenIndexed(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `WhitelistedTokenCount` | Persistent | Extend on write | ~32-64B (small) |
| `AllEscrowIds` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `EscrowCount` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `GlobalEscrowIdIndexed(u32)` | Persistent | Extend on write | ~32-64B (small) |
| `FallbackAdmin` | Persistent | Extend on write | ~64B (small) |
| `AdminRecoveryTime` | Persistent | Extend on write | ~64B (small) |
| `StakeHistory(Address)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `StakeHistoryCount(Address)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `StakeLastModified(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `BuyerEscrowIndexed(Address, u32)` | Persistent | Extend on write | ~32-64B (small) |
| `SellerEscrowIndexed(Address, u32)` | Persistent | Extend on write | ~32-64B (small) |
| `BuyerEscrowCount(Address)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `SellerEscrowCount(Address)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `TotalLocked(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `TotalStaked(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `UpgradeHistory` | Persistent | Both (extend on read & write) | Variable (up to several KB) |
| `RecurringEscrow(u64)` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `NextRecurringEscrowId` | Persistent | Extend on write | ~512B-1KB (Escrow struct) |
| `ActiveObligations(Address)` | Persistent | Extend on write | ~32-64B (small) |
| `UpgradeThreshold` | Persistent | Extend on write | ~64B (small) |
| `UpgradeApprovals(BytesN<32>)` | Persistent | Extend on write | ~64B (small) |
| `UpgradeSigners` | Persistent | Extend on write | ~64B (small) |
| `UserProfile(Address)` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `Username(String)` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `Config` | Persistent | Extend on write | ~64B (small) |
| `UserMetrics(Address)` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `VerificationRequest(Address)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `VerificationQueueHead` | Persistent | Both (extend on read & write) | ~64B (small) |
| `VerificationQueueTail` | Persistent | Both (extend on read & write) | ~64B (small) |
| `VerificationQueueIndex(u64)` | Persistent | Both (extend on read & write) | ~64B (small) |
| `VerificationHistory(Address)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `VerificationHistoryCount(Address)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `VerificationHistoryIndexed(Address, u32)` | Persistent | Both (extend on read & write) | ~32-64B (small) |
| `UsernameChangeFee` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `UsernameChangeFeeToken` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `UsernameChangeFeeWallet` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `LastUsernameChange(Address)` | Persistent | Both (extend on read & write) | ~256B (includes username, optional portfolio CID) |
| `ActiveContractCount(Address)` | Persistent | Extend on write | ~32-64B (small) |
