# Financial snapshot freshness and atomic native publication

Owner: codex-mxb4mq. Base: actual Aster #1756
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6`.
Status: plan/API committed before implementation; shared core and both native
references implemented, final CI/raw proof pending. Owner priority
is this shared core with Binance and Bitunix references, before remaining
cumulative fan-out and reporting. Actual #1737 is integrated through the owner-
approved exact frozen #1757 stack; #1757 must actually land before this PR.
Any predecessor head change requires real integration and fresh CI/proof.
RequestContext and metadata owners
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

`stamp()` observes without advancing; only explicit invalidation advances the
domain. Metadata serial issue ordering uses advance then stamp on one common
metadata clock issuer; its financial sibling has a separate issuer.

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
        &mut self, tokens: I, publish: F,
    ) -> bool
    where I: IntoIterator<Item = OrderSnapshotToken>, F: FnOnce(&[Order]) -> Vec<Order>;
}
```

Methods used by venues are public; representation stays private. Token ownership
makes final consumption single-use without an active-read registry. Capture once
and move through pages/join state. Final commit rejects empty inputs. Admission also rejects another issuer,
expired revisions and exhausted domains before invoking the native closure or
changing rows, loaded flags or dirty state. All fallible decoding/financial checks finish
first. The closure is infallible, synchronous native-cache application and final
owned-row construction. It receives the current rows read-only and cannot
reborrow the same Store. Publish the returned rows only after it returns. No transport, async work,
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
new binding proposals. The admitted closure applies
staged native bindings before deriving retained rows from its read-only current
row slice, preserving that view without duplicate shadow-binding policy or
cloning native maps. Independent correctness/adversarial API review precedes
consumer source edits. Bitunix's initial regular then
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

## Implementation and local evidence

Plan checkpoints `1a72d442724`, `72ab6fd5bdc` and `772d9407f89` preceded their
respective source/API changes. The common boundary is in `fence.rs` and Store;
Binance replaces ten native revision invalidations, Bitunix six. The auth hook
now receives read-only Store context at all three existing observation sites,
before account callbacks. Other auth implementations only accept an unused arg.
Bitunix initial publication consumes both endpoint tokens in one callback while
preserving regular then TP-SL row/native views.

Original focused engine/Binance/Bitunix baseline: 876 tests green. Shared core,
auth and touched override/native-host suites: 1,226 green; strict Clippy passed.
Binance's 266 compatibility tests remain green plus one new permanent-exhaustion
control. Bitunix's 230 tests pass, including initial halves in either arrival
order, native fill-baseline nonconsumption, retired stall/debounce ownership and
two preflight commands each failing once before any dependent HTTP write.
Permanent capture failure retires both endpoint stall owners and the shared
order debounce. This corrects a lifecycle issue found during review of the new
exhaustion API; it is not represented as an original production bug.

Independent correctness and adversarial phase reviews are clean after the
native-callback positive sentinel and exhaustion ownership controls were added.
Architecture gate: YES, a cross-cutting concern handled in one engine primitive
and Store admission entry, with two distinct native consumers. Metadata adoption,
remaining venue snapshot fan-out and RequestContext safe late patches remain
follow-ons. Final authenticated CI evidence and actual merge receipt are pending.

## Final predecessor integration checkpoint

Owner authorized exact-head stacking of reviewed #1757 after actual #1737.
Porcelain merge `4fa76dd09e01` incorporates frozen #1757
`74724d0e36b3206c37c9bca2acdec98edfe7810a`, which contains actual #1737
`4c4df7c99dc52a5192f5c5ff24c64dd4653edda1`. Independent correctness and
adversarial reviews verify the full 5,271-path tree, all incoming files, 19
unchanged owned files and both patch sets in five shared files. Exactly 24 owned
paths remain against that frozen predecessor, including the owner-approved
historical public-prose secret-scan fingerprint; no rule/path bypass exists.
Integrated engine/Binance/Bitunix: 899 native tests pass; touched strict Clippy,
fmt/bans and root typecheck/lint pass (44 tasks, 31 cached; 28 tasks, 26 cached).
These source checks do not certify an unmerged predecessor or final replay proof.

Earlier diagnostic head `52b52375165` had full green CI. Its strict 976-output
comparison retained all 488 Rust outputs unchanged, three documented TS-reference
classes (ApeX/TxFlow multiplicity and WOO checkpoint-zero books), and an identical
MEXC failure multiset with reversed list order. No comparison was normalized.
For this PR's fresh final proof the owner explicitly accepts that MEXC ordering
condition only when both raw TS and Rust canonical hashes are unchanged and the
failure strings form an identical multiset; retain the strict unequal report.
The four standing TS classes still require both complete pre-change variants
independently authenticated. Anything else blocks. A separate small harness PR
will sort failure lists deterministically AFTER this core lands. Fresh integrated
head CI/proof, actual #1757 verification and the separate final guard/merge remain
pending.

## Next pair: WOO and Crypto.com (plan before implementation)

Base: actual Bitrue history `726948e78f7db7ab9fc3e0fc19e107602eb110f0`.
Only Binance and Bitunix use the shared token on this landing;25 other venue
crates remain. Backpack/Paradex are already covered by a separate queued draft,
so this pair is disjoint and does not claim their draft as merged. Exclude
BingX,WEEX,OKX,Bitget,Bybit and queued AsterDEX/MEXC/Phemex/Polymarket reporter
files. BloFin was inspected but not selected: its complete startup census seeds
execution prefixes even when WS revisions reject row replacement, and emits
queued executions. Preserve that policy for a separately designed adoption.

Architecture gate: YES, this is a consumer fan-out of a cross-cutting concern.
Use existing Store begin/commit/invalidate only; add no engine/venue epoch or
counter copy. Keep both families' existing per-ID observation revisions because
these also order WS peers/settings and prevent server-lag regressions; those are
not replaced by the selective write fence. Token capture observes without
advancing and sibling snapshot publication must not invalidate it.

WOO: add one owned token to OrdersJoin before generation/inflight construction,
move it through the existing regular/algo page join, and validate before native
algo classifications, initial cumulative seeding and Store rows are published.
The infallible commit callback reads current rows and keeps rejected identities,
newer per-ID WS rows, terminal status and monotonic filled totals exactly as now.
Keep generation routing, page metadata/budgets and healthy-peer WS progress.
Accepted native order-write ACKs invalidate even absent Store rows, after native
identity/result validation and before scheduling refresh. Leverage/mode ACKs do
not invalidate order tokens. Do not invalidate on generic account_upsert/WS.
After a discarded read, clear join ownership and let native poll cadence retry;
no extra hot-loop or command outcome change. Capture exhaustion issues neither
order endpoint, creates no join and leaves ordinary polling/positions available.

Crypto.com: add Account.order_token ownership captured once when Balance starts
normal Orders, retaining it across regular->advanced join and same-read retries.
Failed/invalid reads clear both order_rows and token. Exhaustion schedules the
ordinary reconciliation timer and issues no orders HTTP request. At completion,
take both lists/token and commit all advanced membership/pruning and Store rows
inside the shared callback, preserving family-specific rejection retention,
per-ID WS versions, timestamps and terminal filtering. Calls to schedule/retry,
notices and transport remain outside. Accepted Place/Amend/Cancel/CancelAll
results invalidate once their existing native response contract succeeds,
including empty Store and partial successful batches. LeverageAccount/Leverage
and malformed/refused mutations do not gain acceptance semantics. Do not infer
new native IDs or optimistic rows beyond the existing trading contract.

Scope: two account modules, two narrow trading ACK hooks, existing account tests
(and trading tests only if needed), plus existing freshness/AUDITS/QUALITY/CALLERS
docs. No sign/request/header/identity/socket/history/metadata/build changes.
Exact legacy maps should stay equal because tokens live in account/join state,
not request-renderer syntax; if an unavoidable same-site re-expression changes a
hash, use the standing literal full-map rebind pattern with old/new/count and
independent full-map negative controls. No new HTTP site or exemption.

Verification: plan commit and independent correctness/adversarial review first;
original focused engine/WOO/Crypto.com/Binance baseline; native accepted ACK vs
older joined response controls on original code before claiming any bug; preserve
ordinary WS healthy-peer/terminal/empty/failed-page controls. Add explicit token
invalidation/absent-row and capture/late exhaustion tests (API-adoption REDs are
not original production bugs). Bound native cache evidence using later amend
routing or cumulative-fill suffix controls. Then focused native suites, strict
all-target Clippy/scoped fmt/bans/root typecheck+lint, both implementation/tests/
caller reviews. Draft once clean. CI/proof watches for1784–1787 remain running;
all merges stay after ApeX1772 and1783 in owner queue order. Future shared actual
integration/current full CI/proof/separate guard/separate merge remain required.
No local release WASM/replay/timing/size sweep or comparator/budget relaxation.

Both independent plan audits are CLEAN with these explicit continuation rules:
Crypto.com clears token/regular half only on terminal abandonment; bounded40101
and transport retries retain the exact original token plus regular builder,
including when an ACK crossed the failed attempt. WOO metadata refresh and
stop_awaiting_price consumption/immediate reread run only after accepted commit;
a rejected token waits ordinary poll cadence. Preserve mapping-time diagnostic
forget/report timing and the existing stop-price flag (no reporter repair).
BloFin's bootstrap seeding is a separate design item; no BloFin source changes.

### WOO/Crypto.com local implementation evidence

The original1140-test/58-group focused baseline passes. Twelve added native tests
then produce10 failing original controls and2 WS-peer compatibility passes on
exact726948 sources. Only the two native accepted-cancel-ACK crossings are called
original correctness gaps; explicit Store invalidation/exhaustion and cache/
startup-seed controls assert the new engine contract. The missing expected amend
route is a route-lookup failure, not mislabeled as a literal assertion RED.
Migrated controls pass all12, including ACK before auth/transport retry and WOO
initial discard followed by either direct0.006 execution or fresh0.004 prefix
then0.006 report delivering0.002. Same-ID WS and healthy peers progress without
invalidating sibling tokens.

Current original receipt SHA
`8247a288238253e42238e4ffc1a39d95e0fc5e0ba8c5a13ce96c6d5d404fd6b3`
binds four original source modules, both final test files, both complete run logs
and exact migrated-source restoration. Earlier original receipts stay historical.
The full legacy lexical map remains153 paths/1232sites, byte-identical to the
landing manifest; full-map SHA
`9a098b9b5f0a8961c66836a57cc848b92d92d5036216cef94b350c5625821a36`.
No inventory, checker, harness, engine, signing, build or request recipe changes.
Both independent plan/source/test/caller/architecture reviews are CLEAN. Final
focused native/Clippy/fmt/bans/root checks and draft CI receipts are recorded in
the PR body; no historical replay proof or disposition qualifies a later head.
