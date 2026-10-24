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

## Bounded raw evidence

Baseline run `40037292055`, artifact `12538002330`, is measured on a checkout
whose tree equals actual #1902. Corrected run `40040892521`, artifact
`12539701275`, measures reviewed #1944. Fresh artifact metadata, archive digests,
producer jobs, full upload logs/windows and checkout parents/trees bind both
roles. The full raw objects compare equal after excluding only top-level
`metrics` and `protocolErrors`. Baseline TS four and Rust five errors disappear;
all sixty other ordered failures and fixers remain exact, with five writes,
nine checkpoints and zero unserved/missing-write/invalid-signature/unused-write
failures in both receivers. The strict private report remains FAIL.

Receipt: `/tmp/codex-cryptocom-replay-evidence/proof.json`, SHA256
`c31e42bf8c3479d77ed65f41f93cffb5b30978c1aa3d639bcb8b9f8036046dc8`.
This is one pair per role, not five-run determinism or current-head CI.
Independent control receipt: `/tmp/codex-cryptocom-controls-review.json`, SHA256
`abe77ce2625824ead0a6b3772b0bcf874733bcb1c79f274ab098a94c867ed97b`.
The control asserts the final healthy result before the checkpoint result, so
the original RED proves persistent final errors rather than an interim-only
reporting difference.

Independent raw review CLEAR: receipt
`/tmp/codex-cryptocom-replay-evidence/independent-review.json`, SHA256
`48ba6214cbf377a4e62656fe687e9628744e9afff47b5564c33ff708b057585c`.
Only the exact current-tape nine-key protocol group is removed (811 bytes).
All other manifest bytes are preserved. The real suite verifier reports two
strict declared FAILs (public and private), zero unexpected failures and zero
unobserved declarations; its public case was already a declared failure, rather
than the public PASS assumed by the first local summary assertion. Correcting
that local assertion changes no report, declaration or verifier behavior.
Crypto/declaration-policy controls pass 20/20; the separate capture controls pass.
