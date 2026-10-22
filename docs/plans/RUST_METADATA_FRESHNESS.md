# Engine-owned metadata reads and settings freshness

Status: shared core and WEEX/BingX adapters implemented; draft pending #1741
physical-release integration and final CI/proof. No universal fan-out or runtime
deployment closure claimed.

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
