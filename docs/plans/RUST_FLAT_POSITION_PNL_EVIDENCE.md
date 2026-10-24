# Flat-position realized PnL evidence

## Problem and economic contract

The native frame encodes unavailable position realized PnL as NaN. The shared
`flatPositionPnl` projection admits that value because `NaN != null`, then maps
it to null. An equivalent TS position with undefined PnL produces no row.
Consequently a flat COIN-M placeholder creates fourteen declared differences
across the final account and six checkpoints without reporting any actual PnL.

Keep flat-position evidence only when the existing `finiteOrNull` conversion
returns a finite value. Actual zero, positive and negative PnL remain evidence;
finite numeric strings retain their existing conversion. A real PnL mismatch or
loss must still fail comparison. Non-flat positions and native state are outside
this correction. Fill prices, isolated wallets, IDs and terminal statuses that
correctly differ from TS remain declared.

## Scope and sequence

1. Commit this plan before controls or implementation. Independently review the
   shared projection, frame sentinel and exact diagnostic identities.
2. Commit original failing controls in `financial-evidence.test.mjs`: unavailable
   NaN/infinity/non-numeric values must not manufacture flat PnL rows. Keep
   working zero/signed/numeric-string, genuine-mismatch and non-flat controls.
3. Change only the admission predicate in `comparison-policy.mjs`, using its
   existing conversion rather than a venue-local rule or truthiness shortcut.
   Add the CALLERS contract and nearby quality/audit records.
4. Publish a draft with focused checks and full CI. Use its fresh Binance-family
   raw correctness outputs to authenticate every resolved inverse diagnostic.
5. Prune only the fourteen `flatPositionPnl` keys for inverse tape SHA
   `a0a35640cbdb5fdd8f43a14188cc4113290683b1e625e131f6da0e9b8b38f94b`.
   Retain all other declarations and economic fields. If fresh output does not
   prove each key absent, do not prune it. Refresh CI on the pruning head.

## Verification and risks

Run scoped `node --test` checks for financial evidence, comparison policy,
snapshot comparison and declaration policy, plus syntax/format checks on the
changed harness files. Full replay certification runs in CI. Independently
review the implementation and exact manifest diff before marking ready.

This shared correction affects every family using flat-position projection.
Missing evidence must remain distinct from zero; actual negative PnL, numeric
strings and genuine cross-implementation mismatches are mandatory controls.
Do not alter fixtures, native producers, thresholds, CPU methodology, profiles,
request inventory or unrelated comparison policy. The open-PR file map at this
plan's start has no edits to the two scoped harness files.
