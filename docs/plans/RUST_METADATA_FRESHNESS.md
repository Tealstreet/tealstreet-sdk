# Engine-owned metadata reads and settings freshness

Status: shared engine/Store freshness and BingX query owner are implemented.
WEEX explicit leverage queries retain the incoming managed Context reader and
native settings revision; shared observations apply to its background settings
proposals and Store write fences, not query coalescing. The earlier design and
checkpoint sections below describe the original two-adapter scope; this current
boundary and the Plan/execution continuation supersede its WEEX query-cutover
claims. BingX joined Plan adoption consumes the reviewed composition source
under the production contract below; the legacy query path still remains at
this checkpoint.

Owner assignment: architecture audit §G, rank 13. Start from actual
`ecc84a798163c9f61ea9439e90001a98bb07b907`; retain the later decision-only merge
`371488b44b084a9adea188a724bd5ae2cb959f33`. Capability conformance #1737 and history
fan-out stay separate. No handoff, owner ledger or overseer scratch files belong
in this change.

## First two adapters and demonstrated boundaries

- WEEX: `src/lib.rs::fetch_leverage/leverage_response` reads one native
  `symbolConfig` response. `account.rs` separately applies configuration reads;
  a global settings revision fences writes, but overlapping metadata commands
  issue duplicate reads and do not share per-symbol read ownership.
- BingX: `account.rs::fetch_metadata/on_response` stages leverage then margin.
  It has local settings revision, per-symbol query generations and a sticky
  publication flag for reads issued during pending writes. Superseded reads
  return current rails with validated per-side fallback; overlap alone must not
  fail or manufacture null rails. Two commands currently issue separate reads.

These are different response/continuation shapes, not two copies of one adapter.
Keep endpoint arguments, credential slots, parsing, native error/code tables,
fees/financial rows, ordinary account reconciliation and existing rate floors.
Inventory every settings mutation, not only explicit query responses: BingX
account config events/position-derived settings and trading ACKs, plus WEEX
config/position reads and trading ACKs. Their settings proposals must use the
shared ownership boundary without replacing native financial admission.
The current SDK `updateSymbolMetadata` skips absent capability/public-only
contexts and uses the runtime policy for cached/flat private symbols. Preserve
that behavior and pending-start waiting; cached markets alone are not readiness.

## Shared ownership and commit contract

Add one engine metadata module and an engine-owned per-symbol state boundary
(associated with Store), rather than two venue maps or a helper that merely
returns a publication boolean. A typed attempt token binds the symbol,
read identity/epoch, settings/write revision and publication eligibility. Keep
state and joined command waiters bounded and fail closed on exhaustion.

1. Admission joins a same-symbol read only when its revision/eligibility is
   current; retain each command owner. A caller needing a newer write/config
   revision cannot join an obsolete attempt. A later waiter cannot upgrade an
   attempt's sticky publication restriction. One native logical read serves an
   eligible group: one request per leg (BingX leverage plus margin; WEEX config).
   Different symbols remain independent. New attempts receive globally distinct
   tokens even after bounded state is retired and the symbol is recreated.
   Retired/duplicate/late tokens cannot publish or finish another attempt.
2. Stage a complete validated metadata proposal before committing. BingX's
   leverage response alone cannot publish; its margin leg belongs to the same
   attempt. WEEX's setting and native config cache move together. Core admission
   owns atomic settings publication; the adapter retains native codec/cache data
   and performs any approved native-cache update in the same synchronous commit.
   Do all fallible validation beforehand; no parsing, await or fallible work may
   separate approved Store publication from native-cache replacement.
3. Settings writes/config events invalidate the relevant symbol or explicit
   account-wide scope. An accepted-write revision is never bypassed for progress.
   Reads admitted while a write is pending retain their publication restriction
   after that write finishes. Preserve existing conservative invalidation on
   write issue/refusal unless a separately proven correction is authorized.
4. A superseded valid read returns usable current settings without stale STORE
   publication. Preserve native result contracts: BingX uses independent cached
   rails with validated per-rail read fallback; WEEX uses a complete cached
   rails/margin triplet or the whole validated proposal. Never publish a mixed
   or partial proposal. BingX already discards a superseded margin response
   before parsing it, using its previously validated rails/current cache; retain
   and test that stage-specific behavior. Eligible margin proposals must parse
   and validate fully. Native/transport errors and required-stage parse failures
   still fail; this refactor does not strengthen stale-stage parsing policy.
5. Background settings/config proposals use the same per-symbol commit fence:
   an older account read must not overwrite newer metadata or accepted writes.
   Preserve initial/partial readiness rules and newer-position fences. This
   authority covers settings only, not financial row admission or execution.
   Specifically, WEEX invalidate_settings also bumps its position revision;
   BingX approved metadata commit bumps positions before settings publication.
   Keep these native financial fences at their current lifecycle boundaries;
   a new settings token is not a replacement for them.
6. Successful or failed completion fans out exactly once to its joined command
   owners. Preserve typed errors/outcomes; do not broadcast results, invent
   success for an empty/partial proposal, auto-retry writes or start mirror engines.

Ready/public-only policy must be explicit at the shared boundary: automatic
metadata hooks do no private I/O before readiness or in public-only mode. SDK
explicit operations retain their pending-start/typed-refusal handling. Separate
that production lifecycle from raw native embedders: WEEX's existing prepared
market/credential tests issue explicit reads without Engine::start. Do not add
a blanket start requirement or change that raw contract accidentally. Validate
actual adapter lifecycle flags and read intent before selecting the core gate.

## Scheduler, request lifecycle and other owners

Reuse `engine::read_scheduler` for scheduling, bounded reservations, shared pauses
and retries where adopted; do not add another retry/backoff/admission algorithm.
Metadata owns logical waiters/proposals/freshness, not dispatch or authentication.
Coordinate engine exports and Store changes before implementation.

The pending RequestContext release seam (#1741) is a dependency for physical
release integration: no-Cx `on_http_release(pending, &HttpRelease)` returns
`Admission::{Unscheduled, Scheduled(absolute raw lease), Denied}`. The scheduler's
`admit_http(token, release)` starts a lease at the first actual physical release;
continuations retain its original raw-time deadline and check pause/revocation
on each leg. Only validated progress renews it. Do not call `mark_issued` when
queueing and do not turn denied scheduled work into unscheduled work. Pure core
tests can proceed now; use the actual merged release API before production
adoption/final proof, without a venue-local workaround.

The row lane's validated financial-snapshot token is future work, with no
reserved generic API today. Expose/reuse a common engine token only where the
semantics actually match; do not duplicate it in venues. No changes to coverage,
execution, num, signing/dispatch, host executors or history are intended. Ask the
owning lane before any scope expansion there. Clock/scheduler lane confirms no
WEEX/BingX metadata overlap; its first scheduler adoption is Bitunix balance.

## Committable phases and original-RED evidence

1. Audit current lifecycle, publication and config-write call sites in both
   adapters. Commit native original-RED controls for same-symbol coalescing and
   stale publication/order where current code demonstrably fails. Retain existing
   successful race/failure tests as preservation controls; do not call a
   compile-failing new API an original-RED behavioral proof.
2. Implement the shared engine owner, opaque tokens, bounded waiter state and
   validated atomic settings commit. Direct core tests cover late/duplicate and
   superseded attempts, independent symbols, pending/accepted writes, partial
   proposal rejection, failed/canceled attempts and command-owner isolation.
3. Adopt WEEX and BingX through that same core, including competing background
   settings publication. Remove their replaced local generation/coalescing
   policy, preserving native staging and error codecs. Use actual release
   integration once available; no second scheduler or weakened write fence.
4. Update caller/audit/quality status only for demonstrated behavior, review the
   whole change independently, and run final focused gates/full CI. Remaining
   families, native auth behavior and NX/live adoption stay open.

For each family prove: cached and uncached/flat symbols; one request per leg of
each eligible joined group; both owners receive one success/error; independent symbols;
out-of-order old/new reads; writes between issue/legs/commit; pending-at-admission
stickiness; refused writes; malformed or missing eligible rails/margin and the
superseded-margin discard control; partial initial data; genuine errors;
automatic public-only/no-start no-private-I/O and prepared native explicit-read
preservation; retry after failure
and cleanup after timeout/cancellation. BingX's second leg cannot extend the
logical raw deadline or publish its first leg. WEEX native config and settings
must agree after the shared commit. Prove existing metadata-to-position fencing
and mode-wide invalidation are retained.

Run focused engine/WEEX/BingX plus Binance native tests with four Cargo jobs,
strict all-target Clippy/fmt and relevant SDK hook regressions. Use Corepack Yarn
only, never the Volta shim or npx; do not rerun the account-policy projection
check. No local release WASM, full replay/size/timing matrix. CI runs shipped
artifact and replay gates; declare only owner-approved behavioral differences.

Independent plan/correctness/adversarial reviews must answer the architecture
gate: this is one engine choke point adopted by two different families, not two
venue-specific fixes. Integrate actual shared dependencies before final proof,
announce the merge window, then use a separate guard and separate exact-head
merge on green. Verify actual parents/tree/owned and incoming blobs; report pin.

## First original-RED checkpoint

Before any implementation change, two coalescing/fan-out controls in each
adapter compile and fail because two same-revision commands emit two reads
instead of one. They cover success and error fan-out; BingX expects one request
per native leg, not one request total. The pre-existing 69 BingX account and 41
WEEX trading tests still pass with only these four new controls excluded.
That checkpoint precedes implementation. The four controls now pass through the
shared owner; they do not alone prove the other freshness/cleanup boundaries.


## Implemented core and two-family checkpoint

`engine::metadata` owns bounded attempts (256), joined command waiters (64 per
attempt), symbols (4096), opaque monotonic tokens, pending-write reservations
and freshness. Store owns validated publication: explicit complete proposals
commit together, while background settings observations reject older issue
stamps or later mutations. Native partial hedge observations validate every
supplied rail but cannot satisfy a complete metadata proposal. Generic Store
settings mutations also invalidate the shared owner. Observation issue order
is separate from publication revision, so a valid older account read can still
supersede pending metadata, but cannot overwrite completed newer metadata.

WEEX uses one config read and atomically replaces its native config cache before
owner notifications. Its complete-triplet fallback, prepared native calls without
start, secondary-only GET selection and last-valid duplicate config selection
remain. BingX stages leverage then margin, discarding a superseded margin leg
before parsing it and retaining independent validated rail fallback. Its approved
metadata publication still bumps the native positions fence. Settings write
issue/completion and background/event settings sources feed the same engine
boundary. Neither adapter keeps a second metadata generation/pending-write map.

Four original-RED native controls are now GREEN. Sixteen direct core tests cover
joining, errors, stale/duplicate tokens, finite/partial validation, fallback
policies, independent symbols, scoped/sticky writes, background issue/completion
order, bounded capacity and cleanup. Native controls additionally exercise old
background config versus completed metadata, refused-write stickiness, native
cache/Store agreement, secondary-only credentials and duplicate config rows.
Existing BingX race/malformed/snapshot and WEEX settings/financial controls stay.
The prior BingX newer-query race control now expects eligible queries to join;
its pending-write assertions remain. A baseline hedge-setting fixture is seeded
before issuing its snapshot, since a Store mutation after issue now fences it.

Post-integration focused engine/WEEX/BingX/Binance checks pass 990 native tests,
strict all-target Clippy and targeted fmt. Forty existing SDK runtime capability
and metadata-hook controls pass. No local release WASM or replay matrix ran.

Correctness and adversarial reviews are clean after fixing validated observation
admission, secondary-only WEEX credentials and duplicate-row cache coherence.
Architecture gate: **one shared engine/Store ownership and publication core,
not per-venue implementations of a cross-cutting concern**. Actual #1744
`2c5c5f6987f35cafc0e272282e54da37c0a66d4f` (including #1742) is integrated;
all nonshared owned/incoming blobs and both added README row sequences were
verified preserved.

Physical RequestContext/Scheduler release admission remains deliberately
unwired until actual #1741. No lease starts at queue time, and no new local
retry/pause algorithm substitutes for the pending shared seam. The draft cannot
claim per-leg physical-release deadline enforcement, completed scheduler
adoption, final replay equivalence or production readiness. The actual merged
API, lease/denial controls and final shipped-artifact/replay CI proof are still
required before readiness. Other families remain future adoption.

## Landed-core continuation and exact remaining dependencies

Actual #1741 `b2fe97324763e1e2702c69ac5b839da37102928b` and #1759
`9bf754a01149d373c522728edbebb46abc5ae5b2` are integrated. This continuation
replaces metadata's independent scalar clock with the existing issuer-bound
`fence::Epoch/Stamp`. Token and Observation become opaque Clone values; attempt
identity, symbol/global mutation stamps, background issue floors and account
settings revision retain their distinct policy. Compare stamps only through the
one metadata Epoch; never use its latest revision as a universal publication
fence. Foreign-Store observations/tokens and permanent exhaustion fail closed.
Native financial order/position fences remain unchanged.

Add one shared metadata scheduler adapter around `read_scheduler::Scheduler`,
with bounded logical reservations, original raw deadline, same-attempt
continuations and neutral retirement. Admission delegates to `admit_http` at
physical release; stale/revoked/paused/expired attempts return Denied, never
Unscheduled. Tests cover no lease at queue time, first-release anchoring,
continuation nonrenewal, pause/revocation/expiry, invalid times and cleanup.
The scheduler response lease is an observation, not a replacement physical
timeout: actual lifecycle deadlines and cleanup must be wired before any
production-adoption claim.

Owner clarified the additional prerequisite on 2026-10-23: private WEEX/BingX
metadata cannot use managed HTTP until their real dispatch-signer migrations
land. `request_managed` does not carry native signed headers, and no raw-secret
recipe, public credential placeholder or legacy-header compatibility seam is
authorized. BingX checkpoint `e912867d991` is future work; WEEX follows it.
Existing metadata rails routes in these two adapters are private. Public-only
hooks continue to skip private I/O; no fictional public rails endpoint is added.
Unsigned managed-read machinery may be exercised through the shared boundary
without claiming private production wiring.

Managed coalesced completion also needs shared engine group authority: every
joined owner retains its own generation/deadline/cancellation, while a physical
delivery for the first owner cannot numerically complete another owner. Keep
the current adapters explicitly legacy until that authority and actual expiry
cleanup are integrated. The history managed Completion bridge remains separate.

Verification: add original-RED cross-Store token/observation controls before
Epoch implementation; retain all existing coalescing/settings/financial tests.
Then run focused engine/WEEX/BingX/Binance tests with four Cargo jobs, strict
Clippy/fmt and existing SDK metadata-hook controls via Corepack Yarn. Independent
correctness/adversarial reviews precede the final checkpoint. Fresh current
landing integration, CI and source-bound proof are still required. This is one
engine policy boundary, not per-venue scheduler/signing workarounds.

## Issuer and release-contract checkpoint

The metadata clock now uses the existing common Epoch/Stamp primitive, including
foreign-Store identity and permanent exhaustion. Two new assertion-RED controls
failed on the original scalar implementation (equal tokens across Stores and
foreign observation publication); both are GREEN. Account settings revision is
now an opaque issuer-bound snapshot with a current-validator. Observation issue
and unrelated symbol reads do not invalidate that account mutation fence.
Twenty metadata unit controls and 364 WEEX/BingX native controls pass.

Eleven additional engine controls exercise the existing Scheduler directly,
including real unsigned managed HTTP prepare/bind/claim, typed unsent denial,
neutral cleanup, first-release anchoring, same-lease continuation, exact admission
and response expiry, pauses before/after release, stale canceled reservations,
invalid/sub-ULP times, and superseded-publication fallback versus read liveness.
A two-leg unsigned probe uses `http_leg_context`: the second release retains
both the original logical deadline and scheduler lease; a denied second leg
cannot publish partial settings or succeed. No replacement scheduler wrapper
or second policy is needed. These are shared-kernel/unsigned-adapter controls,
not proof of private metadata production adoption or active response-lease timers.

The observer-group capability will be the same engine mechanism as Hyro fan-out,
owned by the RequestContext lane. Its acceptance cases include independent live
observer generations, first/follower detach and expiry, command-ID reuse, late
join without deadline extension, per-leg denial and exact once-only terminal
delivery. Publication freshness remains separate from observer/read liveness.
No numeric managed completion, raw-secret signing recipe, public-credential
placeholder or legacy-header compatibility path is introduced.

## Exact legacy inventory checkpoint and remaining conversion

The integrated source includes actual RequestContext #1741, common snapshot epoch
#1759 and Hyperliquid history #1765. The complete native regression run passes
1,238 tests; all 31 metadata controls, 40 SDK hook controls and strict all-target
Clippy pass. Production private metadata requests remain on the explicit legacy
bridge until the family signer migrations and shared observer-group authority
land; the release controls above exercise unsigned shared-engine probes only.

The legacy inventory prunes WEEX `lib.rs` from 19 to 15 sites. BingX `account.rs`
shrinks from 21 to 16 under one exact owner-approved old/new map: three GET calls
have re-expressed pending payloads, and one genuinely new `cx.fail(command, error)`
reports metadata admission refusal before a request is issued. This last site is
not described as a same-call rehash. The landed history Completion still calls
numeric completion, and this legacy entry owns no Context; manufacturing a new
HTTP Context solely to reject can itself fail capacity/state/time checks and
cannot guarantee that refusal. The owner therefore approved the bounded map,
with no new HTTP, signing recipe or site kind and no general exemption.

**Required follow-up:** replace that pre-request numeric refusal with the shared
observer-group completion authority when it lands, together with the joined
metadata read conversion. The exact-map tests pin both source revisions and map
hashes; existing controls reject regrowth, altered original maps, unrelated paths
and attempts to override a newer pruned map. Every other inventory map is retained.

## Reviewed read-authority stack and paired-public delivery checkpoint

Genuinely integrate reviewed #1792 `ea0f416a2e3` (including actual #1784) under
owner exact-stack authorization. The independently audited merge preserves all
incoming paths and both exact bootstrap maps, with checker logic AST-identical;
actual #1792 with that ancestry remains a landing prerequisite.

Apply the existing shared `Cx.validate_read_http_delivery(root, leg)` where this
branch already has Context authority: the real managed unsigned single-command
metadata release fixture. Retain the original logical root separately from its
current physical child. Validate before any settings/native publication and
synchronous completion, and complete/fail with the original root. Original-RED
controls substitute the child as root or an unrelated live command root on the
second leg, while the metadata freshness token remains current: neither may
publish or complete. The positive two-leg read must still publish once with its
original deadline/lease. Use the shared validator directly, no copied checks.

WEEX/BingX private native metadata requests remain legacy and cannot gain managed
rights from a helper call. Their dispatch signers and shared execution/observer
core are separate prerequisites. This checkpoint adds no private production
adoption, joined completion authority, raw-header adapter, numeric managed
fallback, new lifecycle API or send permission. Native/freshness proof remains
separate from delivery authority. Review and run focused engine/WEEX/BingX gates,
strict checks and exact inventory controls before pushing the refreshed draft.


Paired-public delivery receipt: regression-only `654bceca586` compiles against
the authorized #1792 stack and fails the assertion that a wrong root cannot
publish a still-current metadata proposal. Calling the shared paired validator
before publication makes child-as-root and unrelated-live-root variants pass.
The positive two-leg path retains the original root for completion and the same
logical/scheduler deadlines. All 1002 engine/WEEX/BingX native tests pass, including
these controls; strict all-target Clippy/fmt, bans and fourteen exact provenance
controls pass. Native private adapters are unchanged by this checkpoint.

## Plan and execution ownership continuation

The authorized Plan prerequisite is reviewed #1873 `2fe13f4d892`, with core
integration `450555399be`. Its later head `7585d3beb49` adds landing ancestry;
the Plan owner confirms all 90 engine entries are identical. Stack on the exact
authorized pin, then integrate actual shared landings before final CI. The
engine `http-plans` feature is default-off. Adopting BingX must enable it in its
own dependency graph; combined workspace feature unification is not proof of a
standalone family build.

This continuation replaces BingX's numeric metadata admission refusals and
joined numeric terminals with genuine command-entry and shared execution
authority. Existing Store Epoch/Stamp settings publication remains a separate
proof. A live Plan does not make a superseded settings token current, and a
current settings token does not make a stale delivery or observer live.

### Ownership and integration

- This lane owns shared metadata/settings policy, its controls, BingX's
  `account.rs::fetch_metadata` pre-admission checks and metadata-specific caller
  documentation. Preserve unrelated financial, signing and history changes.
- The native BingX paired-read adapter is #1884 `f833fd19e7e`, owned by
  claude-4yjlw0. Its agreed handoff may accept an already admitted Preflight and
  the shared metadata token/work key. Only its current Plan-constructor region
  may change to the agreed shared planned-execution constructor; the resulting
  Plan, Slot and Context continue through the existing native Leg.
- Preserve #1884's two serial steps, each with two fresh 20-second native
  attempts: 40 seconds per call and an immutable 80-second whole-read bound.
  Preserve secondary read-key selection, signature-refusal retry evidence,
  dispatch recipe/custody, signed managed release, typed callbacks and
  `done_read_plan_after_data`. No local retry, timer or deadline is substituted.
- Account `positions_revision` and `bump_positions` remain native financial
  fences. Replace only metadata eligibility's duplicate generation/revision
  checks with the shared Store token. Native settings-write and snapshot paths
  must still invalidate the relevant metadata publication. Retain
  `settings_revision` wherever another native consumer still uses it.
- Execution and planned-execution source remains claude-fe7lgm's shared core;
  Plan/lifecycle source remains its owning lanes. No allocator, observer table,
  journal, cancellation clock or execution core is duplicated here.
- WEEX's native codec, cache/publication ordering and positions fence remain
  covered. Do not silently reinterpret a legacy WEEX completion as managed
  authority or claim its production cutover from the BingX proof.

The read-only stack preview has conflicts in BingX `account.rs`, WEEX `lib.rs`,
the legacy inventory and three shared append documents. Resolve shared append
documents incoming-first with each own block once. Preserve incoming signer,
typed callback and native routing changes in source; agree the exact remaining
metadata hunks with their owner before resolving them. Inventory changes must
be exact reviewed operations, with no broader bootstrap or exemption.
The checker also retains exact temporary `STAGING_BOOTSTRAPS` sign_http bounds
for BingX and WEEX. Remove a staging entry in the same PR that retires its last
site; never re-key, widen or introduce another staging entry to permit this work.

### Admission, joining and publication

Initial refusal belongs to a genuine command-entry loan. Missing credentials,
unknown market and metadata capacity checks cannot call numeric `cx.fail` or
obtain a synthetic HTTP Context merely to complete. The founder uses the
reviewed ReadOnly Preflight and the shared atomic planned-execution admission.
A follower uses its own genuine Read root and shared join authority. Failure
must leave no physical request, stray observer, retained slot or numeric retry.

The complete coalescing key includes normalized symbol, trusted account subject,
credential slot/reference and intent, read kind, full native-work identity,
publication policy and fallback policy. Return-only and publish-if-current,
independent rails and complete-setting fallback must not accidentally join.
Uniform terminal projection is sufficient only within one such key. Settings
freshness storage does not become a second list of numeric completion owners.
Preserve native read supersession: a newly admitted different-key read of the
same symbol, including return-only, supersedes an older publish read. The older
read may answer through the existing current-settings fallback but cannot
publish; completion or failure of the newer read cannot revive its token.
Pin the interleaved publish/return-only case and signature re-sign after a Store
token change. Exercise the actual dispatch recipe with a fake provider.

Only an exact validated Plan-slot delivery may enter a Store-capable native
codec or publish. Validate before native state changes, check the Store token
separately, and settle only the matching native call. Only validated current
DATA progresses the existing read lease. Signature refusal, admission, reprepare,
retry and late joining cannot renew the logical bound. Typed preparation and
transport failures remain typed. A Plan or slot is never cast to an original
Read Context, and direct execution completion cannot bypass a Plan seal.

### Execution-owned composition and source gate

The ownership decision is resolved: the physical Plan belongs to
`Owner::Execution`, with an explicit founder Observer. The group owner's local
`e8914194951` implements this design, but independent source review and a frozen
reviewed pin remain required. Do not consume the superseded founder-owned
`779b8891ecf` implementation.

The proposed shared constructor is
`begin_planned_execution(&preflight, steps, policy, Window::UntilTerminal, key)`
returning `(Plan, ExecutionHandle, founder_root)`. The founder root permits
observer detach/completion only and cannot send. Followers retain their own
bound capped by the Plan root; joining never extends the physical deadline.
The adapter must map the complete work key to the handle because the engine
does not look executions up by key and terminal projection is uniform.

With a live peer, founder detach between first-leg DATA and second-leg issuance
must preserve the second leg. Last-observer detach between legs must revoke
unsent slots, deny delivery before codec even with a current Store token, and
prevent publication. Event-boundary retirement also keeps late replies out of
the venue codec. Exercise a two-step Read Plan with a fresh-attempt read retry;
one-slot core evidence alone is insufficient for this adapter.

### Original REDs and implementation gates

Commit original assertion controls before production changes, recording exact
source, fixture and assertion separately from compile/API failures:

1. A duplicate command ID cannot use the unknown-market admission path to
   numerically fail an already active metadata owner. A distinct invalid-market
   command remains the working refusal control.
2. A valid private metadata read requires real managed Prepare/Bind/Claim before
   native HTTP, and one complete paired native result; no legacy signed path.
   This is a RED against the old `7e999` metadata branch, but an existing positive
   control against #1884 `f833`: do not label it a new defect in that adapter.
3. Same-key peers share native work, terminate once each and retain independent
   bounds. Different account/key/intent/native-work/publication/fallback keys
   cannot join. Late joining cannot extend the Plan or existing lease.
4. Initial capacity/refusal and mixed-owner denial precede native effects;
   stale/reused command generations cannot complete or publish through a
   numeric fallback. Missing-key and unknown-market controls retain their native
   text when entry authority is available.
5. First/follower detach and expiry, all-observers-gone with a current Store
   token, exact-deadline denial and a live-peer positive run before the native
   codec. A Plan-founder-only approximation does not satisfy these controls.
   Until the shared authority exists these controls are explicitly failing or
   absent behind this landing gate, never ignored or weakened. Their absence
   blocks landing and cannot be replaced by a documentation disclaimer.
6. Wrong slot, sibling delivery and wrong physical attempt cannot decode,
   publish, settle or complete; first-leg DATA alone permits the next leg.
7. Preserve settings-write/snapshot invalidation, fallback rails, superseded
   margin discard, native position fencing and the 20/40/80-second retry matrix.

Review this plan and test design independently before implementing. First commit
the reproducible native-entry/managed-release REDs on unchanged source; retain
them through the exact Plan integration. Review integration preservation before
adapting fixtures to native signer changes, keeping the original assertions.
Wait for the frozen reviewed composition source before group production
wiring. Use build/test concurrency two. Run focused metadata/BingX/WEEX native
controls, standalone feature-on/off checks, strict Clippy/fmt, bans and the
existing scanner controls; shipped WASM and replay gates remain CI work.
BingX replay CPU and WASM size are explicit landing gates because `http-plans`
enters its standalone family graph. If WEEX `lib.rs` changes, its CPU and size
gates are required too. Landing `2badd1cb0e3` has reported BingX CPU ratios
1.32x/1.38x and WEEX 1.59x/1.71x; retain those baseline receipts and measure the
candidate in normal CI. Report a miss rather than waiving or changing a gate.

Update CALLERS in the implementation commit: genuine admission/refusal,
independent observer semantics actually provided, native retry bounds,
freshness versus liveness, incomplete authority boundaries and required caller
fallback handling. Keep the Plan/composition and family adoption evidence
separate. Actual shared inputs, fresh green CI, independent review, an explicit
window, a fresh separate guard and a head-matched merge remain landing gates.

Original run on unchanged production `e5306f26121` compiled four new native
controls: managed release is one assertion RED (legacy HTTP before Prepare).
Duplicate-ID refusal, distinct invalid-market refusal and the native paired
rails result are three GREEN controls; the duplicate-ID case is not claimed as
a reproduced defect. The earlier missing cargo/rustc shell runs did not compile
tests and are not RED evidence. Record new entry-authority REDs against the
integrated Plan/#1884 source separately, without weakening these assertions.

### Exact Plan-stack checkpoint scope

The exact `2fe13f4d892` integration preserves WEEX `lib.rs` byte-for-byte,
including its managed explicit leverage read and native settings revision.
Only background settings observations and Store write fences adopt the shared
freshness boundary there. Five own unlanded query-cutover tests are removed by
name in QUALITY; every incoming WEEX test remains. BingX typed signing failures
retain the provider error and release settings-write ownership before a later
eligible read publishes. A gated provider installed before startup delegates
to the same native signing recipe with refusal off; its emitted URL, headers
and body match the ordinary fixture byte-for-byte.

At this checkpoint 416 BingX/WEEX native tests pass across 26 targets, with the
one original legacy-release RED explicitly filtered and independently rerun as
an assertion failure. Engine all-feature/all-target controls pass 908 across
11 targets; the four original entry test bodies remain verbatim. The scanner's
15 controls pass. The scanner/bans still refuse historical-pin inventory growth
against the current landing and restored incoming WEEX sites against the older
branch parent. This checkpoint is not ready for landing: integrate current
actual source and complete the real managed conversion before claiming those
gates pass. No staging entry is changed or enlarged.

### Original genuine-entry capacity refusals

On unchanged production `024f995adb3` (actual #1873), two new compiled controls
fail the final metadata admission assertion: unknown market and missing key.
The fixture delegates the native BingX capability record and handler. It fills
exactly 256 genuine command-callback Read roots at a fixed clock, validates
each original root and live Context, then a genuine ReadOnly `begin_preflight`
control proves `Rejected(Capacity)` and captures its exact typed terminal. A
test timer completes one retained owner through `done_context`; preflight
admission then succeeds and both native refusal texts remain unchanged.
All these working controls run before the final expected RED comparison.

With capacity exhausted, the legacy metadata path still emits its native text
with `NotSent`, while the genuine Read control emits `Too many requests in
flight`, `Internal`, and no write outcome. The expected replacement uses that
exact engine error, without inventing `NotSent` for a Read. Each command has one
terminal and no HTTP, sign, managed release, deactivation or authentication
failure. Both tests reach the final assertion; an earlier wrapper-capability
setup failure and a physical-capacity-only prototype are excluded from this
command-owner RED evidence. The four earlier entry test blocks
remain verbatim. No production source or engine internals change in this receipt.

Independent rerun of the refined test tree reports 75 account tests passing and
three intended assertion failures: these two command-owner capacity controls
plus the original managed-Prepare RED. Source/adversarial review is CLEAR; the
only follow-up is a fixture comment explaining why native startup I/O is omitted.


## Managed metadata production contract

The frozen composition is #1919 `c5d73d6c5a5` (reviewed source
`3f02db74335`), now integrated alongside actual #1873 and actual #1888
`c55a6a76044`. The constructor and observer contract above are available on
this reviewed stack, not yet claimed as landed. Consume native BingX #1884
`60f51b0c228` through its owner's agreed conflict disposition before source
conversion. Preserve the actual Store ticker-index implementation verbatim.

### One subject definition and a complete work key

Name the configured namespace `CredentialSubject::ConfiguredPrimary`, not
`AccountSubject`: credential decision C2 reserves the latter for a future
venue-resolved identity. The configured subject contains the immutable primary
presentation/signing references, optional passphrase reference, and stable_id
when present. No fetched UID or key hash is inferred. Absence of stable_id does
not disable joining on equal configured references. Credential rotation creates
a new engine; there is no mutable identity generation in this adapter.

The complete typed key separately retains normalized symbol, configured primary
subject, selected `CredentialSlot` and selected signing Identity references,
`RoutingIntent`, `Operation::Read`, metadata read kind, native instrument and
native-work/profile/bound identity, publication policy, and fallback policy.
Secondary reads therefore share the primary account namespace without joining
primary-slot work. Reuse the engine's slot and intent types. Credential C2's
future resolved-account selection gate remains outside this configured type.

Use full equality in one bounded Store metadata work index. A linear collection
bounded by the existing 256-attempt limit avoids adding Hash implementations or
encoding credential references into an execution tag. The index retains only a
full key, freshness token and shared ExecutionHandle, never numeric waiters,
observer lists, its own timers or terminal authority. The engine group key is
limited to 128 bytes. The group owner confirmed a closed `metadata-settings`
projection tag for both begin and join while full typed-key equality selects the
handle. The tag contains no account or credential data. Pin different full keys
with the same tag to distinct handles/Plans, and only call join after full-key
equality. Do not widen the limit or hash credentials.

### Entry, admission and supersession

Pure key/market inspection grants no refusal or publication right. A live
same-key handle may be checked with `validate_http_context` as an advisory
read-only check. In the same callback, admit the follower's genuine Read root
with the native 80-second bound and join through `join_execution`. That join
is authoritative. A join refusal fails that same root through `fail_context`;
never remint Preflight or start another execution after spending the entry loan.

A founder calls genuine ReadOnly `begin_preflight` before missing-key or
unknown-market refusal, then calls `begin_planned_execution` once. Do not call
separate `admit_plan`. Capacity/refusal must preserve the original command-owner
REDs' exact typed error, including no write outcome for Read admission failure.

Check Store space without mutation before admission. A full distinct-key index
uses a genuine `admit_http_command(Read)` root solely to carry a synchronous
`fail_context` refusal, with no Preflight, Plan, numeric terminal or retained
root/timer/index entry. The Read capacity error has Internal kind and outcome
None; compare its complete value to genuine admission capacity in the same
harness, rather than relying on Error::venue_error defaults. Engine admission
may itself refuse first. Prove index saturation separately in the core: at 256
entries a distinct key refuses, same-key lookup returns its handle, and pruning
a dead handle frees a slot. Native engine pools need not permit a 257th owner;
do not fabricate one to prove the index. Assert refusal-only pool counts return
to their prior values within the same callback. Only after Plan admission
succeeds create/bind its freshness token and work-index entry, before any native
request. A failure to bind retires that exact admitted Plan without I/O or a
stray index entry. Failed Plan admission cannot supersede an older read. A new
admitted different-key read for the same symbol, including return-only, does
supersede older publication. Retiring it cannot revive an older token. A read
issued during a pending write remains permanently nonpublishing even after the
write finishes. Distinguish current-token lookup from publication eligibility
so valid return-only work can still coalesce.

### Direct completion cutover and native handoff

Remove the metadata core's numeric waiter vector, Joined admission variant and
Completion::finish numeric fan-out in the same conversion. Store resolves a
validated proposal into publication status and a native value/error only.
Shared Plan sealing projects the uniform result once to its actual observers;
there is no second completion owner. CALLERS must state this uniform per-observer
terminal, independent detach/expiry behavior and removal of numeric fan-out. Adapt core publication-only tests, keeping
actual native joined-caller result/failure assertions and all original entry
RED test bodies unchanged.

At the #1884 handoff retain native STEPS/READ bounds, selected read slot,
request_signed_managed recipe, one signature re-sign, exact Plan Slot/Context,
finish_plan_call, early done_read_plan_after_data, and positions_revision fences.
Only the constructor/handoff and metadata eligibility/publication seams change.
Replace legacy Request::Metadata/token response and typed failure arms with the
incoming Pending::Metadata owner. Preserve supplied provider errors unchanged,
no authentication inference, and pending-write retirement on every native exit.
Move the existing typed-provider/auth controls onto the real managed owner when
their old manual Request fixture disappears; do not drop the coverage.

Validate exact Plan delivery before decode or Store effects. On validated final
publication, resolve the complete proposal and apply the infallible native
positions fence before Plan seal in the same callback. On stale DATA retain the
native current-settings fallback and discard the superseded margin before
parsing. On seal remove the exact token/key entry. Since expiry/orphan retirement
need not enter a venue callback, prune invalid handles before a subsequent entry;
never reuse a dead handle or leave bounded attempt storage permanently occupied.

Before readiness, execute full-key separation controls and a two-step real
Read Plan/retry with founder/follower detach and expiry. Founder detach between
first DATA and second issue must preserve work for a live peer; all-detach must
prevent that issue and reject a current-token reply before codec. Pin newer
return-only supersession, token change during re-sign, exact slot/Attempt
custody, stale-index retirement, native 20/40/80-second bounds and all three
original REDs turning GREEN. Retire the BingX inventory rehash through this
managed conversion only; no new exemption or STAGING edit is permitted.


### Native Plan handoff checkpoint

The native #1884 `d01191625e5` handoff removes legacy Request::Metadata and
numeric metadata_done, keeps the incoming Pending::Metadata/Plan adapter, and
retains both Store observations/write fences and native settings/positions
fences. The native settings revision/generation fields remain until the next
Store-token eligibility cutover. The BingX map is regenerated from the source
with the checker, taking every non-BingX map from the incoming manifest. No
checker/STAGING policy changes or inventory exemption are introduced.

The two own manual metadata signer fixtures are replaced by real managed-owner
controls in dispatch_failures: typed provider refusal retains the exact error,
auth health remains unchanged, Deactivate/AUTH_REJECTED are forbidden, and a
margin preparation refusal preserves newer Store settings. Existing native ACK
and position-fence controls remain. All 13 dispatch-failure tests pass and the
BingX all-target check passes; the scanner passes. The four original entry
blocks remain verbatim. This checkpoint supplies the native managed Plan path,
not the full-key execution coalescing/source conversion, which remains required.


### Pending-write handoff regression control

Independent source review found that the native Plan handoff passed publish=true
without consulting Store pending-write ownership. The tests-first checkpoint
5084ac1b133 reproduces publication of 12/34 isolated settings over the existing
20/20 cross settings while a leverage write is pending. Its final Store-equality
assertion fails on unchanged ecff production. A later fixture-only correction
clears positions/orders for the position-mode case so that native preflight
really issues that write; the original leverage assertion is unchanged.

FetchLeverage now captures !metadata_write_pending(symbol) at entry, restoring
the native sticky publication restriction. Account-wide None-scoped mode writes
therefore fence all symbols. The new regression covers leverage, margin and mode
writes with retirement before and after metadata DATA. It and the existing
per-rail fallback and typed provider-refusal/pending-retirement controls pass.
The native request declarations, signer recipe and supplied typed failures are
unchanged. This restores the documented nonpublication contract; full typed-key
execution coalescing and its remaining gates are still separate pending work.


### Remaining native checkpoint failures

Independent all-target verification of 29d1d29d984 reports 92 BingX tests
passing and five failing. Two are the original genuine command-owner capacity
REDs. The other three are the pending full joined conversion controls:
metadata_owner_coalesces_each_native_leg_and_completes_both_callers,
metadata_owner_fails_both_joined_callers_and_keeps_good_settings, and
metadata_pending_write_is_sticky_and_eligible_queries_coalesce. They fail
identically before the pending-write fix and must all pass before readiness;
none is ignored or treated as a supported exception. The original Prepare RED
now passes through the real native managed path. Current full scanner failures
are only newer KuCoin/Lighter inventory reductions against the moved landing;
actual integration remains required before final scanner/CI evidence.

## Current-landing conversion checkpoint

Continue from actual `c473342d483` with canonical reviewed #1919
`fff4d424898`. Integrate that reviewed group head before the landing so the
#1894 prepare/commit terminal funnel is preserved without a local core conflict
resolution. Shared docs keep incoming text first and each metadata block once;
#1919's capacity-outcome documentation correction is retained. No landing merge
or engine window is requested by this checkpoint.

The implementation removes Admission::Joined, numeric command lists and
Completion::finish from metadata. Store returns a publication Resolution only.
A complete WorkKey retains the configured primary subject, selected slot and
Identity, RoutingIntent, Read operation, symbol/native instrument, native
endpoint sequence and immutable call/retry geometry, publication and fallback
policy. The bounded index stores only WorkKey/Token/ExecutionHandle. It checks
full equality and token currency independently of publication eligibility;
only reads with the same publication policy can join. Return-only reads never
join publish-if-current work; writes/supersession cannot revive publication.
The adapter prunes nonvalidating handles before lookup, and retires their token
storage too. No key text, observer list or new completion table enters the group.

Founder entry spends one ReadOnly Preflight before native missing-key/market
refusal, then begin_planned_execution once. Followers spend one genuine 80-second
Read root and join only an exact live key; refusal fails that same root. The
constructor binds Store ownership only after admitted Plan/group custody exists.
The adapter carries that token through both native slots. Validate exact DATA
before codecs and resolve/publish through Store before the one Plan terminal;
current-settings fallback uses the existing early Read terminal. Retain native
STEPS/READ, selected slot, re-sign recipe, retry and positions fences verbatim.

Verification starts by rerunning the five named original failures against this
integration, then preserves their bodies while the conversion makes them pass.
Adapt own core tests from numeric waiters to publication-only outcomes; native
joined caller tests keep terminal/result assertions. Add full-key separation,
index saturation/prune, return-only supersession and two-slot retry/independent
observer controls. Typed admission-capacity equality replaces literal text.
All native/provider failure and Store pending-write controls stay required.

For #1928's separate caller boundary, plan managed-host controls using its real
Prepare/Bind/Claim adapter and deterministic deferred responses. The caller
wait must reject once at 120 seconds without cancelling a pending managed
resource; late completion cannot change that failed ingestion-window result.
A modeled serial ingestion chain releases on promise rejection and admits its
next wait while old engine work may remain. Test late success and late failure,
and the within-bound success control. These are SDK managed-host boundary
controls, not an executed NX deployment or Plan-history runtime receipt. Keep
NX abd35931b4's current TS path distinct from the future Rust binding. The real
private paged consumer still needs its final-slot physical-timeout/root-expiry
controls after #1928's exact runtime signatures are reviewed.

On exact513985923bb, the refreshed BingX account target executes78PASS and
five assertion failures: the same two entry-capacity and three coalescing
controls named above. No setup failure or ignored test is counted. Native-owner
plan review is CLEAR; same-policy-only joining is explicit.


### First executed direct-cutover checkpoint

On the reviewed current stack the native BingX account target now passes all
83 controls, including the five original assertion failures above with their
bodies unchanged. The Store API returns Resolution only; numeric waiter lists,
Joined admission and metadata Completion::finish are removed. Its 34 focused
metadata/publication/release controls pass after adapting own publication-only
tests. The bounded full-equality index controls use inert numeric payloads only
as pure lookup data, never fabricated execution Contexts or runtime authority.
Native key construction includes primary subject and selected identity, slot,
intent, operation, symbol/native, endpoint sequence, waits/concurrency, root and
per-call bounds, attempts/retry window, retry, publication and fallback policy.
The native Plan constants and dispatch recipe remain unchanged. Observer-specific
and caller-boundary controls, strict/feature checks and independent source review
remain readiness gates; no merge or final CI readiness is claimed.

The first native review identified a missing-token terminal gap. Original tests
at 66219b1ef4c retire the Store token while the managed Plan remains live; both
current-settings fallback and final-margin paths then produce zero terminals,
with a fresh native query completing as a control. The fix fails that exact
Plan through the supplied Internal lifecycle error on either missing resolution.
Account85 and dispatch-provider13 controls pass; the two entry-capacity and
three coalescing original assertions remain unchanged. The work descriptor's
static arrays are now named consts, and the scanner prunes only two account.rs
fingerprints (9 to7 occurrences), without reordering or an exemption. Native and
independent source delta reviews are CLEAR at 92c7616455f.

The new metadata_execution engine target executes five genuine managed Read
Plan/group controls: founder detach between slots with a live follower plus a
fresh-attempt retry; all-observer detach refusing the next queue before Prepare;
short follower expiry with the founder still publishing; different publication
policies sharing a closed tag but retaining distinct Plans; and all-observer
detach in flight suppressing the late codec, then pruning the dead handle/token
before fresh same-key admission. All five pass. These are shared-engine consumer
composition controls, distinct from the executed signed BingX native adapter
controls. The pure full-key/index checks and existing metadata release tests
remain required alongside strict/feature and family CI gates.


Strict Clippy for engine/BingX/WEEX with all targets/features passes. The
feature-off metadata target passes32 controls; the feature-on target previously
passed34, including the full-key/index checks. The now-unused own metadata
bootstrap exemption and its historical-only unit control are removed: both
checker files are byte-identical to actual c473342d483. Incoming staging and
all other bounds remain intact; the generated BingX account map is removal-only.

## Frozen #1919 reintegration and terminal write-fence cleanup

Integrate canonical `2f4a08b449cb69316cf9d657f7c12f9e696c50a0` by plain merge.
Keep its Plan/Execution/FounderObserver source unchanged, both metadata/identity
BingX arms, and incoming WEEX Plan trading with the existing Store write fences.
A Store start refusal fails that exact Plan before send; a response clears its
fence only after exact Plan delivery validation. Remove the orphaned BingX
legacy metadata request helper instead of retaining an unused alternate path.

Native-owner disposition (codex-uufub4): every validated terminal also retires
its Store metadata write owner. Add one idempotent metadata_write_finished call
in the common finish_sealed_command after exact seal/registry retirement;
Seal::Stale must return before it. No new emitter, timer, completion API or
Plan policy. This closes expiry/no-callback latching while preventing an old
Plan's stale seal from clearing a reused command's pending write.

Before that hook, add compiling Engine controls for explicit Plan failure,
authenticated local success, and Plan expiry with same-id generation reuse and
stale Plan/timer rejection. These must establish actual pending Store state,
then require it to retire with exactly one terminal. The stale replay must
leave the new generation pending until its own terminal. Fresh CI and bounded
owner review of the exact common-funnel hunk remain required.

Original merged-source terminal controls execute 3 assertion RED / 0 ignored:
explicit Plan refusal, authenticated local success and Plan expiry each emit
one terminal but leave metadata_write_pending true. No compile/setup failure is
counted (the fixture uses the current StepBound/CallBound schema). Same-id
reuse/stale-seal checks follow the expiry assertion and are not claimed executed
until the fix makes that first assertion pass. Logs stay outside Git.

The one-line common-funnel cleanup turns all three controls GREEN, including
execution of expiry -> same-id reuse -> stale Plan and timer rejection -> new
expiry. The exact hook follows successful request_contexts.finish and
untrack_read_root; Seal::Stale returns before Store cleanup. This source result
is separate from fresh CI and owner source review.

The bounded common-funnel review found that metadata_write_finished eagerly
materializes metadata state even for an unrelated command. A genuine Read entry
and done_context control first proves one successful terminal with Store metadata
absent before entry, then fails the final metadata-is-None assertion on unchanged
cleanup (one assertion RED, zero ignored). Its initial fixture compile typo was
corrected before this evidence run. Make cleanup a no-op when metadata is absent;
pending-write retirement and generation-isolation controls remain mandatory.

Current landing cf05cf13663 is integrated by plain merge. Shared documentation
keeps incoming sections and the own readiness/metadata sections; the existing
Read-capacity outcome wording correction is retained. A local merge commit
accidentally included unresolved doc markers; the next commit removes them without
rewriting history. No source conflict or local core resolution was introduced.
The as_mut-only cleanup makes the untouched-Store Read terminal control pass
(one GREEN). It changes no existing pending-write invalidation behavior; the
three genuine Plan terminal controls remain the regression gate for that path.

Bounded source/preservation review of `6f05a0403ec` is CLEAR (claude-4yjlw0):
canonical #1919 custody retained, automatic source integration, exact nine-line
BingX removal, checker/STAGING incoming-exact, no conflict markers. Its two Low
notes are addressed by integrating actual `f8163b7b852` (including Lighter `8f22` and
shared command entry #2004) and a native late-failure preservation control.

The WEEX control completes one settings write, reuses the command ID for another
real managed write, then sends HttpFailed for the retired request. No terminal
or fence change occurs; failing the current request emits one failure and clears
its own fence. It passes as a GREEN preservation control. Engine failure routing
consumes Pending by exact request ID before the venue callback, so a late failure
cannot enter trade_failed for a reused command. A nearby comment records that
rule; no native reducer or failure policy changes.

## 2026-10-25: bounded write-fence index size reduction

The actual `92de1326994` composition at `1b5ea288600` fails the approved
Bitget allowance in Rust run `40110890188`: both Bitget variants measure
1,364,068 bytes against 1,363,968 (100 bytes over). The official reports are
`strict-replay-bitget` and `strict-replay-bitgetusdc` artifacts. Bitget source
is incoming-exact; no allowance increase is part of this change.

The private metadata write-reservation index uses a Vec instead of a HashMap.
It keeps the same 256-owner cap and duplicate refusal before invalidation or
insertion. Retirement finds only the exact command, removes it with swap_remove,
and performs the same symbol/global invalidation. Pending checks remain an
any-match over symbol scopes. No iteration order, public API, generation,
terminal, Store publication or financial policy changes. Scans are bounded by
256 reservations; the index no longer links its identity-key HashMap machinery
through the common terminal cleanup.

A new preservation control first passed on the original HashMap implementation.
It pins duplicate and capacity refusal without revision changes, surviving peer
fences, slot recovery, and inert repeated retirement. After the Vec change,
55 filtered metadata library controls and 180 scoped engine/BingX/WEEX
integration tests pass, zero ignored, with four jobs and serial builds. The
exact source patch is subject to bounded core review and strict scoped checks.
These are native correctness receipts; WASM savings remain unmeasured until the
new freeze candidate has its single PR Rust run. No local WASM sweep is used.
