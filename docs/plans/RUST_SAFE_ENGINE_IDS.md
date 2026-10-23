# Host-safe engine resource identities

Plan, 2026-10-23. Fresh branch from actual RequestContext #1741
`b2fe97324763e1e2702c69ac5b839da37102928b`. Separate from socket #1771,
selected signer mode #1776, and all family migrations. No allocator source edits
precede the concrete API/ownership review with the RequestContext owner.

The shared `Io.next_id`/poll helper currently wraps u32 back to one. Retiring a
socket cannot protect an old ingress event if a physical ID is reused. At high
server request rates u32 exhaustion is operationally plausible. Widen physical
SocketId, RequestId and TimerId storage to u64 while restricting every host-visible
numeric identity to `2^53 - 1`. Command IDs, duration fields, protocol clocks and
managed preparation/permit string tokens retain their separate contracts.
Signing dispatch also issues RequestId and has its own checked u32 counter; it
must widen consistently rather than leave a second resource ceiling.

Implement one checked shared allocation/range policy. Multi-ID managed HTTP
request/generation/timer admission reserves its complete range atomically and
returns typed Capacity before actions or child admission when unavailable. No
wrapping, dummy IDs, saturating IDs, aliasing or partial range reservation.
Legacy infallible APIs need an explicit checked fail-closed backstop; settle that
exact API before implementation, with original deadline and prior-send outcome
semantics preserved. No venue-local counters or fallback resource identities.

Audit every engine/poll/HTTP/signing allocation, registry trait/event forwarding,
native host maps and WASM ActionOut/HTTP packet/ingress exports. Numeric JS ingress
must be finite, nonnegative integral and within the safe bound before conversion;
never rely on wasm-bindgen u32 truncation or BigInt. Preserve lossless string
preparation/permit tokens and scalar managed final claims. Emit JSON numbers only
inside the exact safe-integer domain. Tests must cross the old u32 boundary and
reject NaN, fractions, negatives, unsafe integers, forged overflow and stale IDs.

Plan phases: (1) audit and concrete API review; (2) original-RED allocation,
retired-socket ingress and host round-trip controls; (3) allocator and all core /
host consumers; (4) independent correctness/adversarial preservation reviews,
focused native/SDK/HOW checks, exact ratchet map review, fresh CI and full replay.
Socket adoption integrates the actual allocator PR before its own final proof.

Required controls: retained retired socket ingress rejected after many later
allocations and across u32::MAX, request/timer uniqueness across that boundary,
last safe ID and exhausted-range atomicity, no pre-release child send/NotSent and
prior-send Unknown classification, old managed string token/claim behavior,
actual WASM+SDK event round trips with IDs above u32, invalid numeric ingress.
Caller section in the same PR describes wider safe JSON IDs and strict malformed
ID rejection; it does not claim a source or runtime change until implemented.

Architecture gate: engine resource allocation and host serialization are one
shared choke point; this is not a per-venue lifecycle fix. No socket policy,
command outcome aggregation, clock calibration or provider custody changes.

## Concrete allocation seam for review

`pub const MAX_RESOURCE_ID: u64 = (1_u64 << 53) - 1` is the single numeric
wire bound. `reserve_engine_ids(next: &mut u64, count: u64) -> Option<Range<u64>>`
checks a nonzero count and checked exclusive end at most MAX_RESOURCE_ID + 1,
advancing next only on success. Exhausted next is an internal sentinel only; it
is never emitted. Io exposes `reserve_ids(count)` over that helper. Managed HTTP
reserves three IDs before constructing/staging any child and maps None to its
existing `(pending, Error::Capacity)` return. Callers consume request, generation,
and timer from that exact reserved range. The separate signing-attempt counter
uses the same checked bound and never wraps.

Existing infallible legacy Io/poll helpers keep their call contracts, calling
checked reservation and explicitly failing closed at impossible exhaustion
instead of returning an alias or dummy. This backstop needs review separately
from the typed managed admission path; no capacity error may become an ordinary
transport retry or fabricated unsent result after a previous release.

WASM numeric ingress takes f64, validates finite/integral/in-range before u64
conversion and reports a typed JS error on invalid data. Zero remains available
for the existing shared-public socket identity; allocated IDs start at one.
Command IDs and milliseconds remain u32. All outbound resource fields become
u64 bounded numeric JSON, with no wasm-bindgen u64/BigInt export. Preparation
and final-claim token strings retain their unchanged u64 decimal codec.

Independent audit found two required refinements before source implementation:
signed managed admission must preflight both the three Io IDs and its Dispatch
attempt before committing either counter; reprepare must preflight before taking
the recipe or advancing lifecycle generation. Poll registration similarly
reserves logical and physical IDs together when it arms a finite cadence.
Monotonic gaps after later independent refusals remain intentional, never reused.

The legacy fatal backstop requires explicit fail-stop custody: latch exhaustion
before trapping, reject subsequent handle/drain work, and have hosts dispose the
failed engine so catching a trap cannot flush a partially executed callback.
This proposal is sent to the overseer before source edits. Tests cover both
managed counters exhausting independently, no partial pending/context/actions,
reprepare recipe retention, last safe ID exactly once, and no usable I/O after
fatal legacy exhaustion. Actual host ingress/serialization must reject invalid
IDs before touching clocks, health or draining actions; tuple constructors do
not exempt forged outbound IDs from the safe-number boundary.

## Engine checkpoint on actual history prerequisites

Actual #1768 `848924471635eee7a0c61c64407a861c3aea37f3` is integrated at
`e19020ee3e4`. The only conflict was independently inserted README rows, resolved
with the incoming row followed by the unchanged resource-ID row once, retaining
the complete incoming file when the latter is removed. The hash receipt is kept
for a comment on the eventual PR.

The concrete implementation retains checked state in `Io.next_id: ResourceIds`
instead of a mutable scalar. `Io.reserve_ids(count)` returns the reviewed
`Option<Range<u64>>`; consumers use that seam rather than direct arithmetic.
Dispatch owns a separate instance of the same allocator policy. Physical ID
wrappers, dispatch ordering and registry issuer inputs use u64; command IDs and
timeouts stay u32. Managed HTTP preflights all three Io IDs plus the signing ID
before committing either namespace. Context issuance returns typed Capacity;
reprepare preflights before advancing lifecycle or taking its retained recipe.
Finite poll registration atomically reserves logical and physical IDs; paused
polls need only their logical ID. Later independent admission refusal leaves
intentional monotonic gaps.

Legacy exhaustion latches before trapping. Engine event/dispatch/drain/frame and
managed-release ingress reject further work, and preparation/signing queries
become inert. Tests retain a real bound permit before fatal exhaustion and prove
it cannot invoke release afterward. Separate controls start with genuinely live
route/provider work before checking retirement. The existing decimal managed
token codec keeps full-u64 generations, but physical request IDs now admit exactly
the safe-integer bound.

This is an engine checkpoint only. WASM numeric ingress/outbound validation,
native and WASM typed terminal disposal, all remaining host/type adapters,
actual-WASM round trips, final caller contract verification, ratchet checks and
full CI/replay proof remain required before opening a ready PR or merging it.

## Host phase: numeric codecs and terminal custody

Implement the next phase as thin adapters over the engine checkpoint, without
changing command IDs, durations, hosted capture IDs or managed string tokens:

- WASM's fifteen physical-resource ingress exports plus signingPending accept
  f64, call one shared finite/integral/nonnegative/MAX-safe decoder before any
  clock/event/drain mutation, then construct the u64 wrappers. Return typed
  invalid-input errors on malformed numbers. Keep the JS ABI numeric; do not
  expose wasm-bindgen u64/BigInt parameters. Optional text/bytes APIs retain their
  empty/invalid-UTF8 behavior after resource validation.
- Widen ActionOut resource fields, managed HttpPacket.request and Cancel.signing
  to u64 serialization. Audit nested SignedOutbound and managed packet/permit
  IDs before any host receives an action, rejecting forged unsafe tuple values
  rather than trusting Rust tuple construction as a wire-boundary exception.
- Expose the engine's sticky failed state through AnyEngine/WasmEngine. The JS
  host's existing EngineDeadError path owns disposal, task/route/socket/timer
  cancellation and conservative pending-write settlement. Check sticky state
  around host invocation and final claims, so catching an earlier trap cannot
  permit later output. Workers must use the same terminal notification and stop
  their flush timers instead of retaining a disposed host.
- Native host callbacks use one common guarded turn. Only a panic accompanied by
  the engine's sticky fatal allocator state becomes the typed terminal failure;
  unrelated panics retain their existing behavior. Stop/cancel all host resources
  before returning or reporting terminal failure, and never drain partial
  callback actions or frames. FrameServer maps the same terminal failure through
  its existing bounded startup-failed channel and settles pending commands
  conservatively, without restarting the failed engine or reusing identities.
- Update only mechanical downstream resource-helper types. Keep registry duration
  casts, command IDs and independent hosted capture token bounds unchanged.

Verification includes native serialization above u32/MAX-safe and managed string
round trips; actual-WASM wide/invalid ingress must not alias a low pending request
or drain work; JS host wide Map keys and sticky-fatal same-turn no-I/O controls;
Workers/native terminal once-only cleanup and late-callback suppression. Forced
low-ceiling engine tests supply reachable exhaustion without load or timing
experiments. No local WASM release build is needed: actual-WASM acceptance uses
CI-built matching artifacts, retaining artifact provenance and explicit limits.

Host plan review refinements: inspect sticky state only before invocation and
after a normal return, never through a new Rust getter after a WASM trap. The
existing terminal cleanup remains the owner of resource disposal. A detached JS
action batch must stop when the host dies; managed claims still hand off
synchronously. Outbound validation checks the entire batch, including nested and
managed IDs, before any action is exposed, and latches the same fatal engine state
on unsafe output so the common native guard recognizes it. Native guards cover
initial startup and final frame generation too; ordinary venue StartupFailed is
not a fatal allocator event. Worker death during synchronous start clears its
matching host before interval installation, and late death cannot retire a newer
host or suppress already-owned pending command failures.

The actual-WASM original regression uses the existing CI Binance module from
run `39869163120`, artifact `12465757900`, without a local WASM build. ZIP SHA256
is `d280a6c4b48e67d7f2d0380bab7c56c249836505134d97d4f0761dc9e5793bfa`;
its build manifest records synthetic checkout `90a3d46dc42795825581cdc4bcd36895e7793ef7`
and the retained #1780 input fingerprint. This is historical original-RED
evidence only, not a current-head package or merge-proof claim. The native managed
prepare/bind/claim path releases HTTP request 2. Supplying 4294967298 to
httpFailed aliases it, increases httpFailures from zero to one and cancels its
managed resource. The durable actual-WASM fixture reproduces 97 assertion failures
and one compatibility pass across 98 controls: sixteen ingress exports fail to
reject six malformed-number classes, plus the above-u32 alias control. The
MAX-safe unrelated-ID compatibility control passes. Temporary ignored package
copies are removed afterward; the fixture requires a current CI module for its
eventual GREEN result.

Two JS-host original regressions separately exercise an already-sticky failed
engine before invocation and failure latched during a normal native return. They
pin zero/one native entries respectively and no transient connect or timer arm;
the existing 189 host controls pass while these two new assertions fail. Neither
case substitutes a thrown trap for the sticky-state contract.

Three additional engine assertion-RED controls establish outbound custody:
a valid HTTP packet followed by an unsafe timer must expose neither; unsafe
nested SignedOutbound socket IDs must be checked before dispatch can silently
discard their wrapper; managed Cancel.signing IDs obey the same bound. All 525
existing all-feature engine tests pass while these three new assertions fail.


## Host numeric and outbound checkpoint

The shared engine validates the complete outbound batch before yielding any
item, including nested signed actions and managed preparation/cancel identities.
Validation is iterative and checks both sticky allocators after callbacks. A
callback that catches exhaustion cannot resume dispatch or expose its earlier
queued HTTP action. All four outbound custody regressions pass with zero exposed
actions; all 529 all-feature engine tests and strict Clippy pass.

All sixteen WASM physical-ID ingress methods now validate through the shared
numeric decoder before parsing payloads or changing clocks/state. Resource IDs
serialize as safe numeric u64 values while command/capture IDs, durations and
opaque decimal managed tokens keep their existing contracts. Nineteen native
WASM unit controls, strict Clippy, SDK typecheck and 191 JS host tests pass. JS
checks sticky state before invocation and after a normal return, never querying
Rust after a trap, and stops detached batches after disposal. Two independent
correctness/adversarial reviews found no blocker in this bounded implementation.
The historical actual-WASM 98-control original-RED fixture still awaits a matching
CI-built module for GREEN; no local release build or old-package relabel occurred.

Native terminal handling is a separate next checkpoint: two compiled original
regressions fail because startup and drain traps currently escape the host task
instead of closing resources and emitting one typed terminal StartupFailed.
Native/Workers lifecycle completion, all-host checks and current-input CI proof
remain required before a ready PR.


Native original-RED checkpoint: two real host startup/drain controls induce the
same sticky resource-fatal state through unsafe IDs; they fail only the assertion
that the task returns via a typed terminal channel. Four compatibility controls
pass, including unrelated callback-panic propagation and ordinary venue
StartupFailed remaining nonterminal. Tests require no frame or HTTP escape and
closed transport ingress before exactly one Internal/Unknown terminal notice.
Two independent reviews confirmed the RED paths; active-resource and FrameServer
pending-write disposal controls are required alongside the implementation.


Workers original-RED checkpoint: the existing 26 Workers unit controls pass,
while five new assertions reproduce sticky death during start/flush, failure of
already-owned commands across replacement, old queued ticks touching a new host,
and join-time snapshot death. The fixtures use the real EngineHost with a native
sticky-state double and deterministic interval capture; no WASM build or load
loop is involved. The terminal path uses the existing startupFailed envelope and
EngineDeadError; success/diagnostic delivery remains tied to the current host,
while genuine captured-host terminal rejections retain their original owner.


## Native and Workers terminal checkpoint

One shared native guarded-turn adapter recognizes only the engine's sticky
resource-fatal state; unrelated panics propagate with their original payload.
Startup, event dispatch, managed handoff, action collection and frame production
use it. It closes ingress and cancels sockets/timers/managed routes/providers
before one typed terminal StartupFailed. FrameServer settles original pending
command owners Internal/Unknown exactly once, including a recorded CancelSymbol
POST that physically preceded failure. Late ingress cannot send; dead joins
receive only the cached failure and no snapshot. A separate queued-unsafe join
regression prevents a saved pre-failure snapshot escaping after drain fails.
Native standalone frame delivery owns a copy before invoking user callbacks so
the post-return sticky-state check happens before any consumer sees the frame;
this deliberately adds one copy per emitted standalone native frame.

Workers use EngineHost's existing EngineDeadError and a private captured-lifetime
error record. Only the matching host death clears current intervals/host/cache;
queued old ticks cannot flush a replacement. Genuine original pending terminal
errors keep their owner after replacement, retaining write uncertainty; old
successes/events and departed clients stay suppressed. Snapshot death during a
join returns one terminal notice without leaking membership or replaying it twice.

Both independent native correctness/adversarial reviews and the independent
Workers ownership review are CLEAR. All 50 native unit controls, strict native
all-target Clippy, 32 Workers controls, 191 host controls and native workspace
all-target compilation pass. The bans check currently reports only newer landed
Backpack/Bitrue/KuCoin history inventory removals absent from this branch; queued
actual-history integration is required before final bans/CI proof. No ratchet
exception or inventory edits were added for this checkpoint. Actual matching-WASM
98-control GREEN, final integration/CI/package proof and merge remain outstanding.

## Replacement-context integration after actual #1783

Owner-approved adaptation belongs in this allocator PR, not socket1771. Actual
response-proof core9a48b645d49 adds a bounded replacement child allocator using
the prior scalar u32 next_id; this must consume Io.reserve_ids instead. Preserve
empty/oversized request checks and exact current Delivery precheck before reserve;
reserve the entire child range atomically, then retain monotonic burned gaps if
policy, ownership or journal admission later refuses. ContextRegistry replacement
first/child identities use u64 and checked safe-end validation. No new timer,
send authority, journal ledger or allocator is added. Existing replacement
financial/ownership/negative controls remain, with fixtures using ResourceIds
states and the actual host-safe maximum. Add success crossing2^32 and atomic
last-safe whole-child-range refusal/one-child success controls. Both independent
reviews and focused engine/native/reference gates must pass before refreezing.
The former076 head/evidence remain historical; only socket1771 is authorized to
consume the new reviewed freeze, and actual1784 is still ahead in merge order.

The actual1783 adaptation passes561 all-feature engine controls and1,040 focused
engine/native/Binance all-target tests. Strict touched-crate all-target Clippy,
fmt, diff and full bans pass. Both independent final source/adversarial and
complete-tree preservation reviews are CLEAR. The only incoming doc conflict
retains the complete incoming CALLERS first and prior own section once; new
replacement behavior is described solely in the own SafeIDs section. Two
redundant test-fixture u64 conversions were removed without changing assertions.
No journal, body-proof, release, timer or venue signing policy changed.
