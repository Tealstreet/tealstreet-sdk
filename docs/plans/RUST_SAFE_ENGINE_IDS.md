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
