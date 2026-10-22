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
