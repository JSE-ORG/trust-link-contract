#951 Add test for batch_create_escrow with MAX_BATCH_SIZE

Chore: Add test for batch_create_escrow with MAX_BATCH_SIZE
Description
Batch creation limits are not stress-tested. May fail on mainnet due to instruction limits.

Requirements & Context
Category: Testing Gaps

Requires updating contracts/escrow/src/test_multicall.rs.

Acceptance Criteria
 Test batch creating the maximum allowed escrows.
Suggested Execution
git checkout -b chore/add-test-for-batchcreateescrow-with-maxbatchsize
Suggested Commit Message
chore: add test for batch_create_escrow with max_batch_size
Testing Notes
Test the updated behavior in contracts/escrow/src/test_multicall.rs.

References
contracts/escrow/src/test_multicall.rs (around line 100)

Definition of Done
 Ready for review.


#952 Add test for rotate_resolver by secondary payee

Chore: Add test for rotate_resolver by secondary payee
Description
The authorization matrix for secondary payees rotating resolvers is untested. Unintended authorization bypasses.

Requirements & Context
Category: Testing Gaps

Requires updating contracts/escrow/src/test_resolver_rotation.rs.

Acceptance Criteria
 Ensure secondary payees can (or cannot) rotate resolvers as designed.
Suggested Execution
git checkout -b chore/add-test-for-rotateresolver-by-secondary-payee
Suggested Commit Message
chore: add test for rotate_resolver by secondary payee
Testing Notes
Test the updated behavior in contracts/escrow/src/test_resolver_rotation.rs.

References
contracts/escrow/src/test_resolver_rotation.rs (around line 50)

Definition of Done
 Ready for review.

#953 Add test for approve_refund by secondary payee

Chore: Add test for approve_refund by secondary payee
Description
The vulnerability allowing secondary payees to approve refunds lacks a failing test. Missing coverage for critical authorization flaws.

Requirements & Context
Category: Testing Gaps

Requires updating contracts/escrow/src/test_refund_override.rs.

Acceptance Criteria
 Write a test demonstrating the exploit.
Suggested Execution
git checkout -b chore/add-test-for-approverefund-by-secondary-payee
Suggested Commit Message
chore: add test for approve_refund by secondary payee
Testing Notes
Test the updated behavior in contracts/escrow/src/test_refund_override.rs.

References
contracts/escrow/src/test_refund_override.rs (around line 30)

Definition of Done
 Ready for review.


#954 Add test for mutual_cancel with malicious token reentrancy

Chore: Add test for mutual_cancel with malicious token reentrancy
Description
CEI violations in mutual cancel are not covered by malicious token mocks. Reentrancy is undetected in CI.

Requirements & Context
Category: Testing Gaps

Requires updating contracts/escrow/src/test_malicious_token.rs.

Acceptance Criteria
 Implement a malicious SEP-41 mock that re-enters mutual_cancel.
Suggested Execution
git checkout -b chore/add-test-for-mutualcancel-with-malicious-token-reentrancy
Suggested Commit Message
chore: add test for mutual_cancel with malicious token reentrancy
Testing Notes
Test the updated behavior in contracts/escrow/src/test_malicious_token.rs.

References
contracts/escrow/src/test_malicious_token.rs (around line 80)

Definition of Done
 Ready for review.


