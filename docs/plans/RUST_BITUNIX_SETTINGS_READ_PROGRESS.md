# Bitunix settings progress during position notifications

Fresh landing at `501fb664edb6769bde4a2337bde56fc44d6b5724`, independently of
pending balance PR #1665. The sweep's WS-generation starvation family still has
one ordinary read path: per-symbol leverage/margin metadata captures the global
position notification generation. A position notification on any symbol can
invalidate a valid settings response, suppress its auth evidence and defer it
again, leaving flat-symbol defaults indefinitely under steady position traffic.

1. Reproduce settings publication and scoped-auth starvation with native engine
   tests, including another symbol's continuous position notifications. Show the
   original source fails before changing its admission predicate.
2. Capture the existing accepted-position-settings write revision with each
   settings attempt. Require exact symbol attempt serial and write revision in
   publication, auth evidence and retry success/failure. WS-only notifications
   must not invalidate it; accepted leverage, margin and position-mode ACKs still
   supersede older reads. Refused/unproved writes do not advance the revision.
3. Preserve canonical market admission, grouped per-symbol refusals, bounded
   retry/rate pause, actual signing identity and empty-margin-mode fallback.
   Regress stale callbacks, successful recovery across overlap, failed/malformed
   reads during overlap, and current accepted-write controls. Never retry writes.
4. Independent plan/implementation/adversarial reviews; Bitunix+Binance focused
   native tests, strict Clippy/format/bans with four Cargo jobs, then draft/full CI.
   No owner/live/protocol exclusions or financial mapping changes. No budgets,
   local release WASM, replay matrices, timing gates or load tests.

The current balance PR and this branch share a venue crate but have no unlanded
source dependency. Any eventual merge must integrate actual current landing,
including #1665 if it lands first, rerun focused/full CI and the separate exact
scope guard, and report the actual merge pin under the lane rules.
