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
Dependent cancel/recreate or amend steps must retain whole-Plan abort behavior.

This is a separate engine follow-up on #1932; that reviewed head stays frozen.
No venue dispatch, classifier table, fixture, reference, budget or ratchet edit
belongs to this core change. BingX and Bybit adopt the flag in their child PRs.

## Admission and delivery contract

Add public PlanPolicy.independent_calls: bool. Existing literals explicitly use
false. Admission copies it into the immutable Plan body. True is valid only for
Mutation Plans; a Read Plan is refused with Invalid before IDs, credits, owner
phase or timers change. It declares independence for every call in the Plan;
it cannot be toggled after admission or used to bypass dependency semantics.

For an exact released delivered 408/5xx, record the existing unproven fact and
UnprovenEvidence IDs before any callback. If the Plan declared independence,
route that same failure through with_owned_read_failure and the existing typed
Venue::on_http_failed. The wrapper marks the original call failed. The venue
may finish_plan_failed_call and synchronously queue its next independent call.
No response DATA codec runs. No http_write_reply authority is installed, so a
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
