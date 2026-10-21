# Deterministic Aster replay clock regression

The delayed-startup regression failed in [CI job 119402081578](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39651803685/job/119402081578)
during Hyperliquid Spot #1653. Its single authorized [rerun 119409243794](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39651803685/job/119409243794)
passed. Replace the test's wall-time dependency while preserving real clock
sync, signing, transport and request validation.

Aster's current private replay rewinds `Date.now()` at readiness while dayjs's
no-argument `new Date()` keeps wall time. A 1500 ms real delay and the first
command at 600 ms leave roughly 100 ms inside the validator's future bound.
The failed log lacks signed timestamp and oracle values, so this explanation
is based on the source rather than a measured timestamp from that attempt.

## Implementation and verification

Change only `sdk-rs/harness/asterdex-clock.test.mjs` and this plan. Use isolated
child processes for two controlled-time profiles:

- The original continuous-clock regression uses a test-only nonvirtual config.
  Inject 21000 ms during module setup and 1500 ms after real clock sync. Pace
  tape delivery explicitly and capture actual signed timestamps and validator
  time before canonicalization. Both must equal epoch plus startup plus frame
  time.
- The current virtual config retains real ReplayClock frozen startup, timers
  and readiness rewind. Make no-argument Date follow replay time, inject
  1500 ms of startup, and assert the rewind and exact command timestamp.

Scope injected performance time to replay.mjs; freezing the loader's own global
clock stalled the rejected prototype. Install controlled Date after imports,
before actual clock sync. Preserve explicit Date arguments and static parsing
and UTC behavior. Derive the epoch from the fixture. Keep real deadlines and
propagate pending-command rejection.

Keep timestamp, recvWindow and signature checks unchanged. Fail closed on
source replacement counts and profile/setup injection. Two negative controls
must fail through the real timestamp validator: remove the TS post-import
clockStart reset, or rebase the continuous signing clock at readiness. Retain
all original receipt assertions. Do not stub the signer or getCurrentTimeMs.

Require independent plan and implementation reviews, focused clock tests, the
full harness suite and fresh full-head CI with normal one-pair defaults. Honor
the #1656 merge hold, integrate its actual build-profile landing, then run a
fresh separate guard, merge, audit actual parents/files and report. Resume
Kraken and the remaining sweep afterward.

## Evidence and production scope

The controlled continuous profile observes the signed order timestamp and
validator time at epoch plus 2100 ms: 1500 ms startup plus the 600 ms command.
Removing the post-import reset produces a 21000 ms stale timestamp; rebasing
at readiness produces a 1500 ms future timestamp. Both mutations are rejected
by the unchanged real timestamp validator. The virtual profile observes epoch
plus 600 ms, after advancing startup 1500 ms and rewinding at readiness.

The live Aster clock computes `serverAtSync + (wallNow - clientAtSync)` in
`asterdex.virtualClock.ts`; the request signer reads that result in
`asterdex.api.ts`. The continuous profile exercises those methods with an
advancing client clock and shows no startup-induced offset error. The narrow
margin comes from splitting replay Date.now from dayjs's new Date. No analogous
live slow-startup defect was found in this inspected path; this evidence does
not assess network delay or unrelated clock failures.
