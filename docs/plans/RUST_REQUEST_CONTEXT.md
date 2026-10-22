# Engine request lifecycle and release admission

Spec/plan, 2026-10-22. Fresh base: actual Aster rollback #1734
(`b95cad5a1003d57cdc89bfa117c92b47d3a4c61f`), including signer prerequisites,
scheduler admission, shared protocol clock and integer wire stamps. Re-verify
and integrate queued financial #1736, history #1735 and Bybit signer #1728
before final adapter review/CI; Bybit migration supplies the temporary refusal
flags/resent fallback that this core replaces. Binance and Bybit are the outcome-aggregation references.
This extends the existing executor, signing queue and pending/order locks.

## Contract

Every managed HTTP attempt carries immutable engine request context: command or
background owner, semantic read/mutation class, permitted retry evidence, chosen
credential slot/ref (never material), logical/physical attempt identity, generation
and original raw lifecycle deadline. POST does not imply mutation; GET does not
prove retry safety. Credential subject/nonce selection remains its separate core.

One engine lifecycle owns queued, signing, released and settled states. It rejects
late/duplicate/cancelled generations, retains only unsigned retry recipes and
reprepares signing on each permitted resend. A retry never extends the original
raw deadline or re-mints an order/client id. Unknown writes never enter generic
retry; venue codecs provide definitive refusal/ACK facts, not transport policy.

Outcome sealing uses release facts: an undispatched attempt is `notSent`; an
unproven released mutation is `unknown`; a validated definitive refusal is
`rejected`. Command aggregation must never relabel an earlier accepted or uncertain
write as `notSent` or a whole-command rejection when a later leg fails. HTTP status
alone is not refusal evidence. Engine release means potentially sent; only a typed
transport fact may prove zero dispatch afterward. Preserve native
partial identities and existing terminal error/receipt delivery exactly once.

At final fresh HTTP release, **plain and signed** use the same admission hook.
Proposed seam: no-`Cx` `on_http_release(pending, HttpRelease)` returns a typed
permit/denial. Release facts include request/attempt identity and raw release time;
a permit supplies an optional absolute response lease, capped by the original
lifecycle deadline. No protocol/calibrated time controls scheduling. The final controllable host
transport-dispatch point, after asynchronous route/token preparation, defines
release; action drain alone cannot prove transport start. Keep this extension
in the existing executor, with a bounded host readiness/release handshake, not
a second sender or a per-venue callback queue.

The adapter only maps its pending read token into shared `Scheduler::admit(token,
raw_now) -> Result<Option<f64>, Error>` (#1723). First release starts the logical
response lease once; parallel legs/pages validate that live token without issuing
it twice or renewing it. Only validated response progress renews a lease. Every
new release consults pause/revocation/expiry, including after asynchronous signing.
A pre-send denial is distinct from network failure and must not double-count
credential evidence, failure backoff or completion. #1721 signing receipts remain
metadata-only and prove neither transport delivery nor venue acceptance.
Admission runs after freshness/deadline/preflight checks, before receipt publication.
Semantic operation class also replaces method inference in the order-lock fence.

Admission distinguishes `Unscheduled`, `Scheduled(validated lease)` and `Denied`.
The shared scheduler adapter maps `admit == None` to denial, never permission to
send without a lease. Validate custom lease arithmetic before admission commits;
there must be no fallible lifecycle check afterward. The release receipt's lease
is an observation, not a physical HTTP timeout: the scheduler alone owns renewable
logical-read expiry. Progress on one in-flight leg must not expire a sibling at
an older lease snapshot. The immutable lifecycle remains the unconditional cap.

### Release clock projection and pacing

The shared actual-release receipt captures raw scheduling time and protocol UTC
from the same release observation, including the calibration revision and the
actual signing stamp/expiry. A protocol-UTC signature expiry must never be compared
directly with raw `Cx.now`. Project its remaining duration onto raw release time,
cap the resulting deadline by the original raw lifecycle deadline, and reject or
reprepare expired/stale signatures before release. Phemex's repair `signed_until`
must consume this shared projection rather than copy protocol expiry or maintain
a venue clock mirror. Test positive/negative calibration offsets and calibration
changes while signing is parked; preserve raw timeout precedence.
Keep projected signature validity separate from the response lease: an accepted
response can arrive after the signature's send window ends.

Venues declare a send group and minimum gap; the shared lifecycle enforces that
gap at final controllable transport dispatch and advances next-eligible time
only there. Engine action drain alone is insufficient: native route/token/proxy
setup can await afterward. Define a host release handshake or equivalent shared
contract, retaining engine admission/fencing without a per-venue transport hook. Queueing or
parking at a signer consumes neither a send slot nor a response lease. Waiting
for pacing must preserve the original deadline and repeat signature freshness
checks before sending. MEXC's existing `advance_trade` queue-time
`next_submit = cx.now + max(gap, 450)` moves to this shared boundary. Original-RED
acceptance: park the first provider, release it, deliver a fast ACK, then release
a second order; physical sends remain at least 450 ms apart, even though the
first order spent longer than the gap parked. Cover cancellation, timeout,
concurrent releases and refusal without consuming a physical-send timestamp.
Delay the first host route/authorization setup and make the second fast; actual
physical sends must still retain the gap rather than bunch after engine drain.
No local SignStep/apply/receipt pacing workaround. Trusted MEXC credential setup
normalization is owned separately by codex-5c57jl, outside this PR.

## Transport handshake and permit boundary

Use distinct route-preparing, prepared/ready, signing, permit-issued, dispatched
and settled states for each physical attempt. A ready message proves only route
preparation. It consumes neither a read lease nor a pacing slot and never proves
sent. Freeze configured `request_timeout` at the first dispatch preparation,
measured from logical creation; later attempts and route/sign preparation retain
that original raw upper bound. A later timeout adjustment may shorten it only.

Extend the existing native/JS/Workers executor with preparation and a synchronous
final claim/handoff. Finish route selection, proxy authorization and HOW setup
before final permission. Prepared transport state stays in its host; the engine
gets only an opaque preparation identity and optional raw expiry. Bind a
single-use permit to logical leg, physical request/attempt, generation, credential
route, prepared transport, raw deadline and protocol revision. If delivery stalls,
revalidate at the final claim: expire, reprepare or re-sign within the original
deadline; never send an old permission merely because it once passed a check.

The final claim calls the shared engine admission hook, starts the logical read
lease once and advances the group's physical-send timestamp. Then the prepared
executor immediately hands the request to the transport API, without another
route/token/HOW await. “Physical send” means that final controllable transport
handoff, not remote receipt or TCP delivery. Native actor and JS synchronous
claim paths must share this contract. The host cannot independently renew a
scheduler lease, reserve a pacing slot or infer outcomes.

Custom/native/Workers transports must explicitly implement this preparation/
claim contract; missing support is a typed construction/protocol failure, never
a silent managed-request fallback. Keep the legacy bridge explicitly unsealed.
Bound preparation, readiness, permits, cancellation, disposal and lost callbacks
by the original deadline. Fence early/duplicate responses before auth/reducers;
retain late write ACK/identity reconciliation without reopening a sealed command
or releasing another attempt. Add delayed-permit, expired-route, early-response,
same-group concurrent-ready, lost-message and disposal race controls in addition
to the owner cases above. Each retry retains all prior per-leg release evidence.

## Shared command-outcome aggregation

| Engine concern | Contract | Reference migrations and regression proof |
| --- | --- | --- |
| Multi-write command outcome | One engine aggregator joins every leg's release and definitive result facts. `NotSent` is valid only when no write may have reached the venue; otherwise aggregate the sent writes, preserving accepted effects, uncertainty and partial identities. Never let first-error text choose the aggregate outcome. | Replace Binance `reached_venue`/NotSent-stripping finalizer and Bybit `refused`/`effective`/`unknown` aggregation with this shared core. Assert original-RED controls for signer-refused siblings followed by native refusal, mixed accepted/rejected/unknown legs, and callback-order permutations. Binance's existing reached-venue fix is already present; consolidate it rather than claiming a second bug. |

Venue codecs still establish definitive ACK/refusal facts. The engine alone joins
those facts and seals the command; a pre-send sibling failure cannot mask a sent
write. Keep representative error text separate from the aggregate outcome.

Required retry case: a first write attempt reaches the venue (including Bybit
native 10004/10002 followed by a staged retry), then the retry fails at the
signer/provider or host transport (`network lost; error sign`). The logical
write must be `Unknown`, never `NotSent`. Preserve prior-attempt release evidence
centrally across signing, retry staging and transport failures, including Hyro,
which has no native-versus-transport slot today. Replace the transitional venue
`Rest.resent` evidence with engine ownership. Add original-RED controls for both
Bybit and Hyro, provider refusal and transport failure, plus out-of-order/late
retry completion; neither exception text nor the newest attempt may erase history.


## Migration and implementation phases

1. Re-count current request/retry/error call sites and freeze the exact internal
   context/lifecycle types. Add pure state/eligibility/outcome tests first.
2. Wire one release/admission boundary plus typed failure/outcome sealing into
   `engine.rs`, existing signing dispatch and native/WASM host boundaries; preserve
   raw timeout, protocol-revision, cancellation and action-ordering regressions.
3. Migrate Binance HTTP reads/writes and its supported retry/refusal paths as the
   reference, and migrate both Binance and Bybit command aggregation through the
   shared core above. Existing unmigrated families must keep working during fan-out. The
   owner-approved transitional bridge is explicit, frozen and forbidden to new callers:
   no inferred method-based semantics, engine replay or false typed-outcome claim
   for opaque legacy work. CI carries an exact call-site allowlist that may only
   shrink, checked against the base revision; new sites or allowlist expansion
   fail. Fan-out follows the signer migration order to avoid repeated venue edits.
   **Hard master-merge gate:** the bridge and allowlist must be empty and deleted,
   as recorded in [the blockers checklist](RUST_MASTER_MERGE_BLOCKERS.md).
4. Integrate actual queued predecessors and retain #1723 admission semantics; independent architecture/correctness/adversarial reviews,
   focused engine/Binance/host/SDK checks, complete authenticated raw replay proof,
   CI timing/size gates, separate fresh guard and exact-head merge. Then fan out.

Regressions must prove: POST reads stay reads; unsent/hung signing consumes no
response lease; first release versus continuation does not renew; pause/revoke/
expiry before release emits no HTTP; timeout/late completion settles once; fresh
resends keep order identity and original deadline; unknown writes do not replay;
a later pre-send failure cannot erase an earlier write's uncertainty. Preserve all
incoming tests/assertions. No per-venue admission/retry/outcome algorithm copy,
second executor, remote-signing wire change or NX deployment claim.

Before wiring, define the freeze point for `request_timeout`; every child retry
and renewed lease retains the original logical upper bound. Fence streaming
callbacks before auth/financial reducers and bound retained leg evidence/tombstones.
Add signed-denial, duplicate-denial, parallel-leg and timeout/response race controls.

## Implementation checkpoints

The first implementation checkpoint adds pure lifecycle, release-projection and
command-evidence types plus the typed scheduler admission adapter. It changes no
production request path until the executor/reference migrations below are wired.
Independent phase review must cover stale preparation/signing tokens, scheduler
denial, in-flight lease renewal, partial-effect retry refusal and prior-attempt
uncertainty. Unique checked attempt identities come from the engine registry;
constructing two independent lifecycle objects is not an identity allocator.

The executor wiring uses one prepared resource in the existing host, not a new
sender. The engine emits an unsigned HTTP preparation description; asynchronous
route selection, proxy authorization and HOW connection/authentication complete
before the host reports readiness with the engine's preparation token. Signing
then uses the dispatch clock. The host binds the resulting signed packet to its
prepared resource and performs all fallible synchronous URL/header/body/request
validation before asking the engine for the final claim. The claim validates
context/generation, time/revision/freshness, fixed-group pacing and admission; a
successful claim is followed immediately by the bound transport API handoff.

A pacing wait retains no reservation: another ready attempt may claim first.
On wake, repeat every claim check; stale packets are discarded and the unsigned
recipe is prepared and signed again within the original deadline. Bind permits
to the preparation token and credential route. Host maps own prepared resources
and in-flight packets, engine maps own immutable context, recipe and evidence;
neither puts credentials into readiness/permit metadata. Lost resources and
callbacks are cleaned up on cancellation, timeout and disposal. Native actors
and JS/Workers use the same synchronous claim contract; no scheduling or route
authorization await is allowed between a successful claim and API handoff.
