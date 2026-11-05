# Hyperliquid native protective modify

SDK, NX and hosted callers retain the existing `AmendOrder` contract and original
five-second preflight deadline. Same-kind SL/TP edits use native modify with the
source order's execution parameters; callers must use the returned native or
pending identity, which can differ from the old id. Native venue refusals are
`Rejected` with the venue reason and “order unchanged”. Pre-send identity, size,
fresh-mark, fill and deadline refusals remain `NotSent`.

Kind-changing fallback requires proven full-position `isPositionTpsl` grouping
and a loaded live position. Unproven partial/attached conversions refuse before
cancellation. After a definitive replacement refusal, one original-parameter
restore is allowed. Unknown replacement outcomes must reconcile; callers must
never retry, restore or resend automatically. The existing 334 ms pacing and
nonce custody apply to every physical leg. No read or optional fee lookup starts
between cancel and replacement/restore.

Render the typed protection outcome: `Restored` means the original parameters
were accepted again as a new order; `Unprotected` means restoration failed or its
outcome is unknown and requires immediate attention. A closed position or newer
protection can retire an unsent restore. The final claim rechecks this eligibility
and the fresh mark. Sibling protection remains independent.

This changes no engine API, public subscriptions, routing or host signing policy.
It supersedes the cancel/place description for same-kind protective edits only.
There is no caller migration or deletion prerequisite. Full-position conversion
must remain refused when native placement grouping cannot be proven.
