# Financial snapshot freshness and atomic native publication

Owner: codex-mxb4mq. Base: actual Aster #1756
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6`.
Status: committed plan/API proposal before tests or implementation. Owner priority
is this shared core with Binance and Bitunix references, before remaining
cumulative fan-out and reporting. #1737 has the next landing slot; integrate its
actual shared inputs before final proof. RequestContext and metadata owners
review the API before implementation; their unmerged work is not claimed here.

## Problem and boundary

An order snapshot that started before an accepted ACK must not restore older
Store rows or native known-filled, terminal, bootstrap or ownership state.
Checking freshness only inside `Store::set_orders` is too late if native caches
were already changed. Capture one token per logical read; validate before both
native effects and Store publication. Native financial/identity validation and
post-ACK lagging-server semantic fences still apply.

This is engine standardization with two distinct protocol consumers, not a new
per-venue revision helper. Existing ordinary native ACK paths already have
fences; do not call compatibility tests original-RED production defects.
RequestContext's discovered late-ACK gap motivates this contract, but #1741's
initial late codec stays read-only and retires Unknown. Safe late patch adoption
is a separately reviewed follow-on using this API.

Bitunix intentionally permits healthy snapshot peers through newer WS rows
using engine RowAdmission. Its write crossings instead reject whole reads,
including partial ACK parsing and cleanup. A token tied to every Store mutation
would reintroduce starvation and break initial regular/TP-SL readiness. Keep
broad Store mutation revision (unmerged #1741 late-response protection) distinct
from this explicit snapshot-write fence. Snapshot publication itself must not
invalidate sibling endpoint tokens or metadata observations.

## Common engine primitive and proposed API

Extend existing `engine/src/fence.rs`; do not create another venue epoch copy.
Existing StaleGuard progress policy remains unchanged and never authorizes a
snapshot that crosses this write fence. Add crate-private issuer-bound types:

```rust
struct Epoch; // Default; not Clone. Checked counter with permanent exhaustion.
struct Stamp; // Clone, opaque; equality/hash include issuer identity.
impl Epoch {
    fn stamp(&mut self) -> Option<Stamp>;
    fn advance(&mut self);
    fn current(&self, stamp: &Stamp) -> bool;
    fn compare(&self, left: &Stamp, right: &Stamp) -> Option<Ordering>;
}
```

The identity survives moves and rejects another Store/account, including equal
numeric counters. Stamp fields/constructors are private; no clock, credentials,
wire IDs or serialization. Prefer lazy stable issuer allocation: unused private
fences add no public-only allocation; advancing an existing fence only bumps
its scalar, without minting/dropping a stamp on every event. Equality/hash must
use issuer identity, not equal unit payloads. Compare rejects foreign, future
or exhausted stamps. Overflow never wraps, resets or admits saturated equality.

Metadata #1747 can reuse Epoch/Stamp for its serial/global/per-symbol arithmetic;
its current-read and observation ordering, coalescing, validated setting/rail
proposals, fallback and sticky pending-write exclusion remain metadata-owned.
Its owner agreed not to extract a second primitive. Metadata adoption is not
silently bundled into these two financial references.

Store owns the financial fence. Export only an opaque non-Clone/non-Copy
`OrderSnapshotToken`, not the epoch implementation, to venues:

```rust
impl Store {
    fn begin_orders_snapshot(&mut self) -> Option<OrderSnapshotToken>;
    fn orders_snapshot_current(&self, token: &OrderSnapshotToken) -> bool;
    fn invalidate_order_snapshots(&mut self);
    fn commit_orders_snapshot<I, F>(
        &mut self, tokens: I, orders: Vec<Order>, native: F,
    ) -> bool
    where I: IntoIterator<Item = OrderSnapshotToken>, F: FnOnce();
}
```

Methods used by venues are public; representation stays private. Token ownership
makes final consumption single-use without an active-read registry. Capture once
and move through pages/join state. Final commit rejects an empty token inventory,
foreign/exhausted/stale tokens before invoking the native closure or changing
rows, loaded flags or dirty state. All fallible decoding/financial checks finish
first. The closure is infallible, synchronous native-cache application and cannot
reborrow the same Store. Publish only after it returns. No transport, async work,
command callback or outbound action belongs inside this closure. This is one
synchronous admission boundary, not a panic-recovery transaction.

`begin` exhaustion must issue no read, strand no inflight/readiness state and
settle command-owned preparation through existing typed failure handling.
Accepted native writes still apply if the fence is exhausted; future snapshots
remain inadmissible. Pin these lifecycle choices with controls before adapters.

## Plan audit: auth context and staged compatibility

The Bitunix authentication observer calls read-current admission before account
mutation, but the existing Venue observer has no Store argument. Owner and
RequestContext owner approved one shared extension: pass read-only `&Store` into
`Venue::account_auth_outcome` and its existing `observe_account_auth` forwarding.
Update all four real overrides and engine/native test fixtures; only Bitunix
uses the context. Preserve managed admission-before-auth ordering. No live
mutable token state, extra observer or venue revision counter is introduced.
These narrow engine auth regions are clear with RequestContext's owner.

Binance retention of rejected rows currently reads native terminal state AFTER
new binding proposals. Candidate rows must use the equivalent prospective view;
only the admitted closure applies native caches. Bitunix's initial regular then
TP-SL commits are sequential: validate both tokens before either publication,
then preserve the second half's view of the first Store/native result. Combining
the two requires an explicitly equivalent staged sequential result.

An old stamp retains its issuer allocation after Epoch destruction, so allocation
address reuse cannot authenticate a new issuer. Exhaustion controls include
background reads, staged initial halves and command-owned preparation, with no
new immediate retry loop or stranded readiness/timer state. Mechanical adoption
is not an original-RED production fix. Baseline focused engine/Binance/Bitunix
native suite passed before source changes.

## Two reference consumers and compatibility

1. **Binance (all existing variants).** Replace AccountState.order_version
   capture/comparison and invalidation arithmetic with the Store-owned token.
   One token covers the existing regular/algo join. Preserve existing conservative
   stream invalidation trigger sites, native filled/terminal/incarnation/client
   ownership checks and refresh scheduling. Stage delivery bootstrap floors,
   known-filled and native binding effects before final admission. Thread Cx/Store
   through private invalidators that previously lacked it, including accepted
   native-only/absent-row ACKs; never mutate first and hope a setter fences later.
2. **Bitunix.** Replace only order_write_revision capture/comparison/increments;
   leave balance/position and native endpoint generation/serial policies intact.
   Preserve response-before-parse, optimistic dispatch, cleanup and rollback
   fences, even when partial/malformed siblings end a command. WS order_newer /
   order_rejected RowAdmission continues healthy-peer publication. Stage native
   metadata and cumulative baselines before final admission. Recheck both initial
   regular/TP-SL tokens before either half publishes; snapshot commits do not
   advance their shared write fence. Keep terminal, optimistic and pending-fill
   retention semantics and all native quantities/quotes unchanged.

## Committable phases and verification

1. Commit/review this plan and concrete API with correctness/adversarial reviewers
   plus RequestContext/metadata owners. Inventory every native invalidation and
   commit call site before editing it. Run baseline focused native controls.
2. Implement common epoch and Store admission first. Tests cover issuer identity
   across moves, foreign stamps/tokens, single-use ownership, empty/mixed token
   inventories, permanent exhaustion and overflow; stale rejection leaves native
   cache sentinels, Store rows/readiness/dirty state untouched. Fresh validated
   empty snapshots still clear eligible rows. Existing StaleGuard tests remain.
3. Adopt Binance, then Bitunix with independent phase reviews. Existing native
   tests must pass on original source and after adoption. Add real event controls
   for read -> ACK -> old response; native-only/absent cancellation; partial ACK
   with malformed sibling; multi-leg/page joins, endpoint siblings and initial
   pair; WS healthy-peer progress; post-ACK lagging snapshots and reincarnations;
   failed admission cannot rewind native fill/terminal/binding state. Prove any
   newly found behavioral fix original-RED before extending its scope.
4. Update nearby audits/quality/caller evidence only for actual effects. No FILLS
   shape/ID change, synthetic fill, signer/clock/request transport, retry/outcome,
   history, metadata policy or NX API migration. Every later financial consumer
   uses this engine path; no all-venue completion claim in this first core PR.

Run engine/Binance/Bitunix focused tests with CARGO_BUILD_JOBS=4, strict touched
Clippy/fmt/bans, root typecheck/lint and independent integration reviews.
CI owns release WASM, sizes and timing. Full authenticated 32-suite/110-workload
baseline/candidate proof keeps all 976 raw outputs, full IDs/requests/receipts/
checkpoints and ordered gates/fixers, excluding only top-level metrics. Standing
TS-reference classes require both complete pre-change controls and independent
review; strict unequal evidence stays unequal. No local release/replay sweep,
new declarations, normalizer or threshold relaxation.

Integrate actual shared predecessors, wait for full current-head green CI and
complete evidence reviews, run a fresh exact-scope revert/trial guard and merge
as separate calls. Audit actual parents/tree/owned and all incoming files, report
the actual pin to the overseer and affected owners. No live captures or orders.
