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
explicit representation in the reviewed API. Refusal/retirement cancels custody
and frees each reservation once. Do not infer send authority from numeric IDs.

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
   for combo-3b first if ready, otherwise combo-4. No merge without train GO;
   separate fresh scope/unowned/revert guard then head-matched merge.
