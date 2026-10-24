# Dispatch nonce reservation and release ordering

## Scope and pins

Owner: codex-uufub4. Consumer: codex-9czdjh, Kraken combined signer
`RUST_KRAKEN_DISPATCH_SIGNER.md` at `317e3495c8c`. Subject/alias review:
claude-m26ps4. HMAC/receipt review: claude-4yjlw0.

This checkpoint adds a plan and compiling assertions only. It starts from actual
`a392b114d80` and incorporates actual COMBO-5b `219d4bc6725` before edits. No
runtime, provider, host, venue, Cargo or inventory change is part of this phase.
Runtime implementation follows reviewed source hunks and this RED checkpoint;
5b's former source hold is now satisfied. #2024 StampFloor is a composition
prerequisite to test, not an assumed actual dependency.

## Current gap and actual lifecycle

`Cx::reserve_nonce` uses one process-global engine store, but only at venue
callback/queue time. `DispatchContext` exposes calibrated protocol time and
injected entropy; it cannot reserve a nonce. Managed requests deliberately allow
routes and providers to complete in different orders. Allocating A then B while
queueing can therefore send B's larger nonce before A's smaller nonce.

The actual order is route Prepare, SignStep::prepare/provider, Bind, final Claim,
then the host's immediate synchronous transport handoff. The nonce must exist
in the preimage at SignStep::prepare; it cannot first be allocated after Claim.
Reservation at prepare alone is insufficient: A can park in its provider while
B signs and claims. A's late provider result must not put its smaller nonce on
wire. Both provider apply and the final Claim need the private release fence.

The guarantee ends at the engine's irreversible release/handoff boundary.
Concurrent HTTP can still reach the venue out of order, including across engines;
this change does not serialize network delivery or promise venue arrival order.

## Selected subject and immutable claim

Proposed API shape, subject to exact source review:

```rust
// Fields and constructors private; neither type reveals the namespace in Debug.
pub struct NonceSubject { /* engine-owned stable-id namespace */ }
pub struct NonceClaim { /* domain, subject, selected slot and KeyRefs */ }
impl Slots<Identity> {
    pub fn select_with_nonce(
        &self, role: CredentialRole, domain: &'static NonceDomain,
    ) -> Result<(Selection, &Identity, NonceClaim), NonceSelectionError>;
}
impl DispatchContext<'_> {
    pub fn reserve_nonce(&self, claim: &NonceClaim) -> Result<u64, NonceError>;
}
```

Mint on Slots, indexing its own selected member. An API accepting an arbitrary
Selection plus an arbitrary Identity would not prove their relationship.
NonceClaim is not Clone and moves into the recipe; repeated prepares borrow the
same claim. Selection remains Copy and unchanged. The dispatcher validates the
claim's selected credential/signing refs, slot and routing against the current
attempt and the produced work before a provider call. A changed route, wrong
selection, foreign reservation or replacement generation fails closed. Exact
binding and refusal-before-reservation ordering are source-review gates.

Trusted setup supplies identical non-secret Identity.stable_id for equal API
keys, including different Primary/Secondary secrets and shared Futures/Spot
keys. Each slot still MACs with its own KeyRef. Different stable IDs are separate
subjects; venue code never compares keys/secrets or computes a secret digest.
Missing/invalid stable_id uses existing InvalidRequest -> InvalidInput/NotSent
configuration projection before IDs, reservation, provider or private I/O.
No fallback subject, slot-derived subject or new credential chooser is allowed.

Narrow NonceKey::stable_id to crate visibility with the mint. Keep public-agent
NonceKey::ethereum_address and existing Hyperliquid/TxFlow callers unchanged.
Legacy Kraken secret-digest allocation is removed only with its consumer cutover;
continuity across that changed namespace and mixed old/new consumers must be
certified there, not inferred from the new stable-id engine recreation controls.

## Clock and integer receipt

Add one engine-declared KRAKEN domain shared by Futures and Spot. Its floor is
RawMs(scale=1000): checked max(floor(raw_now_ms)*1000, last_reserved+1), u64 limit.
DispatchContext gains a private raw clock input from the same engine observation
used by Cx, separate from public protocol now_ms. Never use protocol calibration
for this raw nonce floor.

Overseer ruling relayed by 9cz: a raw clock above MAX_SAFE milliseconds refuses
InvalidTime; list this new ceiling in CALLERS. Finite nonnegative fractional time
is floored before integer multiplication. NaN, infinity, negative time, checked
multiplication overflow and exhaustion refuse before provider/I/O. The nonce
itself may exceed 2^53: never convert it through f64 or put it into stamp_ms.

Add distinct SignReceipt.nonce: Option<u64>, encoded in JSON as a canonical
decimal string. Accept ASCII digits only, no sign/whitespace/leading zero except
"0", and checked u64 range. Reject malformed provider results as InvalidResult;
never truncate or round. Rust, native, WASM, JS and remote result validators must
compare the exact nonce receipt, request preimage and applied wire value.
Unchanged receipts without a nonce remain compatible. Numeric JSON nonces are
refused, even when currently safe, to keep one exact wire representation.

stamp_ms/expiry_ms retain their existing safe-f64 integer rules. #2024 StampFloor
and stamp_after concern only stamp_ms. Nonce allocation/floor/release never
advances a StampFloor, and stamp_after never reserves or advances a nonce.

## Private reservation and release state

Use the existing process-global NONCES namespace, keyed by domain and subject,
with last_reserved and last_released under its mutex. Engines and recreated
engines share both values in a native process/WASM memory. Separate processes
remain a deployment coordination boundary. Do not introduce a per-engine fence.

During each eligible recipe prepare, reserve once and record private exact
attempt/generation/claim/nonce metadata in the dispatcher. Preserve the existing
&DispatchContext SignStep interface; exact bounded recorder storage/interior
borrowing and fallible preparation ordering are source-stage gates. A second
claim/domain within one preparation refuses; a failed or abandoned reservation
leaves a gap and is never rolled back or reused.

Reservation alone never supersedes another request. Supersession means a stale
same-attempt ticket/generation, or nonce <= the process-global released floor.
Thus A and B may both bind while neither has released. A then B can both release
without repair. If B never releases, it cannot prevent A from releasing.

Before provider apply, reject superseded work and wipe dropped signed output.
Before final Claim, recheck the exact current reservation, issuer/attempt,
selection, route, immutable deadline and released floor. All fallible resource,
pacing and owner admissions must finish before release commits. The global
check-and-floor update is one critical section at Released; floor becomes
max(floor, nonce), never rolls back, and no callback executes under the mutex.
No observer, write journal, pacing charge or physical send may occur on refusal.
Once Released commits, subsequent host failure is ordinary possibly-sent/Unknown;
it must never be projected back to NotSent or automatically re-reserved.

Exact two-phase composition with Lifecycle::claim is a mandatory source gate:
failed venue admission cannot burn a released floor or pacing/write credit, and
a concurrent engine release cannot slip between the floor check and commit.
Apply the same private fence to supported ordinary signed transport releases;
group release must validate all member reservations before its atomic commit.
If that composition cannot be proved, refuse nonce-bearing group admission
before IDs/credits, with no single-member fallback, until separately supported.

## Bounded pre-send repair

Overseer explicitly approved constant MAX_NONCE_REPREPARES=8 (2026-10-24).
A superseded, proven-unsent attempt discards signed output and prepares again
with a fresh reservation under its original immutable deadline. Each physical
attempt gets at most eight nonce-only repairs; neither repair nor provider
completion resets that count. Check expiry before every repair/provider call.

Exhaustion is Internal/NotSent, exactly one normal failure terminal, no wire,
physical retry, fresh command receipt, extra Plan credit or auth-health change.
Provider refusal remains its existing typed pre-send refusal. No repair creates
a business retry grant; nothing sent or uncertain can enter this loop. Reserve
and release changes are metadata, never read delivery or session authority.

CALLERS must name the raw-ms ceiling, required stable subject, decimal-string
receipt field and the constant eight with its local pre-send-loop rationale.
These are Rust signing changes; legacy non-Rust production is untouched.

## Compiling original assertions

A missing method or type is not an assertion RED. A dedicated engine integration
target uses real managed Prepare/Bind/Claim and delegated provider events. Its
queue-time adapter uses the existing Cx nonce API. A separate prepare-time probe
uses the existing legacy allocator only to expose the missing release fence;
it has no trusted subject token and is not prospective production code.

Keep outcome/order assertions when replacing these adapters with the reviewed
DispatchContext claim and receipt. Never fix a RED by serializing all managed
requests, weakening a native bound, allocating during queueing, or changing its
expected nonce order. Label additive receipt-schema assertions NEW-CONTRACT.

Required controls before runtime readiness:

1. Queue A/B then route B/A: physical claims have increasing nonces; unready
   routes reserve nothing. Reserve and clock observations use real dispatcher
   callbacks, not a hand-built future engine method.
2. A provider parks; B signs/claims; A's old provider result is refused before
   apply, wiped, and repaired once with a fresh larger nonce. A provider denial
   after B release produces one terminal/no A wire, including exhausted repairs.
3. A/B both bind, B claims first: A's old permit cannot release; one repair
   succeeds. A/B claim in nonce order with no repair; abandoned B does not block A.
4. Different subjects remain independent. Same stable-id/different-secret
   Primary/Secondary, Futures/Spot, two engines and a recreated engine share
   reservation and released-floor continuity without a secret/key-hash subject.
5. Fixed/regressing clocks and >1000 reservations/ms stay strictly monotonic;
   MAX_SAFE-ms boundary, scaled values above2^53, u64 exhaustion and invalid raw
   clocks are exact. Protocol offset and StampFloor do not alter the nonce floor.
6. Canonical decimal receipt round-trip including u64::MAX; deny numeric/sign/
   whitespace/leading-zero/overflow forms and mismatched provider/wire nonces.
7. Missing/foreign subject, route/selection swaps, stale same-attempt provider,
   cancel/expiry/reprepare, eight-repair exhaustion and no late provider action.
   Add compile-fail/API-shape coverage for unforgeable/non-Clone claims only once
   the API exists; missing symbols never count as original runtime evidence.
8. Retain all original assertion bodies through adoption. Certify native and
   WASM host exact receipt matching, immutable deadline and no extra credits,
   selected-credential checker, scoped strict checks and fresh CI after source.

The first checkpoint records actual first failing assertions and working
controls separately; downstream suffixes behind a RED are not executed evidence.

## Executed original checkpoint

The dedicated `dispatch_nonce_reservation` target compiles against actual5b,
with unchanged engine/host/provider/venue runtime. Initial execution gives five
assertion REDs and nine working GREEN controls, zero ignored:

- Queue-time real Cx reservations release `1001` then `1000` when routes resume
  B/A. Both real managed claims succeed before the ordering assertion fails.
- With the prepare-only legacy allocator probe, B claims while A's provider
  parks; A's later provider result reaches apply (`[1,0]` instead of `[1]`).
- A and B both bind; B claims first and A's old permit still returns Released
  instead of Stale. This tests the distinct final Claim fence.
- Separate engines with one probe subject show the same late-apply failure.
- NEW-CONTRACT decimal-string receipt parsing refuses the unknown `nonce`
  field at runtime. This is schema evidence, not a missing-method compile RED.

Working controls exercise both in-order binds/releases, abandoned higher
reservation, provider refusal without Bind, duplicate Claim/late provider
inertness, original queue-store continuity across engines/recreation, distinct
subjects, 2002 regressing-clock integer reservations and u64 exhaustion, old
safe-integer timestamp receipts, and closed original-schema nonce rejection.
The last control rejects malformed nonce input as an unknown field today; it
does not certify the future strict decimal parser.

The prepare probe has protocol time equal to raw time in these fixtures. It
cannot certify private raw-clock selection, trusted Slots minting, key alias
setup, mutable route/selection mismatch, nonce-bearing receipt matching, wipe
implementation, repair success/exhaustion, command terminal aggregation, group
commit or #2024 composition. Those stay source-stage controls, not silently
passing mocks. Original RED suffixes after their first failed assertion remain
unexecuted. No actual network send or provider cryptography is claimed: host
handoff is represented by the genuine successful final claim and signed packet.

Scoped target strict Clippy, rustfmt, credential-selection checker and full bans
pass on this checkpoint at jobs4. Only this target is executed locally; no
workspace/all-feature sweep, runtime certification or CI readiness is claimed.
