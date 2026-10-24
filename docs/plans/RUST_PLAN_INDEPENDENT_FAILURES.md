# Independent Mutation Plan calls after an unproven HTTP response

## Problem and scope

A released managed write receiving HTTP 408 or 5xx is intercepted before the
venue DATA codec. The engine records its unresolved fact and native evidence,
then seals its whole Plan. This prevents a native continue_on_error batch from
attempting independent siblings, unlike provider refusal or transport loss,
which already reach typed on_http_failed and permit finish_plan_failed_call.

The overseer requires the same continuation contract for independent BingX and
Bybit batches, including placements and cancels. The final result remains the
shared journal projection: Unknown if a released leg is unproven, preserving
bounded known IDs. Continuing a sibling grants no retry of the uncertain leg.
Dependent cancel/recreate or amend steps must retain whole-Plan abort behavior: a
handler declines to settle that call, even inside an opted-in Plan.

This is a separate engine follow-up on #1932; that reviewed head stays frozen.
No venue dispatch, classifier table, fixture, reference, budget or ratchet edit
belongs to this core change. BingX and Bybit adopt the flag in their child PRs.

## Admission and delivery contract

Add public PlanPolicy.independent_calls: bool. Existing literals explicitly use
false. Admission copies it into the immutable Plan body. True is valid only for
Mutation Plans; a Read Plan is refused with Invalid before IDs, credits, owner
phase or timers change. It permits the native handler to settle independent calls; the venue still
recognizes dependent follow-ups and declines settlement for them. It cannot be
toggled after admission or used to bypass dependency semantics.

For an exact released delivered 408/5xx, record the existing unproven fact and
UnprovenEvidence IDs before any callback. If the Plan declared independence,
route that same failure through with_owned_read_failure and the existing typed
Venue::on_http_failed. The wrapper marks the original call failed. The venue
may finish_plan_failed_call and synchronously queue its next independent call.
The callback error has Unknown outcome, Timeout kind for 408 or VenueUnavailable
for 5xx, and the captured evidence IDs. No response DATA codec runs and no failed
call Store row is published. Like transport loss, this is uncertainty for the
final seal and never a fresh order read. No http_write_reply authority is installed, so a
second venue write fact or resend grant for the failed call is refused State.

The wrapper tests the ORIGINAL failed delivery after the callback. If that
call was not settled, the existing fail_plan path seals it once and retires the
unissued suffix. Issuing a sibling inside the callback must not cause that
sibling to be auto-sealed. Flag-false Plans and standalone Context writes retain
the current direct fail_plan/fail_context behavior and callback exclusion.

No new Venue hook, timer, owner, journal, error projection or retry policy is
introduced. Read Plans never enter this path. Late or duplicate responses for
a settled attempt remain fenced before both typed callbacks and reconciliation.
The #1857 uncertain seal still queues reconciliation once at the final owner
seal, rather than once per independent leg.

## Original controls and verification sequence

1. Commit this plan and an explicitly inert declaration scaffold (the field and
   old false literals only), then execute assertion REDs against unchanged
   delivery behavior. Missing-field compilation failures are not RED evidence.
2. A genuine Preflight admits a serial independent Mutation Plan. Drive real
   Prepare/Sign/Bind/Claim. For both 408 and 503, capture an evidence ID, settle
   the failed call in on_http_failed and synchronously submit its sibling. The
   sibling reaches Prepare, succeeds, and the single final Unknown retains IDs.
   Assert the callback error kind/outcome and captured partial IDs too.
3. A handler that declines settlement gets exactly one Unknown terminal, no
   sibling request, and retirement of the unissued suffix. Keep a false-flag
   compatibility control proving the old no-callback abort remains unchanged.
4. During the callback, attempts to record another write fact or obtain a grant
   for that call return State. Its captured fact/evidence already exists.
5. A Read Plan with true returns Invalid without changing phase, credits, IDs,
   timers or request output; false remains a working Read admission control.
6. Deliver duplicate/late 408/503 after the call settled. Neither the typed
   callback nor reconciliation runs again. With #1857 integrated, assert exactly
   one reconcile_uncertain callback at the final Unknown seal, none per leg.
7. Run focused tests, strict feature-on/off clippy and fmt. Have classifier/Bybit
   owner claude-dazzy3 independently review source and the native consumer fit.
   Publish a small PR with a CALLERS entry, fresh normal CI, then request a window.

The #1857 reconcile hook is absent from the initial #1932 branch parent. Its
actual combo landing must be integrated before claiming the per-seal reconcile
control passes; no fabricated hook or inherited test result substitutes for it.

## Inert declaration checkpoint

On the #1932 parent with only the inert field/false-literal scaffold and these
tests, six focused controls compile and run: four assertion REDs (missing typed
callback, no-settle callback, duplicate/sibling path and Read admission) and two
GREEN controls (default direct abort and ordinary Read admission). The RED log
is local `/tmp/codex-plan-independent-original-red.log`. No gateway delivery
behavior changed at this checkpoint. The per-seal #1857 reconcile assertion is
still pending genuine integration of that prerequisite.

## Narrow routing checkpoint

The unchanged six control bodies now all pass on the routing correction: captured
failed-attempt error is passed into the existing owned-failure wrapper only for
an opted-in Mutation Plan, and Read opt-in is refused in admission validation.
The source does not add a hook, retry, journal or timer. The final-seal reconcile
control and fresh full CI still require actual combo-3b integration.

Independent review found an overflow edge in the initial routing checkpoint:
error retrieval reused the stronger settlement gate and skipped typed cleanup
when native evidence exceeded its ID budget. Original assertion RED 6a6efe66725
reproduces it. Callback error retrieval now retains the bounded IDs and overflow
marker independently of settlement; finish_plan_failed_call still refuses State,
so the wrapper auto-seals once and never submits a sibling after overflow.

First-consumer hardening adds an unowned-delivery regression. Assertion RED
24dcc857e2e uses a real released Plan but no callback delivery: the prior wrapper
lent the callback and left the Plan unsealed. Independent routing now records
owned failure before lending the callback; refusal goes directly to fail_plan.
The common recorded-failure tail is shared with existing typed failure paths,
and still checks the original delivery after callback settlement.

## Actual write-reconciliation integration

The branch genuinely merges landed #1857 at 867896f3e25 (merge b6592329cfb).
Independent preservation review proves all 5,462 unowned paths exact incoming,
with stable source patch IDs for both sides. The sole CALLERS append conflict
keeps incoming notes followed by the independent-call section once.

The required final-seal control now passes on that actual engine hook: two real
serial Prepare/Sign/Bind/Claim writes receive 408 and 503, the first settles and
the sibling releases with zero reconciliation callbacks, and the final Unknown
seal retains IDs and invokes reconcile_uncertain once. Late replies and a late
HttpFailed neither invoke failure cleanup again nor request reconciliation.
The hook has no HTTP delivery, command entry, order-response token, write reply
or Plan-readiness custody. This is an engine hook receipt, not an exchange read
completion. Current focused matrix: nine controls with order-locks; eight without.

## Physical execution composition

The branch merges frozen #1919 `d7e7593a967` after actual #1932. The physical
Plan journal is resolved through `owner_entry(plan.owner())`, preserving issuer
and generation checks for Command owners and the exact physical Execution
identity. The Mutation-only immutable flag, recorded-failure custody and bounded
fallback remain unchanged.

Original assertion RED `5cb44207583` drives a physical Plan through real
Prepare/Sign/Bind/Claim with a founder and follower. A delivered 408 or 503
produced zero failure callbacks and aborted its sibling before the correction.
With the owner-generic lookup, the same test settles that call, releases the
sibling, and projects the same final Unknown with both evidence and accepted IDs
to each observer exactly once. The physical owner retires once; late replies
change neither terminals nor callbacks. Ten focused controls pass with order-locks.

## Frozen planned-execution restack

The branch plain-merges frozen #1919 `2f4a08b449c` at `712d2e5bbda`.
Readiness and Session fixtures, plus incoming venue Plan constructors, explicitly
retain `independent_calls: false`. The caller documentation keeps the incoming
file followed by this change's unchanged suffix.

The existing physical founder/follower control exposed a semantic conflict:
the Session integration restricted reconciliation capture to Command owners,
so an Execution-owned Mutation's 408 reached the DATA codec. The merged source
produced nine passing controls and that assertion failure. The correction
includes both Command and Execution Mutation owners in reconciliation capture;
Session writes remain excluded. Release metadata, admission and write-mark
ordering remain on the existing shared path.

The unchanged independent controls now pass: ten with all features, nine with
only `http-plans`; none are ignored. The #1919 owner cleared this restoration
of pre-Session coverage, including non-physical Execution legs.
