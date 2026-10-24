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
Kraken retains its native secondary_requires_primary:true Slots policy; engine
minting does not broaden which roles can select that secondary.
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
A receipt without a nonce remains compatible only for a claim-free attempt.
A claim-bearing result that omits or changes the reserved nonce refuses
InvalidResult before apply. Numeric JSON nonces are refused, even when currently
safe, to keep one exact wire representation.

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
claim/domain within one preparation refuses InvalidRequest/NotSent; a failed or abandoned reservation
leaves a gap and is never rolled back or reused.

Reservation alone never supersedes another request. Supersession means a stale
same-attempt ticket/generation, or nonce <= the process-global released floor.
Thus A and B may both bind while neither has released. A then B can both release
without repair. If B never releases, it cannot prevent A from releasing.

Before provider apply, reject superseded work and wipe dropped signed output.
Before final Claim, recheck the exact current reservation, issuer/attempt,
selection, route, immutable deadline and released floor. Any Session generation
and #2012 Bearer revision fence runs first (dead generation Revoked, moved
revision Stale). All fallible resource,
pacing and owner admissions must finish before release commits. The global
check-and-floor update is one critical section at Released; floor becomes
max(floor, nonce), never rolls back, and no callback executes under the mutex.
Observers, write-journal release marking and pacing charge run strictly after
the Released commit. None may occur on refusal. The floor check, infallible
Lifecycle Released commit and floor update share one no-callback/no-allocation
critical section after all fallible owner/pacing/journal admission. A refused
admission must leave the floor unchanged for a later in-order release.
Once Released commits, subsequent host failure is ordinary possibly-sent/Unknown;
it must never be projected back to NotSent or automatically re-reserved.

Exact two-phase composition with Lifecycle::claim is a mandatory source gate:
failed venue admission cannot burn a released floor or pacing/write credit, and
a concurrent engine release cannot slip between the floor check and commit.
Apply the same private fence to supported ordinary signed transport releases.
Nonce-bearing group admission refuses before IDs/credits by default, without a
single-member fallback. Future support must prove host member handoff order
matches nonce order, not merely validate all reservations atomically.

## Bounded pre-send repair

Overseer explicitly approved constant MAX_NONCE_REPREPARES=8 (2026-10-24).
A superseded, proven-unsent attempt discards signed output and prepares again
with a fresh reservation under its original immutable deadline. Each physical
attempt gets at most eight nonce-only repairs; neither repair nor provider
completion resets that count. Check expiry before every repair/provider call.

Exhaustion is Internal/NotSent, exactly one normal failure terminal, no wire,
physical retry, fresh command receipt, extra Plan credit or auth-health change.
Provider refusal remains its existing typed pre-send refusal. A genuine permitted transport retry or definitive-refusal ResendGrant creates a
new physical attempt: its own prepare reserves a fresh larger nonce and its
nonce-repair counter starts at zero. A Bearer/route reprepare reserves fresh but
does not consume the eight nonce repairs; count only nonce supersession.
Whichever prerequisite lands second must test a Bearer Stale and nonce
supersession together in one attempt. No repair creates
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
   MAX_SAFE-ms boundary, scaled values above 2^53, u64 exhaustion and invalid raw
   clocks are exact. Protocol offset and StampFloor do not alter the nonce floor.
6. Canonical decimal receipt round-trip including u64::MAX; deny numeric/sign/
   whitespace/leading-zero/overflow forms and mismatched provider/wire nonces.
   A claim-bearing provider result with an omitted nonce is InvalidResult before
   apply; a genuine granted physical resend reserves greater than its refused
   attempt, without reusing the old nonce or resetting a live repair budget.
7. Missing/foreign subject, route/selection swaps, stale same-attempt provider,
   cancel/expiry/reprepare, eight-repair exhaustion and no late provider action.
   Add compile-fail/API-shape coverage for unforgeable/non-Clone claims only once
   the API exists; missing symbols never count as original runtime evidence.
8. Retain all original outcome/order assertions through adoption, with the
   explicit legacy GREEN ports below. Certify native and
   WASM host exact receipt matching, immutable deadline and no extra credits,
   selected-credential checker, scoped strict checks and fresh CI after source.

The first checkpoint records actual first failing assertions and working
controls separately; downstream suffixes behind a RED are not executed evidence.

## Executed original checkpoint

The dedicated `dispatch_nonce_reservation` target compiles against actual 5b,
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

## Adapter porting and source certification precision

Three legacy-calling GREEN bodies are explicitly ported at source time:

- queue_store_continues_across_engines_and_recreation: construct a trusted
  selected KRAKEN claim in each engine, drive genuine route/prepare, then
  compare the three recorded reservations in the same strict-increase order.
  Observation moves from queued to prepared; the continuity assertion stays.
- distinct_subjects_have_independent_sequences: prepare the two trusted
  subjects at the same raw1000ms, then assert exact [1000000,1000001] and
  [1000000]. These numeric expectations change from the original public-agent
  domain's [1000,1001]/[1000] solely because KRAKEN scale is1000. There is no
  normalization of observed wire values and no independent-subject weakening.
- existing_counter_is_exact_for_regression_more_than_one_thousand_and_exhaustion:
  move the counter probe to engine-private KRAKEN-domain tests with an injected
  initial counter near exhaustion. Port the allocation calls, retaining the
  2002 exact-increment, decimal9007199254740993, u64::MAX and final Err checks.
  Add independent real-dispatch exhaustion coverage below.

These are declared adapter/body ports rather than a promise that all original
source bytes remain unchanged. Review those exact diffs before adoption; retain
all original outcome/order assertions and existing checkpoint in Git. The four
ordering REDs switch only their fixture construction/probe to the reviewed
DispatchContext claim. The legacy allocator is never kept as a test-only
production escape after Kraken's consumer cutover. Its current GREEN is evidence
for the old allocator, not evidence that the KRAKEN domain already exists.

A separate dispatch-level exhaustion control must show Internal/NotSent, no
provider invocation and no handoff. Add genuine same-ms dispatcher pairs,
canonical receipt '0' success and empty/'00' denials; the original unknown-field
negative control is not decimal parser proof. A dispatch-level overflow cannot
be substituted with the existing allocator Err alone.

Final Claim nonce Stale returns Err(Stale) and follows the existing repair path
in that call: Cancel the old preparation, then fresh Prepare and signing under
the same physical request identity and original deadline. Native/WASM hosts
must treat that Stale as do-not-send with no host-created terminal. Only the
engine's repair or exhaustion produces the outcome. Source review must pin
this exact observable action sequence and unchanged host behavior.

Add 32-request same-subject controls for a reverse initial bound-claim burst
(repair each stale request and release it before advancing) and ordinary paced
FIFO release. Every request in those schedules must release in increasing nonce
order without eight-repair exhaustion. This is not a promise against an
unbounded adversary: repeatedly releasing a newer request during each parked
repair can deliberately exhaust the approved local bound. A separate control
forces eight nonce repairs and verifies the one typed pre-send terminal.
If ordinary FIFO pacing alone exhausts the bound, raise that as a design defect
before source rather than weakening the working burst control.

A new physical retry resumed through #2037 Plan/Preflight session waiting
reserves only in its own prepare; parking and Session readiness reserve nothing.
A pending provider result omitting a required nonce is InvalidResult before
apply. Bearer and route repairs do not consume the nonce-only eight-repair count.

## Source-stage admission and hosted signing pins

The existing mutable `on_http_release` can commit venue scheduler state. It
cannot run under the global nonce mutex, and running it before a floor check
would require rollback if another engine releases first. The first nonce slice
therefore supports explicitly unscheduled venues only. Kraken Futures and Spot
currently use the default unscheduled hook; scheduler and group support remain
refused until their own prepare/commit proof exists.

Per fe7's API disposition, replace the old trait hook directly with one
`HttpRelease<V>` registration: either `Mutable(fn(&mut V, &mut Pending, ...))`
or `NonceUnscheduled(fn(&V, &Pending, ...))`. `Engine::new` captures the one
registration once; it cannot change between queue and Claim, and the enum
cannot express both modes. Default is the existing mutable no-op Unscheduled
admission. Convert the four existing fixture overrides without changing their
admission bodies, and remove the old method in that same source change.

A declared nonce claim on a venue without the pure registration refuses at
queue admission before IDs, signing, activation, reservation or provider calls:
InvalidRequest projects InvalidInput/NotSent. A hidden or changed claim refuses
at prepare before reservation/provider. The pure admission check runs before
the lock; under the lock only the floor check, infallible Lifecycle Released,
floor max and pacing mark run. Observers and journal notifications follow.
There is no mutable scheduler callback for a nonce-bearing release.

The decimal receipt component has separate bounded schema/source CLEARs from
fe7, m26 and 4y. It supplies no reservation, subject or Claim authority. Source
controls must still prove missing/substituted provider nonces refuse before
apply, and run MAX/MAX+1 parity through the real WASM signing path.

Hosted signing must retain the exact engine-reserved nonce with its captured
SignRequest and provider result. The legacy `Input::Http`/`hostedSigningClock`
compatibility adapters currently reconstruct stamp-only receipts; adding
`nonce: None` there does not make them support nonce-bearing requests. Before
Kraken adoption, its full hosted path must carry the reserved canonical decimal
nonce end to end, with no server-side second reservation or renewed stamp.
An adapter that cannot carry that capture must refuse before private I/O;
discovering the omission later as InvalidResult is not an admission policy.
This is a consumer/hosted composition gate, separate from schema clearance.

## Retained selected-read nonce mint

Credential-core owner m26 approved `queue_with_nonce(domain, recipe)` on
AdmittedRead and ReadLeg. The recipe receives the retained `&Identity` and a
fresh non-Clone NonceClaim minted by the same crate-private factory as Slots.
The private Selection/Identity snapshot has no public constructor or mutable
accessor. Each physical queue, including a native retry, mints a fresh claim;
no key reselection or reservation happens while a read is retained.

The ordinary selected-read queue refuses a declared nonce claim before queue
IDs. The nonce queue requires the exact minted authority, refusing a missing
or substituted declaration before IDs. Missing stable_id refuses InvalidInput
through the existing typed queue error, before recipe/provider work. Both
return the exact pending wrapper for the caller's contextual settlement.
Direct managed requests with a trusted Slots claim remain supported for
non-selected-read operations, including Kraken mutations; the raw non-managed
signing boundary continues to refuse nonce claims.

Source controls must cover both wrappers, retained Secondary identity despite
venue-slot replacement, missing subject before recipe construction, plain-queue
refusal even for an otherwise valid claim, substituted/missing nonce authority,
and a real ReadFailure retry with fresh signing/request IDs and increasing nonce.
This is an additional API/source review gate before Kraken account adoption.

## Real WASM proof boundary

The selected-read source and immutable `NonceClaim::slot()` metadata accessor
have bounded engine, credential-core and consumer review clearance. This does
not certify Kraken's native private callers or registry adoption. The nonce
branch stays local until the independent real WASM/HMAC proof is cleared.

At fixed raw time 9007199254740 ms, a fresh KRAKEN subject first reserves
9007199254740000. Reservations 994 and 996 reach 9007199254740993 and
9007199254740995 respectively. The proof must observe these genuine reservations
across the JS/WASM boundary, their exact recipe preimages and MACs, and a stale
Claim repair whose fresh signature uses a strictly greater reserved nonce.

The independent reviewer accepted a separate u64::MAX proof: canonical receipt
codec/parser traversal and the real recipe MAC in WASM, without claiming an
engine reservation. A MAX_SAFE raw timestamp cannot directly seed that counter
value; reaching it by unit increments is impractical. No production counter
seed, unsafe clock or new test seed API is introduced. Existing engine-private
near-limit arithmetic controls remain the allocator exhaustion evidence.

Spot form fields keep native order: nonce is appended last when other fields
exist. Reference MAC computations alone, recipe-only probes, stub engines and
native host controls do not establish actual-family WASM adoption. The proof
receipt must identify its exact source, module hash, commands and scope.

## Nonce feature isolation after the first draft size gate

The first draft run `40129055938`, head `7cfb791b1f9`, builds every family but
fails optimized size gates: Bitget/USDC is 1,374,035 against 1,363,968 bytes;
Bybit is 1,345,189 against 1,343,488. Replay jobs stop at the same budget gate;
their missing replay/capsule outputs are consequences, not independent CPU
failures. No budget, throughput profile or CI policy may be changed to fix this.

Add a non-default `safe-cex-engine/dispatch-nonce` Cargo feature. Only the Kraken
venue dependency enables it in production; Kraken Spot shares that family and
dependency. Registry/combined builds acquire it only when Kraken is included.
Bybit, Bitget/USDC, Aster and every other standalone family must leave it off.
The native host exposes an explicit forwarding feature for its nonce controls;
ordinary native/engine builds do not opt in implicitly through test-support.

Keep public receipt fields, opaque types, recipe signatures and ordinary selected
queues source-compatible. Feature-off builds refuse nonce mint/reservation before
IDs/provider/I/O and cannot acquire a nonce claim; a declared or invented nonce
at prepare/Claim still fails closed. Compile-time guards at mint, prepare and
Claim remove reservation, global released-floor access and nonce repair machinery
from unrelated families. The legacy queue-time nonce allocator used by other
venues remains independent and unchanged. Nonce-free receipts keep their exact
schema; only feature-on builds admit the canonical decimal nonce field. The Rust
field remains source-compatible, while feature-off wire nonce fields refuse.

Feature-on behavior retains the exact trusted snapshot, receipt binding, atomic
release ordering, independent StampFloor and eight-repair contract. It introduces
no nonce fallback, new reservation seam, namespace migration or native adoption.
Existing nonce-specific tests run explicitly with dispatch-nonce enabled, retaining
their assertions. Add a feature-off control that refuses trusted mint/reserve
before recipe/provider/I/O and preserves ordinary stamp-only signing.

Before implementation, obtain 9czdjh's consumer/API feature-map review at an
immutable plan pin. Then measure optimized modules with the existing build.sh
profiles and Binaryen 132: exact landed baseline versus feature-off candidate for
Bybit and Bitget/USDC, plus Aster after actual `9cc6eac669c` composition. Record
bytes/hashes and any remaining difference; all fixed budgets must pass. Verify
Cargo feature trees show dispatch-nonce absent for those families and present for
Kraken. Run the feature-on nonce/selected-read/native/receipt/JS controls, feature-off
controls, formatting, strict checks, workspace all-target check and normal ratchets.
Re-run the existing engine+actual-recipe WASM proof with the explicit feature.
Only after these gates and bounded source review: one freeze push and one
PR-triggered run. The stacked recipe draft waits for that exact immutable head.

The feature-off receipt validator is emitted out of line to share its unchanged
strict checks across ordinary dispatch paths. This is a function-level outlining
choice, with no compiler-profile or feature-on behavior change; pinned Bitget
measurement recovers another 478 bytes. Final receipts report residual public
API compatibility cost against the pre-nonce baseline rather than byte equality.

The seeded CI feature-mode dictionary also names an explicit dispatch-nonce OFF
control (`http-plans,order-locks,test-support`), alongside the existing seeds and
the explicit nonce ON control. This runs ordinary selected reads and strict
feature-off refusal without depending on family closure feature unification.
