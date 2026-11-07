# Plan root-expiry cleanup

Owner: codex-uufub4. Separate engine fix; WEEX managed trading is a dependent
consumer. Base: landed 2f85168cfc7. No venue recipe, retired-hook authority,
execution ownership, timer bound or retry policy changes.

## Reproduced defect

WEEX checkpoint 57948baf852 has a real managed native control:
`root_expiry_cleans_provisional_rows_before_late_ack`. The Plan timer at the root
bound seals the journal, discards Pending and skips typed failure cleanup, so a
Creating Store row remains. The HTTP-timer-first control cleans the same row.
The restricted retirement hook cannot lawfully repair Store.

## Correction

1. Before sealing an expired Plan, drain its outstanding managed calls in stable
   request-ID order through existing `expire_http_timer` typed failure paths.
   Unreleased work gets preparation failure/NotSent; released work gets typed
   Timeout and the existing journal projection. Every native callback runs once.
2. A private exact-Plan expiry barrier defers `fail_plan` requests made by those
   callbacks until the drain finishes. The journal remains the single writer;
   no local outcome ledger or new completion authority is created. Recursive
   readiness flushing is suppressed during this private drain. Expired Context
   and Plan validation continues to refuse slot issuance, queueing and release.
3. A scope guard poisons the engine if a cleanup callback unwinds; it never
   restores custody or converts panic into success. The normal path clears the
   barrier only after every pending call is cleaned, then seals once using the
   shared prepare/commit terminal funnel. The retired hook retains no Cx.
4. An expired HTTP timer or host preparation failure that arrives first delegates
   to the expired Plan drain before its individual callback can retire peers.
   This check is cold and only applies to an exact live Plan past its root.
5. Keep original call deadlines, late-reply fencing, journal IDs and unsent versus
   released outcomes. No new timer, reservation, retry or live-event scan.

## Original REDs and validation

Engine controls cover root-timer-first and HTTP-timer-first; preparation,
provider-pending and released phases; parallel optimistic Creating/Canceling/
Updating rows; callbacks requesting `fail_plan`; no new send at the expired root;
exactly one terminal/retirement and no second callback from late host events.
A callback-panic control checks permanent poisoning. A working non-expired
failure control preserves the ordinary callback path. Run focused engine tests
with http-plans on, strict clippy with plans on/off, then the actual WEEX native
control after integrating the reviewed fix. Independent source review is a gate.

## Implementation checkpoint

Original engine controls at ccbfe9ef099: four assertion REDs and one ordinary
pre-root typed-failure GREEN. The implementation uses the same extracted seal
origin validator for deferred and ordinary terminals; deferral cannot accept a
foreign issuer or mismatched delivery attempt. The drain rechecks each current
managed token through `plan_from_context` and picks the lowest request ID without
allocating a second request ledger.

Seven focused controls are GREEN, including mixed unsent/released peers and an
accepted prefix before unresolved siblings. Existing active-Plan25 and boundary4
controls pass. Strict engine clippy passes with http-plans on and off. Independent
source review is CLEAR; full fresh CI and actual WEEX consumer replay remain PR
gates. No merge or whole-repository proof is claimed by this local checkpoint.
