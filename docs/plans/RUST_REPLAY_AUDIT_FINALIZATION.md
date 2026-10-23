# Finalize replay protocol audits at the final capture

Baseline: actual combo-3b `b9d8142261f5f8d0823f76842f36a34840559ae9`.
Standalone harness follow-up after #1902; never part of a feature combo.

## Diagnosis

Both replay implementations call the protocol audit's terminal `errors()`
method inside the capture also used for interim checkpoints. The checkpoint
projection discards `protocolErrors`, but WOO's terminal coverage aggregation
has already recorded missing heartbeat/authentication/subscriptions permanently.
Startup readiness permits the first checkpoint after a valid auth request and
before its acknowledgement, so observing a checkpoint can poison the final
result even when the complete lifecycle is valid.

The original exported-audit probe has four working controls and two assertion
REDs. It reproduces the exact four historical private diagnostics per TS/Rust
implementation. Genuine data-before-auth failures remain recorded. Independent
causal review: `/tmp/codex-woo-replay-ordering-causal-review.json`, SHA256
`691072012e6ceb8c85a2b7a6edf1cea0d4a63348d116d1aeac4c352832fa16fa`.
This proves the observer effect, not complete final lifecycle coverage for every
retained tape. The accepted-write snapshot race stays with queued #1788.

## Implementation and controls

1. Extract the existing unconditional protocol aggregation into one shared
   capture helper, retaining behavior, and wire both implementations to it.
   Commit the original two assertion REDs and four working controls before the
   fix. Add a genuine late private-error control.
2. Invoke terminal protocol aggregation only when capturing the logical end of
   a replay. Interim captures must not invoke it. Both implementations use the
   same helper; the audit validators and genuine sticky violations stay intact.
   Preserve the existing capture-before-memory boundary and fail closed on any
   final audit error.
3. Run focused finalization/checkpoint/protocol controls and independent review.
   Use fresh full replay CI. Prune any of the eight known declaration entries
   only after fresh raw proof individually establishes that it is unobserved.

No tape, fixture, CPU threshold, build profile or native venue changes. Final
protocol failures remain failures; interim diagnostic reads confer no success.
Carry the caller-visible reporting change in CALLERS. Send the frozen head and
fresh run to the auditor for standalone placement after #1902; no merge without
that window and its separate guard.

## Fresh declaration evidence

Frozen `2a046154796` / Rust run `40008675562`, WOO physical job
`120603063447` attempt 1, exercises current private tape
`9d71837d4fc35be11d2b84fccb60c036880aa5d930f7d675469ca7302cbfb687`.
Both correctness raw outputs have empty `protocolErrors`; every one of the eight
TS/Rust protocol keys is absent from the fresh gate. The other 554 failures
remain identical and sorted; private status remains FAIL. Strict artifact
`12525516902` and package `12526035440` are authenticated against physical
uploads, source/tree and payload hashes. Independent per-entry evidence receipt:
`/tmp/codex-replay-audit-1944-woo/audit-review.json`, SHA256
`b6b11dd93245b0e33706d4141adddb5d08923350ee74906f80634952c938c2bb`.

Remove only that exact tape's eight now-unobserved protocol declarations, as one
complete diagnostic group. Preserve all other manifest bytes, including the
historical `72a4ee4e` WOO tape and separate public book-presence keys. This is a
reporting correction, not complete private financial parity.
