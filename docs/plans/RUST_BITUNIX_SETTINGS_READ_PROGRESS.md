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

Verification: three original-source regressions fail for defaults remaining10x,
missing scoped rejection, and hidden real retry failures. The repaired four
regression groups pass, including primary/secondary credentials, same/other-symbol
WS overlap, malformed/transport retry progression and accepted/refused/malformed/
transport-lost leverage/margin/mode ACK controls. Publication-only and auth-only
write-fence-disabled controls each fail; restored461 Bitunix/Binance native tests
and strict Clippy/format/bans pass. Two independent correctness/adversarial
reviews are clear. No private-read/preflight generations or financial mappings
changed; metadata response/failure/auth consistently use accepted-write revision
plus symbol attempt serial. Fresh full CI and actual same-crate integration if
#1665 lands first remain mandatory before any guarded merge.

Actual balance integration:
- #1665 landed at `8f7b0cee3ff364edc9fb0691d3809abbaea5919e` and is now
  genuinely merged into this branch. Only an adjacent CLAUDE paragraph conflicted;
  both balance and settings explanations are retained.
- All incoming files outside the six own paths match that landing exactly.
  Independent preservation review confirms balance serial/write-revision fencing
  and settings serial/accepted-write fencing coexist.
- The integrated tree passes all 470 Bitunix+Binance native tests, strict Bitunix
  all-target Clippy, format and banned-pattern checks. Fresh exact-head full CI
  is required; the earlier pre-integration CI is historical only.
