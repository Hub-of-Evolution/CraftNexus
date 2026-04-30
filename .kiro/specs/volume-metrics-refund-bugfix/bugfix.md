# Bugfix Requirements Document

## Introduction

This bugfix addresses an issue where user trade volume metrics are artificially inflated when escrow refunds occur. The `update_user_metrics` function is called with `volume_delta` during refund operations, which incorrectly counts refunded amounts as part of a user's trade volume. This metric is used for auto-verification thresholds and reputation calculations, so inaccurate volume tracking can lead to incorrect user verification status.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN an escrow is refunded via the `refund` function THEN the system updates volume metrics for the seller with the full escrow amount

1.2 WHEN a dispute is resolved with `RefundToBuyer` resolution THEN the system updates volume metrics for the seller with the full escrow amount

1.3 WHEN volume metrics are updated on refund paths THEN the user's trade volume is artificially inflated, potentially triggering false auto-verification

### Expected Behavior (Correct)

2.1 WHEN an escrow is refunded via the `refund` function THEN the system SHALL NOT update volume metrics for any party

2.2 WHEN a dispute is resolved with `RefundToBuyer` resolution THEN the system SHALL NOT update volume metrics for any party

2.3 WHEN volume metrics are updated on refund paths THEN the system SHALL CONTINUE TO skip the metrics update entirely

### Unchanged Behavior (Regression Prevention)

3.1 WHEN an escrow is released via `release_funds` THEN the system SHALL CONTINUE TO update volume metrics for the seller

3.2 WHEN an escrow is auto-released via `auto_release` THEN the system SHALL CONTINUE TO update volume metrics for the seller

3.3 WHEN a dispute is resolved with `ReleaseToSeller` resolution THEN the system SHALL CONTINUE TO update volume metrics for the seller

3.4 WHEN a recurring escrow cycle completes successfully THEN the system SHALL CONTINUE TO update volume metrics for the artisan

3.5 WHEN volume metrics are updated for successful transactions THEN the system SHALL CONTINUE TO normalize the volume amount based on token decimals before adding to total_volume
