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


Parent-bound checkpoint validation: 62 focused Context/registry tests and strict
engine Clippy pass. Replacing the deadline clamp with a maximum fails the exact
fractional-deadline assertion; restoration passes. Independent API review CLEAN
(architecture No). Existing expiry probes now issue actual unrelated Read policy,
preserving all prior assertions under the explicit background-mutation rejection.


Managed history publication prerequisite: expose crate-only
Cx.validate_completion_context(&Context) -> Result<(), Error>, using the same
read-only issuer/owner/generation/current-delivery validation as central sealing.
Shared Completion validates before emitting history.incomplete, then calls
done_context synchronously; no numeric fallback, state mutation, deadline extension
or new send permission. Completion remains allowed after the raw deadline where
the existing logical completion contract allows it. Pin foreign, background,
retired/reused and old-attempt rejection plus valid post-deadline completion.


Activation checkpoint: mutation queueing now uses the public managed entry points;
all former white-box mutation fixtures use those same APIs. The closed-gate
control fails at the queue assertion before activation. Restored full engine
order-lock suite: 420 tests; strict Clippy/fmt/bans and independent review CLEAN
(architecture No). An unadapted Venue using the default capture hook cannot
release a mutation. Credential/recipe mismatch preserves continuation and emits
no resources. Parked engine signing refusal/deadline seals NotSent once, and late
success emits no Bind/receipt. An actual native-refusal response followed by
provider or host-preparation failure on the retry seals Unknown from central
prior-release evidence. These are engine Event/claim controls, not an external
asynchronous provider or venue replay proof. Reference migrations remain open.


### Legacy provenance ratchet implementation plan

Add scripts/check-request-context.py and focused Python controls, invoked by
the existing check-bans.sh. A Rust lexical scanner excludes comments/string
contents as syntax (retaining literals in call fingerprints), recognizes dotted
and qualified references to request/request_streamed/request_streamed_body,
sign_http, done and fail in venue src, and hashes normalized complete token
sequences per path/count into request-legacy-sites.json. Tests are included, as
with the signing ratchet. Method references without a call are frozen too; no
method, retry or outcome inference is attached to any legacy site. Comments or
line wrapping cannot bypass detection, and duplicated/changed/new sites fail.

The allowlist must match current sites exactly (removed sites are pruned). It
may only shrink versus every available committed parent/HEAD and landing-base
manifest; CI explicitly resolves its base ref and fails closed if that ref is
unavailable. The first reviewed bootstrap is permitted only when no trusted
base manifest exists. Negative tests cover multiline/comments/raw strings,
duplicated/qualified/reference calls, harmless literal examples, stale entries
and attempts to enlarge the manifest alongside new source. No auto-update mode
is wired into CI. The bridge plus manifest must be empty and deleted before
landing->master, as the existing blockers row requires.

The Rust bans job currently uses a shallow checkout; set fetch-depth: 0 for that
one job so committed parent and PR-base manifests are available. Resolve refs
separately from file presence: missing baseline files allow reviewed bootstrap,
but missing required refs fail closed. Receiver matching is conservative across
venue source, with no inferred semantics from a method spelling.


Reference harness prerequisite: add one test-support prepared-action executor in
engine/src/test_support.rs and use it from Binance/Bybit support drivers. It
retains fake route resources by exact Preparation token/id, runs prepare, bind
and synchronous final claim, returns HTTP only on Released, and handles matching
cancellation/reprepare. Immediate local providers reuse engine dispatch; external
signing, delayed route/pacing and unsupported paths fail closed in the existing
drivers rather than being silently treated as sent. Preserve action/receipt
order by processing actions generated by each handshake step before subsequent
sibling actions. A bounded work loop catches fixture recursion. Tests pin exact
primary/secondary bytes, no release on denied admission and progress before
physical HTTP. This is test-only host glue, not a second production dispatcher.


Ratchet checkpoint: initial inventory is 153 source files / 1,308 conservative
lexical sites (includes tests, field references and enclosing macros; this is not
a count of physical HTTP calls). Eight adversarial Python tests and full bans
pass; independent review CLEAN / architecture No after an exact macro repetition
bypass was found and closed. Because no current venue macro generates legacy
calls, such calls/member indirection inside macro definitions and Rust include!
are rejected outright. Trusted comparisons enumerate every HEAD parent and the
exact PR-event base commit, in addition to HEAD and landing. The fixture Git test
proves path resolution from sdk-rs and rejects manifest growth even when HEAD and
its parent already agree on the expanded inventory. No semantic provenance claim.


### Binance trading reference implementation plan

First migrate the existing sequential trading Job as one command authority,
leaving unrelated legacy branches explicitly inventoried until their own adapter
slice. Job keeps the issuer-owned Context and a stop/continue flag, not
reached_venue/effective or outcome classifications. Each planned physical write
derives a distinct child with original command bound; Pending::Trade retains
that exact child. Canonical bytes and signing recipe stay unchanged. Native
codecs report Accepted/Rejected/Failed facts on the delivered child; engine
preparation/transport failures supply their own evidence. Central done_context
converts an optimistic success with refused/unresolved evidence to the honest
outcome; fail_context joins every sent leg, never first-error classification.
Preserve IDs/progress, silent-native-code reporting policy and strict cancellation
proofs. Do not issue retries after unresolved native effects.

Use the existing 120s JS command lifecycle as the root default and the existing
20s transport operation default for each HTTP child, both captured as raw bounds;
no new timeout option. Children always cap at the root. Shared constants and
constructors belong in the engine once. Preparation/signing failure after prior
sent siblings must join centrally; skipped unsent future steps confer no fake
release. A no-I/O empty job completes without inventing a mutation attempt.

Reconciliation must retain bounded native correlation only, never full Pending,
URL/query/body/signing recipe. Extract cancellation ACK validation/patch facts
from existing native proof logic if needed so the late codec can use the limited
revision-fenced view without Cx or a second command finalizer. Late ACK cannot
reopen a command, authenticate, schedule, or resolve uncertainty. Keep all
financial fixes and native row/identity assertions. Original RED controls cover
signer-refused sibling + real rejection, accepted/rejected callback permutations,
known partial IDs, transport loss after release, and late cancellation revision
fencing. The common journal owns outcome and rejected-leg publication; codecs
supply native facts/reporting metadata, including per-item batch messages,
without duplicating aggregation or first-error logic.


Independent reference-plan audit resolutions: no-I/O jobs issue only a read
completion authority before any mutation declaration and complete through
done_context; that authority cannot later grant a mutation child. Nonempty jobs
declare mutation authority. Preflight all known planned attempt counts and native
reconciliation payload sizes before emitting their first packet; predictable
capacity refusal is NotSent, while dynamic later refusal joins existing effects.

Before adapters, extend the existing shared write journal with RejectionReports:
a validated collection of at most 256 messages, 8 KiB per message and 64 KiB
aggregate retained capacity. Default Rejected facts retain the current one-message
reporting; explicit native facts may supply bounded per-item messages or an empty
collection for silent codes. Failed native facts may supply reports, while engine
provider/transport failures keep their normal typed final result. One atomic
result_with_reports path validates provenance/state before recording facts plus
metadata. Central sealing publishes only terminal-attempt reports in logical
registration order; recovered retry reports disappear. No codec-side duplicate
command_error publication. Keep existing WriteResult variants/classification and
provide one Cx.http_write_result_with_reports entry delegating to that same journal.
Test bound failures leave evidence untouched, silence, native batch messages,
recovered retries, callback order and stale delivery. This same seam serves
Binance and Bybit, not a venue-specific outcome/reporting implementation.


Reporting audit refinements: the 64 KiB message budget applies across the entire
command journal, including historical attempts, with checked Vec/String capacity
accounting. Publish terminal reports by original logical-leg order, not the
physical position of a retried attempt (A, B, retry A must publish A then B).
Accepted facts cannot carry rejection reports. Explicit empty reports are not
the default one-message reporting. If bounded reporting is refused, the adapter
must retry the atomic native-fact write with empty reports, preserving the native
classification and all partial IDs; it must never drop the fact or panic because
an oversized diagnostic was supplied. Test this fallback separately from stale
provenance errors, which must never gain a second authority or numeric fallback.


Prepared fixture checkpoint: the shared test-only executor is adopted by both
Binance and actual Bybit drivers; their unsupported-action guards remain. The
initial three-crate suite passed 1,078 tests across 54 suites. Review then caught
a delayed-drain clock gap; its exact stamp control failed before correction and
passes after prepare_dispatch(now) precedes the initial drain. Relevant restored
checks include Binance security plus 15 Bybit dispatch-signing/protocol-clock
tests, shared signed/denied/order controls and strict three-crate Clippy. A compiled
queue-order fault fails the exact progress/physical-send ordering assertion;
restoration passes. Independent review CLEAN / architecture No. These fake routes
do not claim asynchronous transport/proxy execution or full venue replay proof.


Ordering clarification from existing compatibility control: engine-issued leg
IDs encode logical planning order, which can differ from physical registration.
Retain the existing representative-error selection by logical leg ID and its
Partial.total/failed_index assertions. Terminal reports use that same logical
order (including reversed physical registration and A/B/retry-A); do not change
it to first physical registration or callback arrival.


Shared native-report checkpoint: 428 full engine order-lock tests, strict
Clippy/fmt/bans and independent review CLEAN / architecture No. Explicit report
metadata has command-wide count/retained-capacity bounds including prior attempts;
capacity refusal is atomic and the tested empty-report fallback preserves native
facts and partial IDs. Wrong issuer/delivery and duplicate publication remain
refused. Compiled faults pin terminal logical-leg ordering and historical byte
accounting, and both restored controls pass. Existing logical error selection
and Partial.total/failed_index assertions are retained verbatim. This checkpoint
adds the shared seam; it does not claim the venue adapters or final raw proof.


Owner-approved native reconciliation boundary (2026-10-22): initial Binance and
Bybit late callbacks are read-only. Retire unresolved writes as Unknown and use
ordinary fenced reads to reconcile; keep ordinary ACK native known-filled,
terminal ownership and snapshot fences unchanged. Store-only revision checks do
not prove that an older in-flight native snapshot cannot overwrite a late patch.
Safe native late patches wait for the shared financial snapshot freshness token
owned by the row-admission lane; do not pull that prerequisite into this PR.

Initial inventory integration exception: actual #1748
(`265870747c0c646331b62bb00f0d1f00e042a67c`) landed before this core. The owner
authorized exactly its Paradex history bootstrap refresh, 19 to 14 conservative
lexical sites (eight removed, three changed). The three changed entries are the
existing matches! macro-reference form (two lookup checks and one public-trades
check), not new opaque request/completion calls. Pin the original and landed
path maps in enforcement; only an original-map baseline may use this transition.
All unaffected entries stay identical, and every landed/pruned baseline still
rejects subsequent regrowth. Test extra sites/counts, altered baselines, unrelated
paths and post-refresh regrowth. This grants no legacy execution guarantees.


Reference audit refinement: extend the existing engine command ingress duplicate
check to every live managed owner, before placement-ACK bookkeeping or venue
dispatch, including builds without optional order locks. Preserve the original
command/journal and emit the existing command.duplicate event; never seal it with
a numeric failure or maintain a venue-local duplicate table. Native response
codecs must not apply benign-code success exceptions on 408/5xx. Non-inverse
batch planning continues only after validated per-item native refusals; signer,
transport, gateway, malformed or top-level failures still stop subsequent lots.
Keep the old first-lot/second-lot proof and accepted IDs, while the central final
result honestly reports a known partial instead of optimistic batch success.


### Full retained-evidence budget (required before core readiness)

Review found that report bounds do not bound accepted ID vectors or full
VenueError fields retained across sequential callbacks and historical retries.
Fix this once in CommandWrites, before committing any result; preserve issuer,
attempt, release, outcome and Partial.total/failed_index facts. No venue budget
implementation or empty-report bypass may evade it.

Proposed generous command bounds: 4,096 exact retained IDs and 256 KiB for their
String bytes plus Vec element storage; an individual ID above 1 KiB cannot be
retained as an exact identifier. Never truncate an ID into a different order ID.
Keep the exact prefix that fits and mark the remainder as evidence overflow.
An overflowed sent attempt is Unknown and forbids automatic retry/success, even
when its retained prefix is empty or a sibling was definitely refused. Existing
accepted IDs remain available in the partial result; reconcile the unretained
remainder through reads. No-release commands still project NotSent.

Bound presentation strings (message, venue code and venue message) to 4 KiB each
and 64 KiB of retained content per command, including historical attempts. Append
an explicit [truncated] marker whenever content is shortened. Reserve a fixed
three-marker allowance per bounded attempt so an exhausted content budget can
still retain the marker. Reallocate into exact-sized owned buffers; input spare
capacity cannot bypass the budget. Retain kind, outcome, retry delay, builder
authorization and partial counts unchanged. Apply the same normalization to the
terminal fallback; do not claim a whole-engine heap ceiling from these bounds.

Validate state/provenance and report capacity before committing projected budget
counters, facts and reports atomically. A report-only capacity refusal leaves the
fact unmodified and supports the existing explicit empty-report fallback; it does
not disable evidence normalization. Guard retry, succeed, fail and terminal error
selection against the overflow flag. Test exact ID preservation, oversized IDs,
count/byte limits across attempts and retries, UTF-8 text boundaries, hostile
String/Vec capacities, exhausted-text markers, no-release/partial/Unknown joins,
retry refusal, and unchanged ordinary result/partial schema. Add compiled fault
controls for optimistic success and erased prior retained IDs. Native Binance
count-mismatched batches must reject row-to-caller attribution and reducer effects;
the later owner correction below retains valid native IDs as reconciliation evidence.


Evidence-budget review refinements: caps count retained entries, including
duplicates and historical attempts, not a separate unbounded unique-ID index.
Overflow is monotonic; an affected released leg cannot regain retry/success by
later callbacks. Include an explicit bounded ID-overflow diagnostic in the final
error even if the representative error came from another native refusal. Preserve
logical representative selection and ordinary under-budget outputs. At overload,
retention follows admission/arrival order, so do not promise callback-order-equal
truncated text or retained prefixes. Normalize genuinely new fallback partial IDs
and text against remaining budget; never charge already-retained facts a second
time when selecting/merging the final result. Reserve fixed marker overhead from
the configured attempt bound, including one terminal fallback.


Binance write checkpoint `654b4b58201`: root/child Contexts, same-bound managed
signing, native facts and terminal reports now use the engine journal. Remove
reached/effective and native first-error retention; retain only native stop control
and dynamic amendment planning errors. Shared ingress rejects duplicate live
managed command IDs before ACK/venue bookkeeping; completed IDs remain reusable.
Original-RED controls cover report selection/order, gateway benign-code bypass,
duplicate job replacement, overly broad batch continuation and count-mismatched
accepted IDs. The seven new adapter tests pass, including late Unknown/no Store
mutation and fresh ID reuse. Full Binance 272-test run passed before the last
count guard; all 73 affected library/account/ACK/new controls pass afterward.
Engine checks passed 435 tests with order locks and 418 without them; strict
three-crate Clippy/fmt, bans and independent checkpoint review CLEAN / No.

Declared caller/raw changes: an ACK at or after the immutable request deadline
is Unknown and reconciles through reads (the order may exist), where TS reported
success. Preserve the original two-poll PM fixture clocks and Store/native
assertions; only its exact-deadline success outcome changes. A known mixed native
batch continues later independent lots but completes with the existing partial
error/accepted-ID contract instead of optimistic whole-command success. Native
reports and representative error follow logical plan order, not callback arrival.
No final integrated CI/raw proof, read/Bybit migration or full evidence-memory
claim is made by this checkpoint.


Budget implementation layout: add private http_evidence.rs with a small copied
EvidenceBudget (retained entry count, ID storage bytes, text-content bytes),
project_result/project_error and bounded diagnostic helpers. CommandWrites
commits the projection atomically, retains per-attempt id_overflow, and caps its
configured attempt count at the existing engine maximum of 256. Keep public
WriteResult variants and the terminal schema unchanged. The final result merges
already-budgeted native facts; normalize a fallback only when it is selected.
Default result() reporting must not reject a money fact merely because its error
text/report quota is too large: bound its one-message presentation and omit only
that default report when the report-only budget is exhausted. Explicit
result_with_reports retains its atomic report-capacity refusal contract.

Evidence normalization checkpoint: terminal fallback indexes the bounded native
IDs by borrowed reference and copies each already-retained identity at most once,
without charging it again. New fallback entries consume the remaining budget;
an overflow preserves all prior exact native IDs and forces Unknown when sent.
The bounded text overhead is at most three markers for each of 256 attempts plus
one fallback, and one separate fixed ID-overflow diagnostic. The budget does not
claim to cap transient input allocation or every engine allocation. Tests cover
full native quota plus matching fallback IDs, repeated known IDs, one genuinely
new overflowing fallback ID, report-refusal atomicity, exhausted UTF-8 text, and
historical retry accounting. Independent evidence review: CLEAN; architecture
gate No (one engine evidence policy).

Provisional-CI integration follow-up: the native entropy probe still drains only
legacy Action::Http, so drive it through the existing prepared_actions fixture
to capture client IDs after the real final claim, preserving every entropy
assertion. Four actual-WASM fixture transports implement only request(); give
them one shared test-only immediate prepare/bind adapter which invokes their
existing request callback only at physical dispatch, respects cancellation and
single use, and snapshots the bound packet. Preserve all signing, accepted-ID,
cancel/stream race and worker/main forwarding assertions. Test the adapter's
no-send-before-release, override selection and once/cancellation properties;
production transports must still fail closed when prepare support is missing.
The provisional native/WASM failures are required fixes, not timing exceptions.

Actual #1756 landing integration `9995073dc39`: all 23 incoming file blobs match
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6` exactly. Owner-approved initial
history inventory refresh follows: Coinbase #1754 path 7 to 3 (five removals,
one same cx.fail re-expressed with typed metadata), WEEX #1751/#1752 path 17 to
10 (eight removals, one conservative macro-reference fingerprint changed), and
Lighter #1750 path 19 to 10 (nine removals only). Total 1,300 to 1,280; no new
legacy call kind. All other path maps stay identical. Pin exact old/new maps and
adversarial controls before updating the manifest; every newer/pruned baseline
continues to forbid regrowth. This implements the owner's standing pre-#1741
rule only through committed, exact maps; no automatic baseline expansion.

Canonical sorted-JSON SHA256 of the Coinbase old/new maps:
`7f923fa1fa91b9f81e47a240cadedad920dcad577279ab675d23b2f4a84ffb54` /
`d6fe784f25ab245b2860a9f6d46c3617cbc1c4545197e29a140dadeca7e91372`.
WEEX old/new:
`28fb5096b49ad5e340d77654b9b7b0bc4b5922b4dc58af5cff5fcfdff2161327` /
`477c91d6c2d026dea59a81d4dc539b56985dadf3606bdb7eae5b4ed505936163`.

Next reference slice: migrate Binance identity and candle commands first, without
touching shared history Completion ownership. Carry an issued root read Context
in their existing Pending variants; finish/error through done_context/fail_context.
Identity uses the existing explicit secondary-read-or-primary provider identity;
candles are explicitly public reads. Queue unsigned/signed managed requests and
attach the existing streaming collector after successful queueing. Both keep the
original 20-second raw deadline, never inferred from method or renewed on callback.
Pre-I/O validation failures retain their current numeric path only when no
managed owner was created. Once issued, every failure retains context authority;
queue rejection returns the continuation and finalizes exactly that owner.
Do not add a numeric fallback when managed completion fails. Test parked route
and signer, typed failure, streamed result, duplicate/late callback and same-ID
reuse, preserving provider slot/signature, query bytes and ordinary result schema.
Account/background helpers and history follow in separate slices; their current
legacy sites stay inventoried and receive no managed behavior claim yet.

Identity retains its legacy correlated diagnostic before the terminal failure.
Expose one additive command_error_context reporter using the existing synchronous
completion validator; stale/foreign/background authority emits nothing. This
lets native adapters preserve that ordering without an unchecked numeric report
or new validation policy. It grants no send permission or deadline extension.

Owner correction for count-mismatched native batch ACKs: retain valid native IDs
as evidence-only PartialIDs while sealing Unknown, stopping later lots and
applying nothing to Store/native ownership or caller-leg bindings. Keep the
existing Coin-M short-ACK ID assertion. Revise the new mismatch control to
Unknown plus exact native IDs; never invent row-to-caller attribution. Decode
the array shape/count first, then inspect mismatched rows independently for
valid positive native order IDs without a refusal code. Keep that evidence
separate from the normal accepted-ID/planning vector, and submit it only to the
shared command journal's bounded Failed partial record. Malformed/refused rows
grant no IDs or success; earlier/native sibling evidence remains intact.
Regressions cover short/extra rows, malformed/refused siblings, empty mismatch,
no Store or ownership change, no continuation, and preservation of the original
short-ACK cancellation/reconciliation identities. Document in the caller section.

Identity/candle checkpoint: the two preparation/deadline controls failed on the
old adapter, then all five new native read controls pass, including streamed
timeout, typed preparation failure and parked-provider refusal. Existing identity
HMAC/secondary routing, result JSON, Store and candle query checks remain intact.
434 engine and 15 Binance library tests, 32 focused native controls, strict
engine/Binance Clippy/fmt and independent review pass. The native private-read
refusal fixture now prepares its route before exercising the original signer
failure/timer assertions. Inventory shrinks by four more sites, 1,280 to 1,276.
This is not full Binance/whole-PR readiness: the separately approved ambiguous
short-batch ID correction, account/history reads and Bybit migration remain.

Fresh integrated CI at f7a0adc0e94 passed all 579 actual-WASM SDK tests, plus
11 server / 5 MCP / 2 CLI and 2 ancillary checks. The Binance gate then stopped
before replay because the raw-WASM entropy probe still inspected only immediate
http actions; no replay ledger was produced, so no replay parity is claimed.
Keep all deterministic/production/randomness/exception assertions and add a
bounded immediate raw-WASM fixture driver for Prepare -> Prepared -> Bind ->
Claim. Surface the final canonical packet only after successful engine claim;
reject delegated/parked/pacing paths that this fixture does not model. Reuse it
for the probe's startup and order actions without a production legacy fallback.
This validates entropy through the actual compiled engine boundary, not network
or proxy preparation; CI must rerun the probe before emitting replay artifacts.

Batch-count correction checkpoint: independent review CLEAN / architecture No;
the revised evidence matrix failed before the correction and now passes. All
278 Binance tests and strict engine/Binance Clippy pass; the original Coin-M
short-ACK ID assertion is unchanged. Caller contract: Unknown PartialIDs are
exact reconciliation/cancellation evidence only, not proof of accepted caller
legs or a safe retry. No total/failed-index attribution is invented.

Next read-reference phase: route explicit Binance public discovery/ticker/depth
and recurring account snapshot reads through the shared managed HTTP API. Native
adapters declare Background + Read, original 20-second raw bound, Never retry and
explicit primary/secondary credential refs; no method-based semantic inference.
Keep listenKey/keepalive session writes and command history on the inventoried
legacy path until their separately owned migrations. Do not relabel them reads.
Use one native Background(Context, continuation) wrapper solely to retain logical
completion authority; ordinary native parsers/retry/join/snapshot rails stay
unchanged. On response/failure retire that Context through finish_http_context,
and delegate streamed items to the existing collector. Context/queue refusal
uses the existing native failure callback, never silent legacy send fallback.
Public get helpers become managed-only; account call sites explicitly choose a
background signed read helper. Existing history signed helpers stay untouched,
so the history lane's approved mutable receivers are not needed in this phase.
Group query/credential/stream data in one read descriptor reused by command and
background adapters. Keep expiry, routing, signing and response suppression in
engine/host choke points. Tests must first fail for unprepared startup/account
reads and late catalogue/account results, then prove preparation failure unblocks
native retry/join state, existing endpoints/signature slots, streaming and no
registry-capacity leak across repeated completed background reads. Adapt only
canned raw-action fixtures to the existing final-claim driver, retaining every
original clock/result/Store assertion. Run Binance native tests and strict engine/
Binance checks, bans/shrink-only inventory, secret scan and independent review.
Architecture gate: No; these are semantic declarations/native callback adoption
of the already shared engine lifecycle, not a second venue-owned lifecycle.

Plan-audit correction: background timeout cleanup must allow an expired CURRENT
issuer-bound Context without granting send rights. Extract provenance/current-
handle validation from the existing timed send validator; finish_http_context
uses that cleanup-only check, while send/retry validation keeps its original
raw-deadline check. Retire background context before terminal native callbacks
can enqueue replacement work at registry capacity. Cleanup failure never suppresses
the already engine-admitted native terminal callback (a sibling may have pruned
an expired handle). Keep scoped auth/order-response evidence untouched; unwrap
before weight observation to avoid duplicate processing. Test expired/foreign/
reused cleanup and first-free-capacity replacement. No expiry extension or new
send permission. Independent plan audit: architecture No; these are shared
retirement semantics, with native reference adoption only.

Mandatory migration acceptance (not a #1741 core merge gate): WOO must declare
its existing 100-ms write / 200-ms PUT / 500-ms algo gaps through shared actual-
release pacing. A parked signer followed by fast ACKs must retain those physical
send gaps; queue-time next_write/next_algo updates cannot establish them. This
blocks WOO signer adoption/cutover, alongside MEXC's physical 450-ms acceptance
and Phemex's raw-deadline projection test. Do not add a venue signing callback,
new clock mirror or per-venue pacing executor.

Background-read checkpoint: all three new preparation/deadline controls failed
on the original adapter, then pass with the managed path. The fourth control
refuses actual prepared routes and verifies fresh native account/position/order
polling after their unchanged cadence. Shared expired cleanup/foreign/reused
handle and capacity tests pass. All 282 Binance tests and 453 engine tests with
order locks pass; strict engine/Binance Clippy, formatting and bans pass. Inventory
shrinks 1,276 -> 1,274 (only two public legacy queue calls removed); every other
map is unchanged. Independent implementation/fixture reviews CLEAN, architecture
No. Canned fixtures prepare/claim at original event times and answer otherwise
unrelated balance/ticker siblings during unchanged total waits; no original
assertion, native target row, endpoint or clock deadline is weakened. History,
session-token writes and Bybit remain separately tracked, so this checkpoint is
not whole-PR readiness or complete raw replay proof.

Owner scope correction (2026-10-22): land #1741 with shared core and the Binance
reference once proven; ordinary Bybit adoption moves to the immediate next PR,
after shared replacement-group/supersession and known-partial-effects facts are
engine-tested. A definitively refused batch replaced by individual requests
needs a recovered parent join; partial cancellations with effects but no IDs
need a typed fact that is neither Accepted, retry-safe Rejected nor invented
Unknown. Native gateway/malformed success cannot patch Store optimistically.
Hyro stays legacy/inventoried with no whole-Bybit claim; shared multi-owner
observer fanout and reconciliation-proven retry are first follow-ups and hard
cutover blockers. Preserve generation-bound preflight/delay roots and typed
native retry evidence in that plan. Binance history/session-token sites stay
explicitly legacy under the existing bridge-empty cutover gate; shared managed
Completion remains history-lane work. No additional reference fanout in #1741.

Reference-proof correction, owner-approved before implementation: the same
known-partial-effects gap exists in Binance DELETE batch ACKs. New native RED
control proves one ACK cancels order12 and sibling -2011 refuses13, but current
journal projects whole Rejected. Pull only WriteResult::KnownPartial(VenueError)
into #1741. It requires a released attempt, forbids effects-free retry/success,
and seals outcome=None unless another sent attempt/ID overflow is unresolved.
No IDs are invented to encode cancellation effects. Preserve native error/text/
report ordering and the existing single evidence budget, including exact-ID and
text bounds. Include it in representative-error and rejection-report selection.
Binance confirm_cancel supplies a callback-local native known-effects fact for
validated successful batch cancellations; gateway/malformed/uncertain failures
remain Unknown and existing Store/ownership/quantified-fill guards are unchanged.
Test no-ID partial, retry refusal, required release, unknown sibling, bounded
text/IDs and actual mixed cancel ACK with both Store assertions. A compiled
misclassification control must fail. Replacement groups/ordinary Bybit/Hyro
extensions stay out of this PR under the owner's narrowed scope.

KnownPartial implementation-audit correction before final verification: a batch
codec's first diagnostic must not erase a later unmatched/duplicate/invalid or
native-unknown row. Keep callback-local uncertainty independent of representative
text and known effects, then submit Failed/Unknown ahead of KnownPartial when
ANY sibling remains unresolved. Retain valid cancellation effects; do not undo
proven native ACKs or infer missing caller-leg success. Test both row-order
permutations of definitive refusal plus unknown/mismatched/duplicate siblings.

KnownPartial checkpoint: the original mixed native cancel regression failed with
whole Rejected after applying order12 cancellation; it now preserves both Store
assertions and seals known partial (outcome=None) without inventing IDs. Six
row-order/unknown/unmatched/duplicate controls exposed and now preserve every
independent uncertainty fact, sealing Unknown while retaining validated native
effects. Shared engine controls cover released authority, retry/success refusal,
unknown siblings and the existing text/exact-ID budgets. A compiled negative
control that normalizes KnownPartial to Rejected fails the retry assertion; the
exact original source was restored before the final gates. All 456 engine tests
with order locks and 284 Binance tests pass, alongside strict Clippy, formatting
and bans (11 inventory adversarial controls). Independent implementation review
CLEAN; architecture No: shared aggregation policy with a native codec fact, not
a venue-owned command finalizer. This is local reference verification, not final
CI, complete raw replay equivalence or whole-PR readiness.

Final canned-transport integration plan (before editing): provisional CI at
edd9376e0a4 runs 579 actual-WASM SDK tests and exposes nine failures in three
files after Binance background/identity reads become managed. The three signer
controls and shared realVenue canned transport implement request() only; use the
already-tested test-only prepareRecordedHttp hook in those two fixture builders.
Do not add production fallback, alter provider/signature/refusal expectations,
change event clocks, extend startup timeouts or weaken any Store/frame assertion.
The six real startup/frame failures consume that same fixture. Verify shared
helper unit tests, SDK typecheck/lint and fresh CI compiled-WASM tests; local
existing binaries are not current-core proof. Audit: no production path changes,
no new lifecycle; architecture No.

Timing tally update: provisional edd9376e0a4 / Rust39799401878 Bitget public steady
CPU is TS483.837ms / Rust437.171ms versus435.4533ms allowance (about0.39% over).
Private gates remain declared diagnostics. Bitget stays on its original speed
profile. Final integrated CI must remeasure; a repeat requires inspecting shared
public-path work before treating it as runner variation, not a profile/budget
change. This failed provisional run is not final replay proof.

Canned-transport checkpoint: exactly one import plus prepareRecordedHttp member
added to each fixture. All original provider/canonical-signature/refusal, startup,
frame, clock and timeout assertions remain. The helper's three tests, SDK
typecheck and explicit non-ignored fixture lint pass. Independent review CLEAN,
architecture No; fresh compiled-WASM CI remains required.

Final conformance integration plan (before editing, after actual dependency
integration): #1737's probe accepts the old HTTP/sign/WS wire only. Add the new
managedHttp Prepare/Bind/Cancel schema to that one existing checker and its unit
controls. Keep token counters as canonical decimal strings with closed p1/r1
arity, u32 request/u64 bounds and packet-request/permit-attempt correlation; do
not coerce them through Number. Validate closed phase fields, credential intent,
packet method/URL/headers/optional body/route/timeout. Prepare/Bind are ingress
request evidence, never physical-send proof; cancel-only stays deferred. Do not
prepare, sign, claim or send from the conformance probe. Malformed/unknown ops,
unsupported advertised handlers and mismatched completions must keep failing.
Add valid wide-token controls and malformed/mismatched/unknown-phase controls
that fail on the old checker; run existing conformance tests and bans. This is
required new-wire recognition in the existing harness choke point, not another
capability policy or a valid-state proof exemption. Architecture No.

Independent conformance plan review: preparation sequence zero is valid;
request/generation and permit sequence are nonzero. Compare preparation/permit
request and generation only: the permit's third field is a host resource id,
not the preparation sequence. Structurally allow a zero resource id and optional
u32 timeout zero, matching the native parsers/encoder. Use BigInt only after
canonical ASCII decimal validation; keep wire strings unchanged. Packet optional
validation must read the nested packet, not the outer action. Closed route enum
is primary/direct/publicCors, and intent is independent public/primary/secondary.
No requirement for a prior Prepare in this one-batch passive checker. Temporary
exact-incoming-checker controls reproduce assertion RED on valid Prepare/Bind
and Cancel (unknown managedHttp), before repository implementation. Review CLEAN,
architecture No; actual dependency integration still precedes source changes.

Bitget public-path source audit after repeated provisional timing misses:
independent review finds common-event custody pruning/managed admission checks
and delivery-scope save/restore, plus an empty managed-map lookup for legacy
stream chunks and a per-row optional managed guard. Public WS events take
non-HTTP branches with empty lifecycle/signing maps; no new per-frame allocation,
row scan, hash/signing work or managed timer was identified. Legacy HTTP bootstrap
and periodic replies do run the added constant checks. The replay uses
EngineHost+tape transport, not native reqwest/HOW, SDK proxy preparation or the
new default fetch transport. Bitget source/profile and JS WS/frame decoding are
unchanged. This is static scope evidence, not a CPU causality proof or a noise
disposition: constant checks/codegen layout may still affect a narrow margin.
No obviously unintended expensive work was found; remeasure final integrated CI
and escalate a repeat with measured evidence before proposing any optimization.
Architecture No; no Bitget-specific workaround or budget/profile change.

Actual #1757 e6382b5cfc038612a15b4e4eade8542a62f2e1e5 integrated conflict-free.
Independent preservation checked all5,217 unowned entries and all12 overlapping
files' two-sided patch lines; all source/assertions preserved. Integration review
found a real semantic composition gap: managed Binance reads/trades construct
sign::Http directly, bypassing the incoming legacy helper's trusted header policy.
Before final CI, route the three signed recipe sites through one native factory
that attaches construction-owned policy. Keep key-only recipes default-deny.
Adapt the incoming delegated-provider test driver through actual prepare/bind/
claim (preserving all header/feed assertions), add managed trade positives and
capture original assertion RED before the factory fix. Add empty headers only to
the core SignResult fixture literal, matching the new optional-empty contract.
No signing policy/capture/provider redesign; architecture No, shared policy
propagation at the recipe boundary. Incoming private-feed/hosted APIs stay intact.

Passive conformance checkpoint: valid managed Prepare/Bind/Cancel failed the
exact incoming checker first. Updated one existing checker; all25 new/existing
conformance and feature-control tests pass. Independent implementation review
CLEAN: lossless counters, distinct third counters, zero/optional representation,
closed nested schemas, passive ingress-only evidence and every prior unsupported/
terminal/correlation assertion retained. No executor calls or capability growth.

Post-#1759 integration: exact Store keep-both resolution authorized (broad
orders_revision/getter followed by selective epoch/methods); independent review
retained all5,216 unowned entries plus both parents' eight overlapping patches.
Ordinary managed auth observation now supplies the new read-only Store argument;
late reconciliation remains silent. Owner requires the late-reconciliation
follow-on to converge the broad revision with the snapshot epoch or document
why both are necessary, rather than retaining two unexplained mechanisms.

Header policy review also found the same preexisting bypass in legacy client-ID
lookup. Its single factory call is cleared by the history owner; completion and
paging remain legacy. Native managed read+write controls first fail at zero
released packets, then pass with policy attached; lookup independently reproduces
the same zero-packet failure before its one-call fix. Lookup remains unsupported
for COIN-M/Spot, with explicit unchanged native-refusal/no-request controls.

Owner authorized a one-time exact rehash of the two existing cx.sign_http calls
in Binance lib/history as recipe construction moves to the shared factory.
No new call kind or site: total inventory1274->1274, two path totals21->21,
all other maps byte-identical. Exact before/after maps JSON SHA256
5ef75f6182e96dda2de4ad3166a6d37fcbf3798cae540f8eb0e9a3e3946fe6ce.
Commit adversarial controls before implementing the exact map transition;
independent newer-pruned baselines must still block regrowth. These remaining
legacy signing sites still must reach zero and the bridge be deleted before
landing-to-master; no managed history or whole-Bybit closure claim.

Integrated local verification on actual #1757/#1759: 1,053 engine/Binance/
Bitunix unit and integration tests pass, plus3 real provider tests,43 native-host
tests (including parked provider+fast ACK450ms physical handoff),17 isolated
Binance WASM-host native unit tests,358 SDK transport/helper tests,34 HOW tests,
25 conformance/feature controls,25 JS primitive/managed-host tests and16 hosted
signing tests. Strict all-target engine/reference/registry Clippy uses the
Binance+Bybit feature set required by the existing construction fixture; source
formatting, SDK/HOW typecheck/lint,12 ratchet adversarial controls and bans pass.
The native parked Signer fixture adds only headers:vec![] for the new SignResult
shape; all original clocks, sends, gaps and assertions are unchanged. These are
local/debug checks, not rebuilt-WASM replay or timing/size evidence. Independent
integration/factory/ratchet reviews CLEAN, architecture No. Fresh CI, complete
authenticated raw/gate/package proof and separate guard remain required.

CI746817 integration follow-up plan: the actual combined-WASM SDK/server checks
pass, but MCP's recorded Binance public ticker test times out before its first
managed catalogue request. Its shared offline consumer fixture still implements
only request(), so managed startup correctly refuses that custom transport.
Add the existing test-only prepareRecordedHttp hook to this fixture factory,
keeping all MCP/CLI quote-change, socket, polling, clocks and timeout assertions
verbatim. This is the same canned-route compatibility change already applied to
SDK fixtures, with no production fallback or protocol change. Re-run MCP type/
lint/unit tests and the existing helper controls locally; fresh CI must prove the
actual recorded ticker test. The failed run has no Binance replay ledger/package
and cannot serve as final proof. No timing retry or assertion relaxation.

MCP fixture follow-up local verification: MCP35 tests and existing preparation
helper3 tests pass; MCP typecheck/lint pass. Independent review confirms only
the shared hook wiring changed and the real ticker test's assertions, timeout,
clock/socket/polling logic remain exact. Architecture gate No (reuse of the
shared test transport seam). Fresh rebuilt-WASM CI still owns recovery proof.

Next integrated CI diagnosis/plan (d4e): consumer WASM gates now all pass
(603 SDK, 11 server, 9 MCP, 2 CLI, 2 RN). The scripted lifecycle fails all ten
connect stages before replay because its fake transport has no prepareHttp.
Recorder/fence wrappers spread the underlying transport, so merely adding a
fake hook would also expose a real wrapper bug: an inherited native preparation
can bypass the wrapper's request validation/recording. Extend those existing
shared wrappers, never add a production request-only fallback. Fence validation
runs on the final signed packet at bind, before claim; preparation/cancel/ready
forward to the original resource. Recorder redacts/snapshots request data at
bind, forwards physical dispatch first, then captures the response through the
same existing redaction/streaming/failure-latch path as legacy request(). No
writer/profile/provider callback may run between claim and underlying dispatch.
Scripted fake preparation reuses existing prepareFetch with an explicit fake
fetch adapter; no network fallback or copied preparation state machine. Add
original-RED unit controls for final-packet fence enforcement and exact redacted
recording/single dispatch, retain all ten actual lifecycle assertions/clocks and
timeouts, run focused harness controls locally and fresh actual-WASM CI. Review
this shared seam before implementation. Actual Phemex1758 is integrated at953f;
its entire shared history/adapter delta must remain exact. Owner1764 merge hold
is respected; no landing merge while its proof window is active.

Wrapper plan audit refinement before source: explicitly shadow prepareHttp with
undefined when unsupported; preserve method receivers and prepared/bound cancel,
ready and expiry. Capture write command stage when preparation begins (before
parked route/sign work), redact final bytes at bind, retain read completion-stage
semantics. Cancel resources on bind failure. Managed main transport keeps the
same redirect:error and min(configured timeout, 20s) limit via prepareFetch.
Tests cover delayed binding, callback/redaction failures and underlying dispatch
first. Independent plan/preservation review CLEAN, architecture No; actual1758
all seven incoming blobs and all5,288 other paths remain exact after integration.

Wrapper implementation verification: the original four new controls failed by
assertion before the shared wrapper fix; the corrected full harness suite passes
1,145 tests, including final-packet fencing, original write stage across parked
preparation, immutable redacted capture, dispatch before recording and cleanup on
bind/redaction failure. Legacy request remains async. Independent implementation
review CLEAN, architecture No (shared prepared-transport wrapper integration).
All ten scripted lifecycle assertions, clocks and timeouts are unchanged; their
rebuilt-WASM recovery still requires fresh CI. Actual #1758 integration also
passes995 engine/Binance/Phemex native tests and scoped formatting.

Post-#1758 ratchet plan: prune only the three removed legacy completion calls
from venues/phemex/src/income.rs (9->6); global inventory1274->1271. The remaining
six fingerprints and every other path map are unchanged; no new fingerprints,
call kinds or bootstrap exception. Existing adversarial shrink/regrowth controls
must pass before updating the manifest, followed by the full ban gate. Record
exact map hashes in the PR body. The bridge still must reach zero before master.

Post-#1758 verification complete:995 native tests, strict all-target Clippy for
engine/Binance/Phemex, scoped formatting,1,145 harness tests and all ban gates
pass. The ratchet's12 adversarial controls passed before the manifest prune.
Phemex income map SHA2565a1db4f9bd6460c6d035bfe439e6777fc9f055609e4036f15061f5e6dba9ed07
becomes f13cd2730d7abc1d65c27847622a1ff49b7cd95c141112d5347b470e3b859b62;
manifest5461976913e8158ebe4fe8a9b038150bb8e8cb7738c3e7cafd13ca6761a5d535
becomes9136cef658d0891371b97dd6af83419e7715902e9f3712073330b0ef9747c3df.
The landed Phemex source is exact; every other map remains identical. No new
bootstrap authorization or exception was added. Final CI/proof still pending.

Fresh502caee diagnosis/plan: all1,145 harness controls and consumer startup pass;
nine actual-WASM lifecycle cases pass. The first completes its live capture and
TS replay, then Rust replay times out awaiting the private socket. The shared
Rust tape transport in replay.mjs still exposes request() only, so managed
startup refuses preparation. This is not the prior live transport failure.
Reuse the existing canned-response prepareRecordedHttp helper without a second
implementation: move its pure function to harness/recorded-http.mjs with the
same typed declaration, retain the SDK support path as a re-export, and add the
hook to the existing replay transport. Preserve its exact streamed request()
callback,16KiB chunks, tape-clock provider, signature validation and recorded
response selection. No production fallback, new executor, network fetch adapter,
relaxed clocks/timeouts or changed lifecycle/replay assertions. Existing helper
controls and harness tests verify the extraction; the unchanged actual-WASM
lifecycle assertion is the CI regression for wiring. Review this shared seam
before source, then fresh CI. Run502caee remains failed historical evidence, not
a complete Binance ledger or final replay proof. Respect the #1764 merge hold.

Recorded helper extraction verification before replay wiring: the original
function body/comments are mechanically identical after type erasure; SDK
support path is a typed re-export, runtime MJS has no imports/network/clock
patches. All1,146 harness tests and the three unchanged SDK helper tests pass;
SDK typecheck/lint pass. The new plain-Node control verifies original receiver,
exact final packet/header snapshot, deferred single dispatch, identical lazy
response stream and abort propagation. Independent review CLEAN, architecture
No. No replay recovery claim until the hook is wired and rebuilt-WASM CI passes.

Actual #1764 49470a1188b8e98dbdbbca573c67d25827921b19 real-integrated at471260:
all six incoming blobs equal landing and all5,292 other paths equal566e14.
The actual whole tree1f500afa47c791864eb3f3ed9db8dc6111e2f185 equals green
head6133fb814dc/source run39820395109, providing the next matching baseline.
Replay wiring is exactly the pure MJS import plus prepareHttp property on the
existing Rust transport. Post-integration1,146 harness tests and bans pass;
SDK helper3/typecheck/lint passed the extraction. Independent final source and
preservation review CLEAN, architecture No. All ten lifecycle assertions/clocks/
timeouts remain exact. Integrated engine/Binance/KuCoin native gates are running
alongside fresh draft CI; no merge readiness or replay recovery is claimed.

### Native recorded transport regression (2026-10-22, before correction)

Exact corea0c528 native CI39824926892/job119985877284 passed native44unit
and3entropy tests, then hung in tests/host.rs::the_native_loop_streams_startup_and_publishes_books
from17:46:49 until30-minute job cancellation. Its Tape only implements legacy
request, so managed Binance startup receives the intentional Unsupported default
and the old fixture polls for a WS forever. The exact compiled test also times
out after15 runtime seconds locally (compilation excluded); receipt
/tmp/codex-1741-native-host-runtime-red.json. No rerun or production fallback.

Plan before correction: add one test-only prepared-packet binder under native
tests/support, retaining unsigned request identity/method/route/origin metadata
and a one-use binding closure. It validates before handing the exact final packet
to an existing executor, never performs I/O at preparation/bind, and prints no
packet values. Tape explicitly opts into prepare_http, with Bound.dispatch calling
its unchanged three-chunk response emitter synchronously after final claim.
Bounded socket-start and host-join waits replace the unbounded fixture waits; all
existing traffic, sleeps, book/frame assertions remain unchanged. Assert managed
preparation/dispatch counters so a legacy bypass cannot pass.

The same native integration inventory finds NoticeNetwork in routing.rs wraps
Binance's legacy request only. Reuse that prepared-packet binder around the real
NetworkTransport.prepare_http and its actual BoundHttp, applying its existing
loopback URL/header mapping to BOTH unsigned preparation and final bind. Its
existing explicit account_route fixture selects Primary versus Public intent;
never infer intent from headers or modify production routing. Preserve every
connection/body/token/HOW redaction assertion and real network executor. Other
custom transports serve synthetic unmanaged venues; do not change them.

Add focused binder controls for no dispatch during prepare/bind, exact packet
bytes, one-use ownership and identity/method/origin refusal. Run the exact formerly
hanging integration test under a runtime-only timeout, all native integration
tests under a bounded outer process-group timeout, focused managedHTTP unit tests,
strict native Clippy/fmt and independent correctness/adversarial review before
any next push. Keep production Unsupported behavior and its existing negative
control. Architecture gate: No; this is shared test-transport contract adoption,
not a per-venue runtime workaround or a fallback to legacy dispatch.

Native fixture plan audit: validate the ORIGINAL id/method/route/URL boundary
before any fixture mapping; capture its deadline before async preparation and
never renew it at bind. NoticeNetwork must return the real network BoundHttp
directly, preserving ready/drop/dispatch/abort. Tape's canned bound checks that
original deadline and emits unchanged chunks synchronously only at dispatch.
Timeout failures must abort AND await the spawned host; timing out an owned
JoinHandle would detach it. Bound/cancel drop emits no response. Static errors
and custom Debug must not print packets. Independent plan review CLEAN,
architecture No; exact15s runtime timeout is reproduced, not a compilation delay.

Native fixture correction verification: the exact old compiled host test times
out after15 runtime seconds at a0; after explicit preparation adoption, the
rebuilt host binary's four tests (three shared binder controls plus original host
integration) pass in0.06s total. All six native binaries pass113tests under30s
process-group runtime limits each: entropy3, host4, routing37, native unit44,
server19, signing6. Original startup/book/frame/chunk/sleep and all routing
connection/body/token/HOW security assertions remain intact. Strict native
all-target Clippy and formatting pass. Independent final review CLEAN verified
all current fixture source hashes and result log hashes against
/tmp/codex-1741-native-final-suite-receipt.json (SHA256 c42209b2a3a3b3a24c6f1e50fabe928ad734ce1a70b070a7fc860d2972db4d62).
Architecture No; no production Unsupported fallback or executor change. This
resolves the demonstrated fixture hang; cancelled a0 CI/proof stays historical.
Fresh actual dependency integration and green full CI/raw/package evidence
remain required before merge.

### Exact landed GRVT/Crypto history inventory refresh (before integration)

Standing owner net-shrink authorization applies to actual1760
54cedc77ef6f89e2efd7e78938ebec6f1b84ac69 and actual1761
fe44969d033529c244fc4f33fce5395c0273df04. Only two legacy source maps
need re-expression: GRVT history13->11 (hashf97e2d042aad67c83cb7b67fd2e18fe2ea1911d39bee4928b4bfcd29a53a9173
->10b79560bfa80b935be2f78f7f6a3fb2e7b44436372c89a588a15d558c07fa3e),
Crypto.com history15->10 (hash8c08399ee9544d819511127584929ff3a4d532831d64ffc1886424efb4842b0c
->0716d0d13befbecad3402c21949f8d88fa30f02b9a761b1d3dbeae65570526f6).
Total1271->1264; GRVT lib19map remains byte-identical, as must every other map.
These are already-landed history calls re-expressed, no new site kind.

Commit exact old/landed map and adversarial acceptance controls FIRST, including
actual source hash binding, changed old bounds, forged paths/fingerprints/counts,
reverse transition and independently newer pruned constraints. Then extend the
existing exact history transition table with only those two immutable pairs; no
parser/ratchet relaxation. Preserve the two older exact history transitions and
all original bootstrap/signer-factory controls. Update manifest only to exact
actual landed maps after real integration; all unaffected maps must be identical.
Apply the same current-map exactness and independently shrinking base/parent
constraints afterwards. No guarantee/retry/outcome claim for legacy work.
1767exclusive landing window is respected; integrate its actual pin plus these
actual predecessors together before final checks/push/proof.

The exact transition test is committed first and is assertion-RED because the
two approved history-map pairs are absent from the checker. Original older
counts/hashes/source pins remain unchanged; the new actual1761 source and olda0
source are separately fingerprinted, and the new method kinds must be a subset
of the old ones. Existing adversarial tests iterate every history pair, testing
forged fingerprints/counts/paths/old maps and independently pruned newer bounds;
they therefore cover these additions without weakening any original assertion.
No checker, manifest or venue source change is included in this regression step.

Independent pinned-source audit CLEAN: incoming venue-source delta is exactly
GRVT history/lib plus Crypto history. All151 other maps remain exact. GRVT
re-expresses the same done/fail calls using history.command(); Crypto removes
six direct done/fail calls and adds one conservative scanner false positive
for a negated request.page_seen expression (the existing macro heuristic sees
the variable token request). That is not new physical HTTP/completion work;
freeze it exactly and leave the conservative parser unchanged. Net scanner
counts still shrink1271->1264. Explicit reverse-transition rejection was added
to every history-pair adversarial control before checker edits. Architecture No.

Exact transition table checkpoint: all12 inventory controls pass after adding
only the two pinned pairs. Independent final audit verifies every surrounding
source byte, parser/enforcement AST and older pair unchanged. Manifest stays
unchanged at1271 until actual source integration; no complete bans/ready claim.
Crypto's conservative match is not a new physical call/site kind. Source
preservation receipt: /tmp/codex-1741-history1760-61-checker-preservation.json.
After actual1767 merges, real-integrate the actual stack once, write only the
two exact landed manifest maps, prove total1264 and all151 other maps exact,
then rerun scoped checks/full CI and fresh post1762 ordered-gate proof.

Actual dependency integration checkpoint: merge9b2b6e310e3bc4757c3dec7b9ccc494993e463b1
genuinely integrates actual1767 bf362312d3edfb4dfa00b726c172f944053b9198
(and actual1760/1761) into the native-fixture correction. Conflict-free; all15
incoming paths and5300 combined tree entries are independently exact, with no
own/incoming overlap. Only the two approved manifest maps change1271->1264;
all151 other maps remain byte-identical, and integrated source matches inventory
exactly. Manifest SHA2569136cef658d0891371b97dd6af83419e7715902e9f3712073330b0ef9747c3df
->6641f9871098eed887286ec09f6dbf0e6d2f4af6a58edfef3f67013e19233352.
All12ratchet adversarial controls, full bans and scoped formatting pass; independent
final manifest/preservation review CLEAN, architecture No. Native fixture113
checks/strict verification remain recorded above; expanded integrated engine/
Binance/GRVT/Crypto/Orderly checks run before readiness. Start draft CI in parallel
with those additional checks; no proof/readiness/merge claim until every gate
passes on the same immutable source. Preservation and inventory receipts are
/tmp/codex-1741-post1767-{preservation,inventory-receipt}.json.


## Deterministic Workers deadline fixture (priority test-only follow-up)

Start from actual#1765 a3a89719d4079c97d56297bb4738fdc13c659ceb. The Workers
transport test expects exact20,000/5,000ms budgets but leaves Date.now live while
prepareFetch samples preparation and binding separately. A real1ms tick correctly
consumes budget, making the unclocked fixture flaky. Production timing is correct.

Plan before source: force one1ms Date.now tick inside the existing test to obtain
an assertion-RED [19,999,5,000] versus [20,000,5,000], then replace that temporary
clock with a test-context-scoped frozen Date.now. Preserve the exact timeout,
abort-through-body and routing-metadata assertions. No tolerance widening,
production clock injection/change, harness profile or semantic timeout change.
Run the filtered original-RED/GREEN and full Workers/related managed-host unit
checks, independently review the fixture and frozen assertions, then a small PR
with CI/background watch. Integrate actual#1775 before the separate final guard
and merge under owner queue. Architecture gate: No; this only fixes a test clock
for the existing shared managed transport boundary.

Fixture evidence: a temporary test-scoped clock returning1700000000000 for its
first read and1700000000001 thereafter reproduced the exact assertion failure:
expected[20000,5000], actual[19999,5000]. Replacing that temporary clock with one
scoped constant passes the unchanged test. The committed source change is only
that constant Date.now mock; TestContext restores it after the test. Independent
plan/correctness/adversarial review CLEAN, architecture No.

The package Workers unit script and managed-HTTP unit suite pass. An attempted
full Workers plus managed-HTTP run passed37/39; two earlier real-engine startup
tests timed out before the changed test. Keep that local failure visible; fresh
CI with its matching built WASM must verify the complete integration suite.
No production timing, timeout tolerance, route or response-body assertion changed.

## Bounded private-path profiling after #1741

Owner request (2026-10-23): after caller backfill #1777, profile the managed
Binance private request path before growing the follow-on. Use actual #1741's
retained CI Node WASM package (Rust run39844297596; verify ZIP/build/member
hashes) on current landing, which differs only by the caller docs. Run exactly
one sequential pass of the five existing Binance private gate cases: regular,
Spot, COIN-M, PM and scripted USD-M lifecycle. No local rebuild, public replay,
repeated benchmark, concurrency or live venue request. Bound each process to
90 seconds and the full pass to300 seconds.

Collect a sampling CPU profile plus synchronous engine-call timings/counters
through an out-of-tree diagnostic preload; never count awaited wall time as
CPU. Keep the replay source, clocks, options, tape bytes and output assertions
unchanged. Compare every nonmetric Rust result against its exact CI output.
Report startup/GC versus managed preparation/signing/claim/response work with
sampling/instrumentation limitations; this is diagnosis, not new gate evidence.
Only fix demonstrated clear waste that preserves all semantics, with focused
regressions and independent review. Otherwise report findings without a source
change. The shared response-proof/ordinary-Bybit work stays local and separate.

Profiling result (one pass, 4.078 seconds total): all five entire nonmetric Rust
outputs match the exact CI raw results. Aggregate managed prepare/bind/cancel
counts were17/9/12/14/20 and matched claim/drain counts; no aggregate attempt
inflation was demonstrated. Synchronous elapsed sums for the four lifecycle
invoke wrappers (httpPreparationPending/httpPrepared/httpClaim/prepareDispatch)
were1.627–2.346 ms per case; the three response wrappers
(httpStart/httpChunk/httpEnd) were9.555–12.361 ms, with httpEnd largest.
Inclusive WASM-ancestor response/control samples totaled355/54. These wrapper
sums exclude host JS outside invoke and are not CPU totals or new gate evidence.

No demonstrated removable waste justified a production change. The single-run
sampling, shifted import accounting from diagnostic preload, local machine and
stripped WASM symbols do not establish a cause for the CI ratio change. GC/idle
and clock-boundary uncertainty remain separate. An ignored local regular tape
failed preflight before any replay; the unchanged generator recovered exact CI
bytes into a temporary input, leaving the working tape untouched. No measured
case was rerun. Independent DATA/report review recomputed all five outputs,
provenance, sums, counts and sampling tables CLEAN; architecture No.
