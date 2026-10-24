# Crypto.com tape-time replay diagnostics

Baseline: actual landing `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Development branch: `test/rust-cryptocom-replay-diagnostics`.

## Selection and overlap

Crypto.com has nine current protocol declaration keys, more than any remaining
untouched candidate. Binance Spot has four; WOO and GRVT already belong to
parked #1944 and #1998. The current private tape hash is
`4f8906f32518d5ef67894073bc93b78897dda4aa6a66cb5e9738b8e41e6894fd`.
Its 69 keys include 25 execution fields, nine hedge-mode fields, 26 caller client
identities and nine missing-subscription diagnostics: four TS channels and five
Rust channels, whose contract additionally requires `user.trade`.

The refreshed open-PR file map covers all 78 PRs, including complete paginated
lists for #1977 and #1971. No open PR changes Crypto.com's tape generator, the
shared audit in `venues.mjs` or the proposed new audit test. The only shared
manifest/capture overlaps are #1944 and #1998; both preserve the complete
Crypto.com case. Receipt: `/tmp/codex-cryptocom-replay-overlap.json`.
Native history #1999 and account/trading #1788 stay outside this slice.

## Plan and controls

1. Commit the plan before controls. Use the real audit with independent valid
   HMAC authentication, acknowledgement, exact per-implementation subscriptions
   and heartbeat replies. Keep TS's four-channel and Rust's five-channel
   requirements distinct.
2. Compare healthy completion with a checkpoint observation before auth ACK.
   Reproduce original assertion REDs against the exact behavior-preserving
   pre-correction capture module. Reuse #1944's shared final-capture helper only
   when needed; do not stack on unrelated #1998 or copy the capture policy.
3. Preserve actual invalid signatures, early subscriptions and unacknowledged
   heartbeats as final failures. A valid later lifecycle must not erase genuine
   earlier faults. Audit checks and fixture/tape records stay unchanged.
4. Authenticate fresh CI raw evidence for this exact tape. Compare full objects
   outside metrics and protocol errors, ordered strict failures and all remaining
   declarations. Prune only individually absent keys after independent review;
   preserve every other manifest byte and all historical/public entries.

## Publication and qualification

Scope is a new focused harness test, this indexed plan, exact proved declaration
pruning and nearby reporting docs. No native source, socket/signing policy,
fixture, tape, CPU threshold, profile or workflow implementation changes.
Publish a worthwhile independently reviewed batch as a draft. The existing
matching sweep branch supplies five private pairs without changing CI policy.
Actual #1944 integration and fresh composed CI are required before final
qualification; reviewed source evidence does not claim its landing. No merge
without a window, separate fresh guard and head-matched merge.

## Local controls

The original unconditional capture module at `d59643b0b16` produces two assertion
REDs: the pre-ACK checkpoint persists four TS and five Rust missing-channel
errors. Ten working controls remain GREEN, including invalid HMAC, pre-ACK
subscriptions, a genuine late missing heartbeat and the Rust-only `user.trade`
requirement. The same twelve controls pass with reviewed #1944 capture source;
no audit policy or native source changed. Original log:
`/tmp/codex-cryptocom-original-capture-controls/original-capture.log`.
