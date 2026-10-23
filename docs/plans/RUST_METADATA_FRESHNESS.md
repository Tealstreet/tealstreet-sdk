# Engine-owned metadata reads and settings freshness

Status: engine/Store core and legacy WEEX/BingX adapters implemented; common
Epoch/Stamp adopted after actual #1741/#1759 integration. Shared scheduler and
unsigned managed-read release controls pass. Private production adoption remains
blocked on real family signers and shared observer-group completion/expiry.
Final CI/proof, universal fan-out and runtime deployment remain open.

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

### Open composition decision

The proposed shared API exists locally at `779b8891ecf` on the reviewed Plan
pin. Its `begin_planned_execution` returns `(Plan, ExecutionHandle)`; Read
controls and independent source review are still being prepared by its owner.
Do not consume it as a frozen reviewed seam yet.

That version keeps the founder command as the physical Plan owner. The founder
cannot detach and survives until Plan sealing; it therefore cannot represent
the standing metadata requirement that every observer, including the first,
can detach or expire independently while a live peer remains eligible. Nor can
it exercise a truly orphaned read's current-stamp denial before the codec.
The group owner confirmed this difference. An execution-owned retained Plan
with an explicit founder observer would be a shared additive change, not a
venue adapter. Keep the original matrix as a gate until the owner settles this
policy or supplies a reviewed shared authority that implements it. Do not
weaken the controls or create local orphan bookkeeping to manufacture a pass.
The group owner has agreed the revised design with the Plan owner: move physical
Plan ownership to `Owner::Execution` and expose the founder as an explicit
Observer. Its implementation and original Read controls are in that lane; a
new reviewed frozen source pin is still required. Do not consume `779b`.

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
Wait for the composition policy and reviewed Read API before group production
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
