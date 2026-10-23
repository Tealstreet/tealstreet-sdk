# Socket subscription handoff before native read continuations

## Problem and scope

Landed OKX managed sockets issue private reconnect account refreshes from Ready.
Ready queues desired interests, but final drain appends their WsSend actions
later. The combined-action regression compiles and fails on the landed adapter:
HTTP index 2 precedes subscribe index 5. Before #1771, subscribe_private and
subscribe ran before account_refresh. Public OKX books use WS snapshots and do
not have this HTTP-seed path.

The first WOO adopter independently reproduces the same gap for dynamic book
subscription (HTTP 0, subscribe 1) and reconnect (HTTP 3, subscribe 7). Identical
cold baseline probes pass both controls on original 0768 and fail both on 27bc.
The WOO adoption remains separate and depends on this shared correction.

This PR owns shared subscription handoff custody and the narrow OKX consumer.
It does not change request recipes, signing, financial reducers, account refresh
selection, host routing, HTTP/Plan authority or the default-off feature boundary.
Bitunix remains excluded. No profile, CPU/memory threshold or size budget change.

## Continuation API and review requirements

A queued interest transaction is not a released subscription and a queued socket
frame is not a peer acknowledgement. Keep these three facts distinct. A native
read may begin only after its subscription frames pass the final handoff boundary.

The candidate API is an engine-owned bounded continuation carrying the venue's
existing Pending type, current socket epoch and exact admitted interest proposal.
The engine resolves proposal to committed revision. Native code does not retain
a second deferred-work queue, timer ledger or subscription sender. Admission
burns opaque entry freshness and reserves bounded custody atomically; failure
never leaves partial registration or reuses identities.

The final commit point requires explicit review before implementation. A full Cx
callback inside drain_socket_frames is insufficient: later dispatch/fencing, or
that callback itself, can retire the feed or supersede the revision, removing the
prerequisite WsSend while leaving its new HTTP output. Merely appending frames
before invoking a callback does not close this race.

Preferred review candidate: final-handoff custody followed by an engine-owned
zero-delay continuation wakeup, using the existing timer ingress/host ordering.
Only the final fenced batch can arm that wakeup after every prerequisite frame;
no native callback runs inside that batch. The later engine callback cannot
retroactively fence an already handed-out subscription. A timer alone is not a
receipt: epoch/proposal/revision and the final-handoff record must still match.
The wakeup is internal shared ownership, not a venue delay or polling workaround.
No host peer acknowledgement or new transport sender is introduced.

All paced subscribe/unsubscribe batches, repairs and no-op convergence are part
of the barrier; the first frame never releases a whole continuation. Keep exact
revision ownership through replacement, including replacement before drain;
superseded native work must not lose a still-needed book seed or seed a removed
and re-created book. Required wire interests and native generation binding need
explicit representation in the reviewed API. Refusal cancels only the new proposal's custody; retirement cancels epoch custody.
Each reservation is freed once. Do not infer send authority from numeric IDs.

Remove work before invocation; drain a bounded snapshot, preventing recursive
self-registration. Fresh clock/auth/signing/epoch checks precede invocation.
Clear HTTP delivery, command entry, order response and Plan/Preflight activation
loans; callbacks use only the existing HTTP APIs. Preserve all incoming fatal,
Plan flush, admission, owner-retirement, release/claim and whole-batch checks.
The no-work steady ingress path must remain allocation-free.

## Commit and verification sequence

1. Commit this plan first and get independent API/drain ordering review from
   uufub4. Settle the exact final-handoff mechanism before production edits.
2. Commit the assertion-only real OKX reconnect regression. Retain the original
   native working control and combined-action output evidence. Do not use a
   filtered WsSend comparison as cross-transport proof.
3. Implement the reviewed shared boundary and narrow OKX refresh continuation.
   WOO consumes the actual seam in its own venue PR after integration.
4. Add engine controls for pacing, signer delay, stale epoch, expiry, refusal,
   repairs, no-op, supersession and repeated registration. Include callback
   retirement/replacement and a genuine managed Prepare/Bind/Claim control:
   HTTP cannot outlive fenced prerequisite subscription frames. Exercise the
   actual host action/timer ordering used by the mechanism.
5. Verify callback custody isolation and caught-fatal refusal, feature-off,
   managed-sockets only and combined http-plans modes. Focused engine/OKX tests,
   strict Clippy/fmt, ratchets and preservation reviews precede fresh full CI.
6. Include caller/quality/audit sections. Report source-equivalent limits and
   CPU/memory/size gates honestly. Send the green frozen PR/head to the auditor
   first in combo-4. No merge without train GO;
   separate fresh scope/unowned/revert guard then head-matched merge.

## Concrete API candidate

`Cx::set_socket_interests_then(feed, desired, required_keys, pending)` performs
one atomic admission and returns the existing proposal ID. Existing
set_socket_interests/repair APIs retain their contracts. Required keys must be
unique members of desired; the engine captures their exact canonical Interest
values. The current epoch must exist. Pending uses the existing venue type,
with no new numeric terminal or HTTP authority. The combined proposal/pending
count and retained interest bytes share bounded credits; a wakeup TimerId comes
from the landed checked allocator only after every preflight succeeds.

The proposal carries the pending record until commit; failure cancels only that
new record. A refused replacement/repair preserves existing still-needed work
on the unchanged committed revision. A committed replacement/repair rebinds the continuation to the latest revision only
while every required Interest is unchanged. A removed required key cancels the
work immediately, even if a later proposal re-adds it. This prevents a removed
and re-created book from inheriting an old seed. Unrelated additive interests
retain needed work, which waits for the newer full convergence. Replay also
updates the bound revision for the same current epoch. Native WOO declares its
book topic; OKX declares the fixed private interests for its refresh.

`Venue::on_socket_interests_released(cx, epoch, pending)` is default-noop and
runs on an engine-owned wakeup after the earlier handoff, never inside frame
drain. Final drain arms it only after dispatch/fencing has settled, with a
read-only fresh readiness/expiry/signing/revision check and no remaining frame
batches. Arming adds only the ordinary SetTimer action; it invokes no callback
and cannot mutate the socket. The host processes that timer after the preceding
WsSend actions. The wakeup validates the sealed epoch/revision receipt, consumes
and frees custody before invocation, and uses the same event/fatal/retirement
rails as ordinary socket ingress. Early, duplicate and cancelled timers are inert.

If another proposal commits between handoff and wakeup, the engine cancels that
wakeup, applies the required-interest rule, and waits for a new final handoff.
A refused proposal preserves/rearms work on the unchanged committed revision.
No callback can retroactively fence the prerequisite frames from an earlier
returned batch. Callback-created work is ordinary HTTP under normal contexts;
it does not inherit a command/response/Plan/Preflight activation loan.

No new host wire action or peer ACK is introduced. Tests compare concatenated
ordered host batches, firing only the actual engine wakeup, and retain the exact
native baseline comparison. The subsequent engine ingress is a caller-visible
timing change and must be documented; no timeout or account policy is enlarged.

The existing JS host passes two focused handoff controls using the actual
EngineHost: both ordinary deferred timers and injected inline timers call the
socket sender before arming and executing the continuation that issues HTTP.
The new host test uses the host-unit filename glob, so the ordinary CI command
picks it up. This proves host action ordering only; it does not establish the
new engine receipt, cancellation or managed HTTP authority checks.


## Reviewed timer-boundary implementation

The API/drain review of 4583d233556 plus 8739ef44fcf cleared implementation before
the separate runtime review. Arming retains the original drain body verbatim
and adds only a read-only continuation check after its final safety assertion.
No native callback executes at final drain. A fresh wakeup ID is reserved after
rebasing an already armed revision, so a late old event cannot consume new work.
The empty continuation path performs no allocation or additional batch scan.

The actual EngineHost controls also expose and repair inline timer handle
bookkeeping: an inline-fired arm must never be inserted after its callback,
and cleared/superseded callbacks cannot delete or invoke the current arm.
The ordering promise is a subsequent engine ingress after host handoff, even
when a host's timer implementation calls back inline; it is not an event-loop
or peer-ACK guarantee. Three controls are assertion RED against the old host.

Early host one-shot delivery must explicitly rearm the retained TimerId at its
unchanged handoff time; keeping an armed record alone is insufficient because
the host has consumed its timer. The later matching event invokes once and frees
credit. A real Engine assertion RED plus actual-host inline rearm/duplicate
control verifies this boundary without enlarging the deadline.
