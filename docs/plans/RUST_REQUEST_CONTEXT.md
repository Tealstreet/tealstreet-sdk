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

Owner sequencing clarification (2026-10-22): #1741's merge proof uses the shared
engine release policy and a real native prepared-handoff probe with a parked
provider. The actual MEXC parked-provider 450 ms regression is mandatory in its
signer migration; Phemex's actual repair-deadline projection is likewise mandatory
in its companion migration. Both block cutover, rather than the shared core PR.
Core tests alone do not claim either production adapter has migrated.

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

Owner-confirmed projection: no released writes yields `NotSent`; all sent writes
definitively refused yields `Rejected`; known accepted plus definitively rejected
legs yields the existing partial contract (`outcome=None` with Partial IDs and
terminal rejected legs reported). `Unknown` requires an unresolved sent attempt
or the prior-send/failed-retry case below. A recovered retry's old refusal stays
in attempt history and must not be reported as a rejected logical leg. Test all
four projections independently of callback order.

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

The native scaffolding exposes `Transport::prepare_http` (typed unsupported by
default), `PreparedHttp::bind`, and `BoundHttp::ready/dispatch`. Both the legacy
and managed entry points use that executor. Binding preserves the signed packet,
constructs the final request/frame and performs no HTTP handoff; dispatch invokes
the transport API synchronously before spawning response work. The actor must
check readiness immediately before claim and retain the returned abort handle.
An unsigned preparation carries explicit public/primary/secondary route intent,
with no fabricated credential header or HTTP-method inference. This scaffolding
alone does not wire the engine's final claim or migrate any venue.

Send-group configuration defines a fixed gap floor. Individual releases can
extend the following gap (MEXC currently uses 450 ms or 1500 ms by job); the
next request cannot weaken the gap recorded by the preceding physical handoff.

Logical context issuance uses the engine's existing checked identity allocator,
separate from HTTP method or arrival order. A private, nonserialized issuer
identity rejects a handle from another engine even when both local counters
match. Context issuance validates policy before committing owner/leg metadata;
it queues no HTTP, timer, lease or pacing reservation. Command completion retires
its generation. Expired abandoned owners must release bounded registry capacity,
and explicit logical completion releases successful background/page-join contexts
without waiting for their original deadline. Do not retire a logical handle on
each physical page response. Physical-attempt registration still independently
checks native retry evidence; validating a retry context alone authorizes no send.

The next executor checkpoint must gate every response entry before auth and
streaming reducers. Reserve signing/action dependency sequence at creation so
reversed route readiness cannot reorder dependent HTTP/WS work; independent HTTP
releases need no new total order. Native actor final readiness check, engine
claim and prepared API handoff execute synchronously in that order. The registry
and prepared native transport checkpoints alone do not migrate production HTTP.

### Prepared public-read host checkpoint

The shared lifecycle now has an unsigned-public-read entry point, original-deadline
engine timer, native actor preparation/binding/final claim, and lossless WASM/JS
preparation tokens. Default fetch and Workers use the same prepared executor for
legacy and managed entry points. Unsupported custom transports fail explicitly;
managed calls never fall back to their legacy request method. Claim returns only
release/wait/denial status, without draining actions or invoking another provider
before the synchronous transport handoff.

Managed response admission rejects pre-release responses, premature end/chunk,
duplicate start and stale preparation generations. Retirement inside a streaming
row stops the remaining rows and cannot restore a removed continuation. Host
cancellation stops body iteration before another potentially hung read. Unready
prepared resources settle once, and post-release diagnostic failures cannot
relabel a sent request as a preparation failure or strand its response promise.

This is an implementation checkpoint, not production migration or merge proof.
The unsigned entry point rejects mutations and credential-bearing contexts until
signed dispatch, central command evidence and late-write reconciliation are wired.
The native delayed-route/fast-ACK probe verifies the release boundary's 450 ms gap;
the parked primitive-provider probe remains required. SDK proxy/HOW preparation,
signed HTTP, Binance/Bybit reference aggregation, the legacy ratchet and complete
final CI/replay proof remain open. Local compiled-WASM integration tests currently
find a stale single-family artifact (unsupported Binance/Phemex); native WASM-host
unit tests and JS host/Workers unit checks are separate evidence, not a rebuilt
combined-WASM claim.

### Signed-read wiring audit

Reserve the existing Dispatch sequence when the managed request is created, then
hold recipe preparation until host route readiness. Use the same SignStep and
provider/result validation as legacy dispatch. A completed signature produces a
bound host packet but retains only its unsigned recipe and freshness metadata in
the engine until final claim. Publish receipts only after final release admission.
Stale route/protocol/signature metadata cancels the exact old preparation/provider,
reserves a new signing attempt and prepares again under the unchanged raw deadline.
Retiring a logical owner aborts its route, pending signer and bound packet; late
provider success cannot escape through the legacy outbound branch.

The next phase enables signed reads only, keeping mutation gating until command
ledger/reconciliation wiring. Tests cover reversed route/provider readiness,
original deadline and protocol-revision replacement, duplicate/late completion,
receipt publication at claim, cancellation of a hung primitive provider, and the
native parked-provider/fast-ACK 450 ms final-handoff probe. Legacy WS signing and
its subscription sequencing stay on the same Dispatch; no second signing queue.

The signed-read checkpoint reserves the existing Dispatch sequence at creation,
prepares only after route readiness, retains an unsigned recipe through binding,
and publishes receipts at claim. Independent HTTP route/provider completion may
race, while the existing signed-WS/plain-subscription barrier stays ordered.
A read-only signingPending query prevents delayed host work from restarting a
retired provider. Cancellation aborts the exact primitive attempt and ignores
late success; native host shutdown also aborts parked providers.

Its native parked-primitive-provider probe records physical handoffs at 700 ms
and 1150 ms despite a fast ACK. A fault control discarding release-group pacing
fails that assertion (two sends before the second gap); restored code passes.
Additional controls cover calibration replacement/new provider IDs, original
raw-deadline precedence, signature receipts only at claim, mixed HTTP/WS order,
retirement, and invalid recipe versus capacity errors with continuations intact.
Independent checkpoint review is clean. Mutations, central command aggregation,
late-write reconciliation, SDK proxy/HOW preparation, venue references and the
legacy-call ratchet still remain before PR readiness.

### SDK route and HOW preparation phase

Extend the existing SDK HTTP wrapper and HOW adapter with prepare/bind/dispatch;
legacy requests call that same executor. Finish proxy authorization and tunnel
connection/authentication before readiness. Bind exact signed bytes only after
readiness, validate/serialize before claim, then invoke fetch or raw WS send
synchronously without an authorization/logging/route await. Preserve the original
HTTP timeout through every stage and streaming response; cancel only the owned
request/listener, never a peer's shared tunnel.

The SDK maps explicit public/primary/secondary intent to its configured route,
including POST reads, with no credential-header fabrication or method inference.
Add optional non-secret credential intent to provider route selection; legacy
callers retain their existing policy until migration. Missing custom prepare
support fails typed Unsupported. Capture tunnel connection/auth generation at
readiness, reject a changed generation before claim, and avoid immediate reprepare
loops for unusable transports. The direct runtime receives an explicitly resolved
route; it never independently substitutes primary routing for a secondary read.

Regressions cover parked authorization/no HTTP, cold HOW/no rpc.http before claim,
reconnect/auth changes, signed-byte preservation at bind, synchronous handoff,
original deadlines, cancellation/disposal while parked or reading, and concurrent
peer requests. Existing transport/Workers/legacy tests remain intact. Files are
SDK transport/tealstreet types and tests, existing HOW adapter/socket and docs;
no venue signing/remote wire or scheduler algorithm changes.


The SDK/HOW checkpoint finishes route authorization and tunnel readiness before
binding, chooses routing from explicit credential intent (including POST reads),
and invokes the existing raw fetch/WS executor synchronously at final dispatch.
Managed HOW preserves request/response string bytes regardless of legacy parser
selection; legacy Axios parsing/simulation and routing remain opaque. Custom
managed providers require valid preparation and bound-resource interfaces, with
no legacy-send fallback. Original deadlines, raw resource expiry, connection/auth
epochs, signed header conflicts, cancellation and peer isolation are checked
before release. Bound/prepared cleanup is idempotent even after response cleanup
or synchronous cancellation during bind. Already-expired readiness settles rather
than beginning signing or an immediate reprepare cycle.

Phase checks: 367 SDK transport controls, 54 HOW tests, 168 JS host tests and 357
engine tests, package type/lint checks and strict engine Clippy/bans. The review
found and corrected dropped expiry, signed site-header overwrite, managed body
parsing, malformed-resource legacy fallback, provider-route mutation and resource
cleanup gaps, each with focused regression coverage. This is checkpoint evidence;
command ledger/reconciliation, Binance/Bybit migrations, the legacy ratchet and
complete final integrated CI/replay proof remain open.


### Engine write evidence and sealing phase

Extend the existing ContextRegistry/CommandWrites, not a parallel command executor.
Each managed mutation belongs to a command owner/generation and a logical leg.
Register physical request IDs before queueing; a retry references the previous
physical attempt and requires both the original policy/deadline and an effects-free
native refusal already recorded in the journal. A route/signature reprepare keeps
one physical attempt because no transport release occurred. Capacity/invalid
recipe failures leave the continuation usable and cannot commit partial journal
or sequence registrations.

The shared final claim validates journal release eligibility before admission;
only successful claim records physical release. No fallible check follows the
admission/pacing commit. Original raw deadline and final-release API stay frozen.
Transport/sign/provider/timeouts produce failed-attempt evidence centrally. Venue
codecs report only Accepted/definitive Rejected/Failed facts with accepted IDs;
they do not join outcomes. Bind result reporting to a private full active attempt/context token
(issuer, owner generation, logical leg and physical request), so stale/foreign/sibling/retry callbacks cannot
supply another attempt's facts. Add one private current-delivery marker in Io,
scoped/restored around all venue delivery (including preparation/signing failure
and nested synthetic timeout events). A stale token cannot seal a reused numeric
command ID belonging to a newer generation. Explicit leg completion also retains
its prior-send journal.

Context-bearing Cx.done_context/fail_context seal managed command evidence once
through one private seal/retire/publish path. Numeric done/fail remain explicitly
unsealed legacy bridge calls and cannot bypass a live managed journal or delivery.
Managed venue migrations must use the Context-bearing finalizers. Their authority
validates issuer and live owner generation even after logical HTTP-handle removal;
a missing/retired owner is stale rather than legacy. Managed delivery must also
match the owner and current physical attempt. Logical command timers retain their
owner Context; physical timers retain an Attempt and use fail_http_attempt, so
an old same-generation physical timeout cannot settle its newer retry. No release ->NotSent; all sent
writes effects-free refused ->Rejected; known accepted/rejected mix ->None+Partial
IDs and terminal rejected-leg notices; unresolved sent attempt or prior release
then failed retry ->Unknown. A recovered retry does not report its old refusal.
Representative text follows logical registration order, not callback arrival.
A requested success with unresolved/failed mutation evidence must not become
CommandDone. Legacy commands without a managed journal retain existing behavior.
Keep active sent journals until sealing, even if another command is issued after
the old context deadline but before its timer is delivered; bounded capacity may
refuse new work rather than erase sent evidence.

When a command seals, stop unsent work immediately. Released outstanding writes
leave ordinary reducer/auth/command delivery and retain only bounded reconciliation
continuations and response metadata. Introduce a narrow Venue reconciliation hook
with Store/identity access only (no Cx, retry, command-done/error, auth or scheduler
publication). Retain at most the existing 256 managed slots, no new network waits
or extended transport deadline; a fixed 30s bounded reconciliation-state grace after
the original raw deadline handles callbacks already racing cancellation.
Evict expired tombstones lazily and fail closed at capacity. Managed mutation response headers/body and total retained metadata need explicit
bounds: existing Incoming.body has no body cap. Preserve required raw mutation
response bytes from the first chunk (including any portion already streamed),
or reject mutation streaming before release; never reconstruct lost rows from
splitter metadata. A closed size/capacity failure stays Unknown once released.
Collect late response data without streaming reducers; a late complete
response may reconcile identity/store once, never reopen a sealed command. Late
old ACKs cannot overwrite a newer order revision or clear newer order-lock
uncertainty; pass the original owner/attempt/release revision to the reconciliation
boundary and restrict it to identity/order changes compatible with that fence. Reads
and duplicates retain existing retirement behavior. Tests must pin timer-before-
response, response-before-timer, late body/auth silence, duplicate/reused command
IDs, old/new leg isolation, no capacity leak and no held outbound credential data.

First implement/prove this shared engine lifecycle with an adversarial reference
Probe, then enable production mutation entry only after reconciliation is wired.
Next migrate Binance reads/writes, removing reached_venue/effective finalizer
classification and using native fact reports; integrate actual Bybit signer
before replacing its refused/effective/unknown/resent aggregation. Preserve
command progress/Partial IDs and every incoming financial/clock/signing assertion.
Core RED controls cover callback-order permutations, mixed known effects, all
refusals, no release, unresolved writes, prior-send + provider/transport retry
failure, and exactly-once finalization. Focused engine/Binance/native/JS/SDK checks
and independent review precede the checkpoint commit. Full raw/timing proof stays
on final integrated CI; no legacy-provenance sealing claim.


Independent plan review: feasible, architecture No/shared concern, with explicit
requirements above for full-token scoped delivery, evidence retention beyond live
handle expiry, success sealing before teardown, atomic no-fail release commit,
real response-size limits and mutation-stream preservation, and late revision
fencing without extending transport activity. Do not enable mutations before
those controls pass.


### Reconciliation checkpoint implementation contract

The journal checkpoint is c27bf397be1 (386 engine tests, strict Clippy/fmt,
ban and staged secret checks; independent review CLEAN). Mutation queueing remains
closed. The next checkpoint introduces a bounded codec-only late-response record:
256 active captures and tombstones together; 256 KiB body, 16 KiB headers,
8 KiB opaque UTF-8 codec metadata per record; 4 MiB aggregate retained bytes,
including in-delivery reservations. These are integrity bounds, not new network
timeouts. Tombstones expire at original raw deadline + 30 s and never emit timers,
new requests or retry/auth/command/scheduler actions. Streaming mutations are
rejected before release; complete raw response bytes are retained from first chunk.

A venue supplies a bounded ReconciliationData codec payload from its pending
continuation before final admission; capture and capacity validation must precede
admission, leaving only infallible release metadata assignment afterward. Its
default is absent, which refuses managed
mutation release until the adapter declares a reconciliation path. This payload
must contain only native correlation facts, never outbound URLs, request bodies,
credentials or signing recipes. The engine retains no generic Pending, HttpRequest
or SignStep in tombstones. A late complete UTF-8 response consumes the tombstone
once and calls a separate codec hook with this data and a restricted reconciliation
view, not Cx. That view exposes read-only Store and revision-fenced order updates;
it cannot finish commands, clear order locks, authenticate, schedule or retry.
Any newer Store order mutation invalidates the old response's mutation fence;
the monotonic revision must saturate safely and saturation denies reconciliation.
Multiple patches in one callback use its initial fence, not a new check per row.
No late ACK clears order-lock uncertainty. Capacity/size/invalid UTF-8 failures
retire response state without changing a sealed Unknown outcome.

Implementation: pure bounded response/tombstone service and tests first; then
engine retirement/event wiring and restricted Store revision view; then adversarial
native lifecycle tests, independent review and only then mutation activation.


The closed-queue collector checkpoint additionally shares the ordinary active
response event scope/status counters/deferred-lock drain. Late codecs bypass all
of those paths. Engine-generated deferred finalization temporarily clears an
unrelated HTTP delivery token and restores it afterward. Preparation failures
use the same normal event bookkeeping. Independent review is CLEAN; behavioral
fault controls pin delivering-capacity retention, auth scope, deferred authority
and order/write revision fencing. No production mutation support is claimed yet.

Before activation, shared order-lock accounting must use the command owning the
released write (legacy note_write currently marks every active placement). Only
Unknown managed seals may preserve placement uncertainty; a fully known journal
seal resolves that command's sent marker before ordinary lock retirement. Legacy
lock behavior stays unchanged. Controls must cover two active commands with only
one physically sent, all-definitive refusals, known partial effects and unresolved
sent effects. This is shared engine lifecycle work, not venue policy.


Scoped lock checkpoint: Context issuance marks a managed command's lock scope;
legacy opaque sends and placement_resolved cannot provide or erase its evidence.
Actual managed release records only its own command. Central known journal seals
resolve that scope; Unknown keeps its uncertainty. Legacy locks retain existing
behavior. Independent review CLEAN / architecture No. Three behavioral faults
(legacy send, known seal, legacy resolver) fail exact assertions; restored full
engine/Binance suite: 737 tests, strict Clippy with/without order locks, fmt/bans.

### Activation and reference migration plan

Next remove only the closed mutation queue gate after the shared reconciliation
and scoped-lock controls above. Replace white-box mutation fixture staging with
the actual public request_managed/request_signed_managed lifecycle; test default
missing-capture refusal, real public queue rejection rollback, signed provider
failure before release and exactly-once completion through normal event delivery.
No legacy API is promoted or silently wrapped as managed work.

Binance reference: keep one Context authority per logical command and distinct
physical leg Contexts under its original bound. Trading jobs retain authority,
not aggregate reached_venue/effective/unknown flags; native codecs report typed
facts, preserving ACK IDs, native partials and command-error reporting. Background
reads use explicit read contexts. Shared history Completion needs an additive
Context-bearing authority path (owned/coordinated with cxxyqr), not a copied
Binance finalizer or numeric bypass. Preserve HTTP/WS signing bytes and the
existing independent test oracles. Update native test drivers to execute the same
prepare/bind/claim contract, retaining fail-closed unsupported cases. Then actual
Bybit uses the same journal instead of refused/effective/unknown/resent inference.
Legacy session-token work remains its explicitly tracked later WS-auth core;
the exact caller ratchet freezes every remaining opaque site and may only shrink.


Reference prerequisite in the same activation checkpoint: add
Cx.http_leg_context(&parent, Policy) -> Result<Context, Error>. Validate the live
issuer/generation-bearing parent first, keep its owner, and cap the child's exact
f64 raw deadline at min(parent.deadline, now + requested timeout). Do not round
remaining time upward into a new timeout. No new owner generation, permission or
timer is created on invalid/expired/foreign parent. References retain the original
command authority; HTTP children may shorten its bound, never extend it. Tests
cover fractional now/deadlines, sibling expiry, foreign/retired parent and command
ID reuse. This extends the engine issuer once, not per-venue deadline arithmetic.
