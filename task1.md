
7 matches
#411 [BUG] Clean up unused variable warnings (token_contract) inside src/test.rs:394
Repo Avatar
Hub-of-Evolution/CraftNexus
[BUG] Clean up unused variable warnings (token_contract) inside src/test.rs:394
Context and Background
The host test file defines a token_contract variable on line 394 that is never used in the subsequent code, triggering compiler warnings.
This issue is part of the "stellar wave" codebase optimization sweep. We are targeting high performance, safety, and reliability on the Stellar network. Specifically, this issue focuses on src/test.rs around line/range 394.

When developing on the Stellar Soroban network, storage fees and CPU instructions are premium resources. Every byte stored persistently on-chain incurs continuous rent, making storage structure optimization a critical task. To combat this, the Soroban SDK utilizes the extend_ttl method to refresh the Time-To-Live (TTL) of persistent values, preventing storage keys from expiring and becoming archived. Rent is calculated based on ledger entry size, making compact structures like symbol_short symbols and flat structures extremely advantageous over heavy vectors. Furthermore, developers must strictly adhere to the check-effect-interactions pattern. Because Soroban smart contracts can trigger callbacks on arbitrary token contracts, any external transfer must be the final action in the execution path. Failing to structure calls in this manner opens the contract to devastating reentrancy attacks where a malicious actor withdraws funds repeatedly before their internal balance is updated in persistent storage.

Impact and Severity
Category: Bug
Target File: src/test.rs
Specific Lines: 394
Impact Level: Low warning cleanup
Without resolving this issue, the CraftNexus backend faces increased risks of operational failures, excessive gas consumption on-chain, or structural vulnerabilities during high-congestion periods on the Stellar ledger.

Proposed Action Items & Implementation Guide
To properly resolve this issue, the contributor should execute the following multi-step plan:

Locate the unused declaration on line 394.
Remove the variable if it is redundant, or prefix it with an underscore (_token_contract).
Verify compile output.
Ensure that all modifications respect the overall project architecture. In particular, keep in mind that onboarding profiles utilize the CURRENT_USER_PROFILE_VERSION flag, and any modifications to profile shapes must be handled via corresponding upgrade scripts.

CRITICAL PREREQUISITE: Fix Active Compilation and Build Errors
IMPORTANT WARNING FOR ALL CONTRIBUTORS:
The codebase is currently in a broken state due to active compilation and test suite build errors! Before you attempt to implement the changes specified in this issue, you MUST verify that your local environment can build the project successfully. The existing compilation errors include:

An unresolved reference to config inside src/lib.rs inside platform fee helper calls.
Method not found errors for .unwrap() on Vec within the test files in src/test.rs.
Unused variable and feature warnings that clutter build outputs.
Your first step must be to resolve these build issues or ensure that your changes do not introduce further build blocks. We expect all tests to pass (cargo test) before any pull request for this issue is reviewed. Do not bypass local checks, and do not push code that fails compilation. Thank you for maintaining the health of the project!

Verification & Testing Instructions
Run local tests: cargo test to ensure that all onboarding and escrow suites pass cleanly.
Verify gas and size footprints using the guest target: cargo build --target wasm32-unknown-unknown --release.
Check the snapshot files under test_snapshots/ to assert that no unintended event schemas or state shapes have changed.
Ensure no warnings are emitted under cargo check

#412 [BUG] Fix unused variable warning (escrow) in src/enhanced_features_test.rs:138
Repo Avatar
Hub-of-Evolution/CraftNexus
[BUG] Fix unused variable warning (escrow) in src/enhanced_features_test.rs:138
Context and Background
The enhanced features test file defines a tuple unpacking containing escrow that is never read.
This issue is part of the "stellar wave" codebase optimization sweep. We are targeting high performance, safety, and reliability on the Stellar network. Specifically, this issue focuses on src/enhanced_features_test.rs around line/range 138.

When developing on the Stellar Soroban network, storage fees and CPU instructions are premium resources. Every byte stored persistently on-chain incurs continuous rent, making storage structure optimization a critical task. To combat this, the Soroban SDK utilizes the extend_ttl method to refresh the Time-To-Live (TTL) of persistent values, preventing storage keys from expiring and becoming archived. Rent is calculated based on ledger entry size, making compact structures like symbol_short symbols and flat structures extremely advantageous over heavy vectors. Furthermore, developers must strictly adhere to the check-effect-interactions pattern. Because Soroban smart contracts can trigger callbacks on arbitrary token contracts, any external transfer must be the final action in the execution path. Failing to structure calls in this manner opens the contract to devastating reentrancy attacks where a malicious actor withdraws funds repeatedly before their internal balance is updated in persistent storage.

Impact and Severity
Category: Bug
Target File: src/enhanced_features_test.rs
Specific Lines: 138
Impact Level: Low warning cleanup
Without resolving this issue, the CraftNexus backend faces increased risks of operational failures, excessive gas consumption on-chain, or structural vulnerabilities during high-congestion periods on the Stellar ledger.

Proposed Action Items & Implementation Guide
To properly resolve this issue, the contributor should execute the following multi-step plan:

Open src/enhanced_features_test.rs at line 138.
Prefix escrow with an underscore (_escrow) in the tuple assignment.
Verify all test files compile cleanly without warnings.
Ensure that all modifications respect the overall project architecture. In particular, keep in mind that onboarding profiles utilize the CURRENT_USER_PROFILE_VERSION flag, and any modifications to profile shapes must be handled via corresponding upgrade scripts.

CRITICAL PREREQUISITE: Fix Active Compilation and Build Errors
IMPORTANT WARNING FOR ALL CONTRIBUTORS:
The codebase is currently in a broken state due to active compilation and test suite build errors! Before you attempt to implement the changes specified in this issue, you MUST verify that your local environment can build the project successfully. The existing compilation errors include:

An unresolved reference to config inside src/lib.rs inside platform fee helper calls.
Method not found errors for .unwrap() on Vec within the test files in src/test.rs.
Unused variable and feature warnings that clutter build outputs.
Your first step must be to resolve these build issues or ensure that your changes do not introduce further build blocks. We expect all tests to pass (cargo test) before any pull request for this issue is reviewed. Do not bypass local checks, and do not push code that fails compilation. Thank you for maintaining the health of the project!

Verification & Testing Instructions
Run local tests: cargo test to ensure that all onboarding and escrow suites pass cleanly.
Verify gas and size footprints using the guest target: cargo build --target wasm32-unknown-unknown --release.
Check the snapshot files under test_snapshots/ to assert that no unintended event schemas or state shapes have changed.
Ensure no warnings are emitted under cargo check.

#413 [BUG] Resolve potential double mutable borrow in platform fee updates inside src/lib.rs
Repo Avatar
Hub-of-Evolution/CraftNexus
[BUG] Resolve potential double mutable borrow in platform fee updates inside src/lib.rs
Context and Background
Updating platform fee metrics may lead to multiple mutable references of the environmental storage context being active simultaneously.
This issue is part of the "stellar wave" codebase optimization sweep. We are targeting high performance, safety, and reliability on the Stellar network. Specifically, this issue focuses on src/lib.rs around line/range 2500-2600.

When developing on the Stellar Soroban network, storage fees and CPU instructions are premium resources. Every byte stored persistently on-chain incurs continuous rent, making storage structure optimization a critical task. To combat this, the Soroban SDK utilizes the extend_ttl method to refresh the Time-To-Live (TTL) of persistent values, preventing storage keys from expiring and becoming archived. Rent is calculated based on ledger entry size, making compact structures like symbol_short symbols and flat structures extremely advantageous over heavy vectors. Furthermore, developers must strictly adhere to the check-effect-interactions pattern. Because Soroban smart contracts can trigger callbacks on arbitrary token contracts, any external transfer must be the final action in the execution path. Failing to structure calls in this manner opens the contract to devastating reentrancy attacks where a malicious actor withdraws funds repeatedly before their internal balance is updated in persistent storage.

Impact and Severity
Category: Bug
Target File: src/lib.rs
Specific Lines: 2500-2600
Impact Level: Medium code safety
Without resolving this issue, the CraftNexus backend faces increased risks of operational failures, excessive gas consumption on-chain, or structural vulnerabilities during high-congestion periods on the Stellar ledger.

Proposed Action Items & Implementation Guide
To properly resolve this issue, the contributor should execute the following multi-step plan:

Locate fee calculation functions.
Restructure storage calls to release the mutable borrow before initiating fee transfers.
Add safety assertions in tests.
Ensure that all modifications respect the overall project architecture. In particular, keep in mind that onboarding profiles utilize the CURRENT_USER_PROFILE_VERSION flag, and any modifications to profile shapes must be handled via corresponding upgrade scripts.

CRITICAL PREREQUISITE: Fix Active Compilation and Build Errors
IMPORTANT WARNING FOR ALL CONTRIBUTORS:
The codebase is currently in a broken state due to active compilation and test suite build errors! Before you attempt to implement the changes specified in this issue, you MUST verify that your local environment can build the project successfully. The existing compilation errors include:

An unresolved reference to config inside src/lib.rs inside platform fee helper calls.
Method not found errors for .unwrap() on Vec within the test files in src/test.rs.
Unused variable and feature warnings that clutter build outputs.
Your first step must be to resolve these build issues or ensure that your changes do not introduce further build blocks. We expect all tests to pass (cargo test) before any pull request for this issue is reviewed. Do not bypass local checks, and do not push code that fails compilation. Thank you for maintaining the health of the project!

Verification & Testing Instructions
Run local tests: cargo test to ensure that all onboarding and escrow suites pass cleanly.
Verify gas and size footprints using the guest target: cargo build --target wasm32-unknown-unknown --release.
Check the snapshot files under test_snapshots/ to assert that no unintended event schemas or state shapes have changed.
Ensure no warnings are emitted under cargo check.

