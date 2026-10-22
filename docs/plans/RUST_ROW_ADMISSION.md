# Engine row admission and snapshot isolation

Owner: codex-mxb4mq. Base: actual landing `5569062869971dbfce492e3259bbcd5ebded8869`.
Status: phase 1 merged; phase 2 plan before implementation. Owner-directed
engine standardization, not a new per-venue copy. Sequencing update: the owner
narrowed the Signer window to dispatch/request/signing and host files, allowing
the disjoint row core to merge first; the Signer lane integrates its actual pin.
Finish all keyed row-admission migrations before the separately assigned
financial-snapshot commit/freshness-token core.

## Contract

One engine admission/coverage path owns keyed private REST and WS rejection.
A rejected recognizable native identity holds its **current** stored row and
associated lineage; an unidentified row holds absence for the affected scope.
Healthy peers still commit. An authoritative, structurally complete empty list
still removes ghosts. Accepted explicit zero/terminal rows retain their existing
meaning. The admission boundary emits exactly one coded `row.skipped` event per
reported rejection; merging coverage or retaining a stored row emits none.

Venue code retains wire decoding, exact native identity and account/product
scope, structural envelope/page/duplicate failures, financial validation,
mode coherence, side maps and existing timestamp/generation/terminal fences.
No stale clone may overwrite a newer fill/cancel/reopen. A rejection never
creates a fill or permits a write retry. Unknown identity holds absence but
must not globally reject known healthy peers; overflow additionally holds
same-key conflict decisions conservatively once exact keys cannot be tracked.

## Inventory and phases

Current engine `coverage.rs` already provides `SnapshotCoverage` and
`SnapshotBuilder`, but venues still pair rejection with their own notices and
Bybit/Bitunix duplicate bounded key sets, merge and conflict semantics. Other
families already use coverage/builders or their own scoped collectors. The
existing `RUST_PRIVATE_ROWS_TICKERS.md` is historical evidence, not a current
completion certificate.

1. **Shared core and duplicate collectors.** Extend `engine/src/coverage.rs`
   into `RowAdmission<K>` while retaining `SnapshotCoverage<K>` as a compatible
   alias for existing consumers. Centralize bounded rejection keys, unknown and
   overflow distinction, merge/borrowed-key lookup and coded reporting. Add
   `admit(Result<T, Option<K>>, ...) -> Option<T>` and builder admission so both
   REST snapshots and incremental WS mapping use the same rejection path.
   Keep pure `reject` available for already-reported coverage propagation and
   freshness bookkeeping; it must not emit another notice. Replace Bybit's
   `RejectedRows` and Bitunix's order/position collectors with this engine type,
   preserving every current join/freshness/side-map decision and notice message,
   level, order and count. Route their reported row rejections through the
   engine reporter. Structural frame failures retain their existing semantics.
2. **Existing coverage consumers.** Fresh branch after the core: migrate
   Bitget and Orderly keyed REST/WS row admission and notices onto the core.
   Preserve batch/coherence grouping where one native rejection protects several
   keys, and do not introduce a second diagnostic for propagated coverage.
3. **Remaining venue coverage.** Inventory every actual keyed REST/WS path
   against current source, then migrate bounded batches to the engine path.
   Existing coverage/builders are the compatibility boundary during migration;
   do not claim all venues migrated until the call-site inventory proves it.
   Report any new behavior defect/cross-cutting concern before broadening scope.

Each phase gets a fresh branch/PR into `feat/rust-rewrite`, independent plan,
implementation and adversarial review, and an explicit architecture-gate answer.
The answer for phase 1 is engine standardization with two consumers, not a
per-venue implementation of a shared concern. Family signer migrations remain
separate and begin only after the shared Signer actual API/pin.

## Phase 1 files and API boundaries

- `sdk-rs/engine/src/coverage.rs`: shared type/compatibility alias and tests;
  `RowAdmission::reject`, `extend`, `holds`, `conflicts`, `any`,
  `unidentified`, `overflowed`, `allows_absence`, `rejected`, `skip`/`admit`;
  builder delegates to the same admission path. Borrowed lookups support
  `String` keys with `&str` without allocation. All collectors cap at 4096 keys;
  overflow survives page/WS merge and no protected key is forgotten.
  Merge adds each source rejection count once (including repeated/unknown rows),
  without counting key reinsertion or emitting diagnostics. Snapshot builder
  accepted-row precedence and venue conflict filtering remain distinct.
- `sdk-rs/venues/bybit/src/account.rs`: remove the local collector; preserve
  parent/child identity, product filtering, Hyro strict handling and position
  mode coherence; use accessors instead of private state fields.
- `sdk-rs/venues/bitunix/src/account.rs`: remove both local collectors; preserve
  ordinary/TPSL native families, read progress, accepted/newer distinctions,
  full-snapshot absence and symbol settings/financial-state isolation.
- Focused tests in coverage and the two venue test modules where a real boundary
  needs a regression. Existing mixed/malformed/unknown/continuous-WS regressions
  remain intact. No signing, SDK clock, host transport, tape, comparator,
  manifest, fixture or workflow edits planned.
- This plan/index and nearby `RUST_QUALITY.md`/`RUST_AUDITS.md` only for accurate
  current implementation/evidence; do not mark later migrations complete.

## Verification and merge evidence

Core tests cover known/unidentified/overflow rejection, bounded merge and
borrowed lookup; healthy peers, valid empty lists and the **current** stored
copy at commit; accept/reject and grouped coverage with exactly one event,
correct health count even with host log filtering, and no event during merge.
Test via the real engine event path, not an implementation-mirroring mock.

Run focused engine + Bybit + Bitunix + Binance native tests with
`CARGO_BUILD_JOBS=4`, touched-crate strict Clippy and fmt, root typecheck/lint,
and existing bans. Reproduce any behavior correction RED on original source
before fixing; a mechanical shared-helper migration is proved by existing
behavioral tests plus replay equality, not a fabricated failing test.

CI owns full release WASM, all normal strict replays, size and timing gates.
Compare complete Rust raw artifacts from source-equivalent baseline and final
head for affected Bybit aliases/Bitunix and Binance, excluding only recorded
performance metrics. Keep full IDs/requests/receipts/checkpoints and exact known
keys; no declarations, normalizers or thresholds are relaxed. If a difference
is intentional, obtain reviewed precise evidence before declaring it.

Integrate actual Signer core once before final proof. Full current-head CI is
mandatory. Fresh exact-scope revert/trial guard and merge run as separate tool
calls, followed by actual parents/tree/owned and incoming blob audit. Report
each actual merge pin to claude-ddi2il and affected core owners. No live capture,
production credential, local release build or load/timing sweep.

Independent plan review: CLEAN before implementation; architecture answer **No**.
Review explicitly checked unknown versus overflow, silent freshness propagation,
accepted-row precedence and deferred/deduplicated Bybit unsupported-row Errors.

## Phase 1 local implementation evidence

Plan committed as `af161605e8c` before source changes. `RowAdmission` replaces
all three Bybit/Bitunix collectors; original diagnostic messages, levels and
report ordering are retained. Deferred unsupported-order reports use the
shared reporter without adding coverage. Freshness propagation remains silent.
The existing `allows_absence(&K)` signature is retained for compatibility;
new borrowed `holds`/`conflicts` support native `String`/`str` lookups.

1,045 engine/Bybit/Bitunix/Binance native tests pass, including all existing
mixed-row/continuous-WS/freshness regressions. Ten focused coverage tests pass;
three deliberate mutations fail assertions for unknown-row peer starvation,
incorrect merged rejection counts and duplicate diagnostics, then restored
source passes. Strict touched-crate Clippy, fmt, bans, 44 root typecheck and
28 lint tasks pass. Independent implementation/adversarial review is CLEAN;
architecture answer **No** (engine choke point plus two consumers).

Final Signer integration, current-head full CI and complete raw replay
comparison remain merge requirements; this local receipt does not claim them.

## Phase 1 merged receipt

Actual #1708 `b2f074bf26374bc6c4b37c5d03fa1126483390a6`, green head
`0dcf53f9709824e4e4ba07087c066dbaf65fbe9e`, Rust CI `39708007324`.
All 43 applicable checks succeeded; browser PR job was intentionally skipped.
All 222 complete raw outputs (111 Rust, 21 exact workloads, 62 implementation/
mode groups) equal source-equivalent baseline #1707 CI `39705386690`, excluding
only top-level metrics. Ordered diagnostics, full identities/requests/receipts/
checkpoints and 9 PASS/12 known FAIL statuses are unchanged. Independent final
evidence review CLEAN; separate fresh exact-six-file guard preceded separate
merge; actual parents/tree/owned/incoming blobs matched. Original pre-merge
Signer-wait language above is retained historical planning, superseded by the
explicit sequencing decision at the top and in #1708's body.

## Phase 2: Bitget / Orderly existing keyed admission

Base: actual phase-1 merge `b2f074bf2637`. Fresh branch
`feat/rust-row-admission-bitget-orderly`; commit/review this plan before source.

This first fan-out PR migrates the existing Bitget REST/WS and Orderly REST
coverage/reporting sites without changing decoded economics, retained keys,
notice text/level/order/count, foreign-product filtering or freshness policy.
Use engine `RowAdmission`/`SnapshotBuilder::skip` for a single rejected identity;
use silent `reject` plus the shared `row_skipped` reporter for already-recorded
or grouped decisions. Bitget mixed-mode rejection protects multiple symbols
but reports once, exactly as today. Its all-unmappable scope hold, observation
barriers and retained native plan/trailing routes remain unchanged. Orderly
mode/position/normal+algo paging retains duplicate/root/page/completeness
checks, per-row PnL bounds, flat-row treatment and source timestamps.

Files: `venues/bitget/src/account.rs`, `venues/orderly/src/account.rs`, this
plan and nearby `RUST_QUALITY.md`. No engine/host/SDK/clock/signer, wire, fixture,
manifest, normalizer, workflow or caller contract changes. Mapping remains
venue-specific; coverage and coded rejection policy must not be copied locally.

Inventory boundary: Orderly WS `position` currently decodes a typed array and
execution-report arrays use a typed iterator. They are still outside per-row
admission; do not label them migrated by changing REST imports. Audit/migrate
those paths next using the engine core and offline red/green controls, preserving
financial execution validation and terminal/proof side effects. Structural
frames and immutable fill contracts must not be weakened to claim fan-out.
Remaining families follow after Orderly, before the snapshot token.

Verification: preserve existing mixed-row/continuous-WS/mode/metadata and
Orderly margin/partial-PnL/algo/stale-ACK tests. Run Bitget + Orderly + Binance
native tests, touched-crate strict Clippy/fmt, bans and root typecheck/lint under
`CARGO_BUILD_JOBS=4`. As a semantics-preserving migration, use existing behavior
regressions and full raw equality rather than adding implementation-mirroring
tests. Independent plan/code/adversarial/evidence review must answer the
architecture gate: shared engine adoption, no per-venue shared policy.

CI owns release builds/full strict/size/timing. Compare complete archived TS
and Rust correctness/performance outputs for all Bitget/Orderly aliases plus
Binance against source-equivalent actual landing. Require exact literal suite,
tape SHA/books/mode/run coverage, distinct contained raw files and run/head/source
receipts; exclude only top-level metrics. Any dependent landing change needs
actual integration and a matching baseline. Keep all known keys/declarations and
raw IDs/requests/receipts/checkpoints. Full green head, fresh exact-scope guard
and merge as separate calls, actual merge audit and Aimux report remain required.
No local release builds, replay/load/timing sweep or live capture.

Independent phase-2 plan review CLEAN, architecture answer **No**, before code.
The exact archived suite includes separate BitgetUSDC (shared crate) as well as
Bitget, Orderly and Binance: 29 workloads / 184 complete outputs / 92 Rust.
Preserve Bitget synthetic USDC provenance; do not label it a live capture.

Phase-2 local implementation review CLEAN, architecture **No**: Bitget grouped
mode reports and silent barriers, Orderly PnL/flat/unknown/page/root decisions
and parent/child freshness metadata are unchanged. 628 Bitget/Orderly/Binance
native tests, strict touched-crate Clippy/fmt/bans and 44 root typecheck/28 lint
tasks pass. Full current-head CI and all 184 raw outputs remain pending; these
local results do not establish complete migration of Orderly WS or other venues.

Phase-2 dependency refresh: history #1710 landed at actual
`1910baa65a3fa78d3c69a0b726727a96f87438bf` during the original final guard.
No merge was attempted on the old proof. A clean porcelain integration preserves
all incoming history/source/tests/caller/index blobs and both independent
quality-doc hunks, with exactly the four phase-2 paths against landing.
Original head `5b824f2df30` had 43 green checks and184 equal complete outputs;
that evidence is historical. Fresh integrated-head CI/raw comparison is required
against source-equivalent #1710 CI `39710315859` (head `8dce0521f7e`). The owner
granted #1711 a source priority window through this refresh; release on actual
verified merge. This changes sequencing/evidence only, not row behavior.


## Phase 2 merged receipt

Actual #1711 `bd1a69072c7edf66ea78107c62bb96bd162c17c8`, frozen green head
`ce72f589e42f0e8aa7acee440e033dbe6f66119d`, Rust CI `39711922765`.
All43 applicable checks succeeded; browser PR job intentionally skipped. Fresh
184/184 complete outputs (92 Rust,29 workloads,72 stable groups) equal the
source-equivalent actual #1710 baseline CI `39710315859`, excluding only
top-level metrics. Full identities/requests/receipts/checkpoints and ordered
diagnostics are unchanged:6 PASS/23 known FAIL. Independent raw/body review
CLEAN; separate exact-four-file guard preceded separate merge. Actual parents
[1910baa65a3,ce72f589e42] and whole tree match; incoming files preserved.
Corepack's initial registry HTTP503 failed before tests; the unchanged-head
retry succeeded. No gate was waived.

## Phase 3: Orderly keyed WS admission

Base actual #1711 `bd1a69072c7e`; fresh branch
`feat/rust-row-admission-orderly-ws`. Commit/review this plan before tests/code.
Finish this and every remaining keyed admission consumer before the snapshot
freshness token. Use existing engine `RowAdmission`; no local rejection
collector, copied shared policy, frame-wide conflict rule or new repair loop.

Validate the complete envelope and raw container before financial effects.
PositionPush will hold borrowed RawValue elements; execution data accepts one
object or a complete raw array. Broken JSON, wrong list/container types and
duplicate required wrapper fields stay structural/inert. Decode/map each row
once, recover native identity if possible, then engine-admit it. A rejected row
retains its current copy; healthy neighbors continue. Process deltas sequentially
in native order so accepted earlier updates are visible to later same-ID rows;
a valid row can heal a rejection, and a later rejection retains the accepted row.

Split execution preparation from mutation. Pure native preparation validates
all fallible mapping and supplied root/symbol/child identities and returns an
owned sparse-terminal or borrowed-row mapped decision. Only engine-admitted
decisions may commit root stamps/tombstones, child states, proofs/revisions or
execution bookkeeping. Legitimate sparse leaves and empty root-container
controls remain valid without fabricated working economics. Reject a malformed
native nested tree as one row, with one coded row.skipped diagnostic.

Preserve normal mapped execution processing before display freshness, so late
fills survive; sparse terminal executions retain their separate existing stamp
gate. Preserve root cascades, BRACKET boundaries, source stamps, amend/margin
proofs, terminal caches and bounded reconciliation. Position mapping stays pure;
only admitted rows update stamps/revision/store. Unknown identity never grants
absence; explicit valid zero/terminal semantics remain native. No synthetic
fills, global rejection revision increments or per-row network repair.

A separate read-only finding at account.rs1393-1430 writes a cumulative execution
watermark before validating execution price. First demonstrate actual fill loss
with a native RED control on original source, then report cause/scope and obtain
the architecture decision before changing that helper. Do not silently widen
this migration into a new fill contract or per-venue financial policy.

Likely files: Orderly account.rs, wire.rs, tests/account.rs, this existing plan
and quality/audit ledger. No signer/request/dispatch/host/SDK/clock, fixtures,
manifest, normalizer, workflow or caller contract changes. If a general staging
policy proves necessary, stop/report and extend the engine first.

RED controls: malformed first/middle/last position and normal/algo execution
rows, decode and mapping errors with healthy later peers; malformed terminal
root with invalid child identity/side must preserve siblings/proofs/tombstones
and permit a later lower-sequence valid report; same-ID valid/invalid ordering;
structural-container inertness; no fabricated fills/writes; row.skipped once.
Retain existing root-before-children/cascade, sparse/BRACKET, late/duplicate-fill
and stale-order regressions. Old-source failures must be assertions, not compile
errors. Verify focused Orderly+Binance native tests (engine if changed), strict
Clippy/fmt/bans and root typecheck/lint with CARGO_BUILD_JOBS=4. CI owns release,
size, timing and strict full replays. Archived Orderly15+Binance9 =24 cases/138
outputs/69 Rust, complete IDs/requests/receipts/checkpoints/ordered diagnostics,
only top metrics excluded. Healthy fixtures should be byte-identical; any
intended change needs exact reviewed fix evidence, never wider allowances.

Independent plan/code/adversarial/final evidence reviews answer the architecture
gate. Full green exact head, fresh guard and separate merge, actual merge audit
and Aimux report. No live capture or local release/replay/load/timing sweep.


Phase-3 independent plan review CLEAN at `be4228c19c6` before tests/code.
Owner approved the demonstrated Orderly financial validation in this phase:
validate supplied quantities/prices before root/total/dedup effects, then engine
admission. Native original-source control loses a0.25 cumulative-only corrected
fill after an invalid price consumed its watermark. Ten new admission tests run
on actual #1711 source compile and yield nine assertion failures and one passing
structural control; the reviewed implementation passes all35 account tests.
Two adversarial follow-ups separately proved wrong-symbol root-only cascades and
inherited foreign-parent rebinding RED before repair. Lower-sequence valid roots
and absent-symbol/root-before-children controls remain accepted.

Architecture follow-on decision: independent read-only inventory found twelve
Rust venue crates deriving incremental quantity from cumulative deltas, including
Orderly (eleven others: Bitunix, Bybit/Hyro, Bitget, Coinbase, WOO, BloFin, BingX,
Bitrue, ApeX, AsterDEX and Kraken Spot). KuCoin Spot also has an unsubtracted
cumulative-size fallback; Binance uses cumulative progress for dedup but emits
direct last quantity; OKX emits direct fill size. Therefore validated cumulative
preparation/commit is cross-cutting and belongs in one engine helper folded into
the later snapshot commit/freshness-token work. Do not fix those other venues
individually here. Hyro/ApeX/Bitget validation-order candidates are source-only
findings, not RED-proven financial defects. This approved Orderly native repair
does not establish a new shared fill contract or finish that follow-on.

Final local phase-3 readiness:400 Orderly/Binance native tests pass, including
37 account tests and explicit normal late-unique-fill versus sparse-terminal
stamp-gate controls. Strict touched-crate all-target Clippy/fmt/bans,44 root
typecheck and28 lint tasks pass. Independent code/adversarial review CLEAN;
architecture answer No for this approved shared-boundary adoption. Full exact-
head CI, current dependency integration and all138 archived outputs remain
pending; local results do not claim them or complete the other keyed consumers.

Latest owner sequencing decision (supersedes the earlier bundled-helper order):
after this Orderly WS PR, build the cumulative-fill derivation engine choke point
first, proving source-only financial-loss candidates before repairs; then extend
persistent row reporting and migrate its remaining consumers; finally implement
the snapshot commit/freshness token. The twelve-crate inventory includes eleven
public cumulative-derived Fill paths plus ApeX synthetic execution/history
progress; ApeX cumulative orders intentionally do not emit public Fill. Preserve
that settled distinction and do not fabricate a public-fill RED claim there.

Dependency refresh: actual infra #1713 `04bec2ac52ff295ad94f912145269feefc142197`
was integrated by clean porcelain merge `134bcf0ec797`. All eight incoming files
and six owned-file blobs were preserved; exact six-file scope against landing.
Initial head `148c941fed6` suffered four unrelated Corepack registry503 failures
before replay; its complete Orderly48-output comparison is unchanged. Initial
CI is historical iteration evidence; final actual clock/workflow dependencies,
full green head and complete138-output proof still gate merge.

Clock refresh: actual #1709 `6f815755c37de240d1a87bfec6edc1e4d4c8b614`
was integrated by clean porcelain merge `3a80a651c8b7`. Exact six-file scope
survives against that landing: all nonshared incoming/owned blobs and both quality
hunks are preserved, with the three reviewed source/test blobs unchanged. Final
proof baseline is clock CI `39714718741`, head `07cd88601774`, whose tree equals
the actual clock merge. Owner queue is history #1712 first, then this PR; signer
and install-retry infra follow-up hold. The owner permits a clean trial against
actual #1712 only if its real diff and dependency review show no overlap with
this change; otherwise integrate it and refresh normally. No general waiver.

Phase-3 actual receipt: #1715 merged `2972949f896da9449655011d7e1f36a6f01396f3`
after actual history #1712 `0e15f31a874d33a877a155c9698bc83531eb37fb`, using
the owner-approved independently verified history dependency trial. Green head
`2ad1adefbed937654c91a596f3a90a72d05d37ab`, Rust CI `39717118698`, all43
applicable checks SUCCESS and expected browser SKIP. Full138/138 raw outputs
(69 Rust,24 cases,58 stable groups) match actual clock baseline CI39714718741;
only top-level metrics excluded,4 PASS/20 known FAIL unchanged. Independent raw
review CLEAN, fresh separate guard exact6 then separate merge; actual parents,
tree and every incoming/owned blob verified. Architecture answer YES for the
financial derivation concern: this bounded Orderly repair was explicitly owner
approved, and the next fresh plan is [RUST_CUMULATIVE_FILLS.md](RUST_CUMULATIVE_FILLS.md).
Admission/reporting fan-out and snapshot token remain open in owner order.

## Persistent row reporting — 2026-10-23 plan

The cumulative-fill fan-out finishes its Orderly slice in actual #1767
`bf362312d3edfb4dfa00b726c172f944053b9198`. Its separate guard/merge and complete
tree audit pass, with strict976/976 raw equality. The financial snapshot token
already landed in #1759. This fresh `feat/rust-row-reporting-core` phase starts
from actual #1767 and implements the assigned reporting extension; it does not
reopen financial preparation, Store freshness or RequestContext authority.

Read-only inventory finds eighteen account source files with `SKIP_LOG_LIMIT`,
plus Bybit's deferred unsupported-order memory and WOO's changed-message memory.
Repeat suppression, one Error upgrade, namespace re-arm and grouped/deferred
emission are cross-cutting policy and belong in engine `coverage`, alongside
`RowAdmission`/`SnapshotBuilder` and the existing coded `row_skipped` boundary.
Native identity recovery, message formatting, retained-row lookups and the
original reporting time remain native. No new production defect is claimed.

Work in bounded, separately reviewable phases:

1. Shared persistent report memory, with Coinbase (Error upgrade/namespaces),
   Crypto.com (once per key) and WOO (changed message) as real consumers.
2. Shared grouped/deferred policy extension and consumer adoption, after a
   concrete native-policy audit: Crypto.com counts every skipped row and retains
   its first label; WOO counts unique known IDs in first-seen order plus anonymous
   rows; Bybit reports unsupported rows only after joined snapshot admission.
   Preserve these distinct policies rather than forcing a single count rule.
3. Fan the engine path out to the remaining actual copies in bounded slices,
   coordinating account/pending regions with signer and RequestContext lanes.
   No fan-out closure claim until the remaining inventory is empty.

The concrete phase-1 API in existing `engine/src/coverage.rs` is:

- `RowRepeat::{Once, ErrorUpgrade, ChangedMessage}`: explicit native repeat
  policy, stored once per reporter rather than selected differently per key.
- `RowReports<K>`: one private identity-to-report-state map. `new(RowRepeat)`
  uses the shared4096 clear-on-cap boundary; `Default` is bounded `Once`.
  `unbounded(RowRepeat)` preserves the existing WOO fixed-kind/Bybit policy
  where applicable. No hidden eviction or new per-family threshold.
- `report<P>(&mut self, cx: &mut Cx<'_, P>, key: K, level: Level,
  message: impl Into<String>) -> bool`: update memory independently of host log
  filtering, and emit only through `row_skipped`. Return whether repeat
  suppression admitted a call to `row_skipped`, even if the host filters it. Once ignores later severity changes; ErrorUpgrade permits exactly
  the old non-Error-to-Error upgrade; ChangedMessage compares exact text, with
  no new severity-based repeat. Empty/unknown identity remains a native key.
- `forget<Q>(&mut self, key: &Q) -> bool` with borrowed-key lookup, and `clear`.
  `RowReports<String>::rearm_namespace(prefix, clean, mapped)` preserves exact
  native prefix selection: a clean snapshot clears only that namespace;
  otherwise remove only its successfully mapped keys. No peer namespace reset.

Bounded reporters must clear the entire native memory at capacity BEFORE looking
up the next key, including an already remembered key. This reproduces the
existing wrappers' boundary rather than silently replacing it with LRU or
clear-only-on-new-key behavior. Suppressed reports do not increment health;
reported notices retain the exact level, code, message and emission order even
when the host filters delivery. This memory never changes coverage/protection,
retained rows, snapshot eligibility, native Fill identity or financial progress.

Phase-1 source scope: `coverage.rs`; Coinbase/Crypto.com/WOO `account.rs` and
focused native/helper tests; existing admission/audit/quality/caller documents.
Audit every Account construction path before replacing a field: Coinbase's
reporter must initialize ErrorUpgrade, Crypto.com's Default stays Once, WOO's
fixed-kind reporter stays unbounded ChangedMessage. Replace wrapper policy,
not native label/identity extraction. Existing grouped collection/formatting
and deferred sites stay unchanged in this first phase and remain explicit
phase-2 work; do not claim those copies are removed by phase1.

Commit this plan before tests/source. Independently audit its API/compatibility
and architecture gate. Add meaningful native compatibility controls first and
run them against unchanged production; architecture-only controls should pass
original code, not be presented as invented REDs. New engine controls cover all
repeat policies, unknown keys, namespace clean/partial re-arm, exact capacity
reset (including duplicate keys), borrowed lookup and log filtering/health.
Preserve native whole-snapshot/healthy-peer behavior with existing regressions.

Run focused engine/Coinbase/Crypto.com/WOO plus Binance native tests with four
Cargo jobs, strict Clippy/fmt/bans and root checks. Independent source/adversarial
and caller review precede the source commit. Integrate actual shared inputs and
use full green CI plus authenticated post-#1762 all-suite raw/ordered-gate proof;
no local release WASM build/replay/timing sweep and no changed thresholds or
allowances. Standing TS classes require fresh complete controls; any other
difference blocks. Announce a fresh separate guard, merge separately, audit the
actual parents/whole tree and report each actual pin before the next fresh slice.


Both independent phase-1 plan/API reviews are CLEAN. Compatibility controls
must pin Warn→Error→Warn→Error, text A→B→A, capacity reset on a duplicate key,
empty-key re-arm, namespace isolation and filtered notices followed by repeats.
Coinbase and WOO must select their policies explicitly in `Account::new`;
Crypto.com retains its two independent default Once reporters. Preserve
Crypto.com's decoded-key forget before market lookup, its clean position clear,
and its order clear only after both joined lists are clean. Coinbase order
namespace re-arm stays after revision rejection and before newer-cache merging.
WOO position reporting remains before bad/freshness rejection, while order
reporting retains its distinct generation/bad/envelope ordering. No grouped
collector or native reporting boundary moves in phase1.


Phase-1 implementation checkpoint: the three native adapters now use the shared
reporter with original event sites and re-arm ordering intact. Four new engine
controls pass (repeat/health, filtering, namespace/empty identity, bounded and
unbounded capacity); all14 coverage unit controls pass. Exact final formatted
native compatibility controls pass against all four original production blobs:
Coinbase59, Crypto.com28, WOO55. These are compatibility GREEN, not bug REDs.
The fixed five-crate suite passes1172 tests; strict all-target Clippy, scoped
formatting, repository bans, root44 typecheck and28 lint tasks pass (root tasks
cached). Both independent source/caller reviews are CLEAN. Full current-head CI,
authenticated all-suite raw/ordered-gate comparison and a fresh separate guard
remain required; this checkpoint is not a merge or replay-evidence claim.

## Grouped and deferred row reporting — phase 2 plan

Phase1 #1769 is actual `90dc6637d6e12eafecc283d0dfb9a78e7fc967d6`, with
all43 applicable checks green on unchanged1be4, separate guard/merge, and an
actual whole-tree audit matching the green head/trial. Its full proof retains
strict974/976 FALSE solely for freshly authenticated ApeX/TxFlow TS request
counts; all488 Rust and all110 ordered gates are unchanged. Persistent memory
is shared for its first three consumers; grouped/deferred policy and the other
seventeen bounded source files remain open, including KuCoin Spot `mod.rs`.

This fresh `feat/rust-row-grouped-reporting` starts at actual90dc. The owner
holds landing merges for RequestContext #1741. Plan, tests and implementation
may proceed locally; integrate its actual shared inputs before final CI/proof.
No source migration or merge claim is made by this plan.

The audit identifies two different grouping contracts, not interchangeable
count rules. Put both explicit collectors in `engine::coverage`, with private
state and no venue names/messages:

- `RowCount`: count every `add(Option<&str>)`, including duplicate known IDs.
  Keep the first supplied label exactly. An anonymous first row remains first;
  `Some("")` stays an empty label, distinct from `None`. Private first-state is
  `Option<Option<String>>`, where the outer None means no row. `Default`, `add`,
  `count() -> usize`, `any() -> bool` (native count >0) and
  `first() -> Option<Option<&str>>` expose the existing Crypto.com policy.
- `RowKeys`: keep nonempty known IDs uniquely in first-seen order; count every
  anonymous or empty-ID row separately. `Default`, `add(Option<&str>)`,
  `ids() -> &[String]`, `anonymous() -> usize`, `count() -> usize`, `any()`
  expose WOO's existing policy. `into_ids(self) -> Vec<String>` preserves the
  existing pagination move without cloning strings. No sorting or new limit.
- Each collector's `summary(format: impl FnOnce(&Self) -> String)
  -> Option<String>` calls the formatter once only for a native nonempty group.
  RowCount tests first-state presence, preserving the existing report boundary;
  RowKeys tests known/anonymous presence. Keep native usize arithmetic rather
  than silently adding saturation or another financial/coverage counter.
- `row_summary<P>(cx: &mut Cx<'_, P>, level: Level,
  summary: Option<String>) -> bool` emits one `row_skipped` for Some and nothing
  for None. Grouped row count is not the health counter: a summary increments
  health once at its original site even when host delivery is filtered.
- `RowReports::report_summary<P>(&mut self, cx: &mut Cx<'_, P>, key: K,
  level: Level, summary: Option<String>) -> bool` uses the existing repeat path
  for Some; None forgets only that key and reports nothing. It owns WOO's
  existing empty-summary re-arm, with no automatic lifecycle/timer policy.

Actual consumers and preserved boundaries:

1. Crypto.com `lib.rs::Skipped` becomes a thin RowCount adapter. Keep `add`,
   `any` and the exact native summary formatter/placeholder `without an id`.
   Discovery and account consumers report at their current accepted/read/frame
   points through `row_summary`; no per-row replacement events or changed level.
2. WOO `account.rs::Skipped` becomes a thin RowKeys adapter. Keep exact native
   summary punctuation, order and IDs/anonymous accessors used by retention.
   `account_skipped` uses report_summary with its existing unbounded ChangedMessage
   memory. Positions still report before their bad/freshness rejection; joined
   orders keep their distinct generation/bad/envelope ordering. No admission,
   healthy-peer, position-mode or financial behavior change.
3. Bybit `unsupported_logged` adopts unbounded Once RowReports. Keep staging in
   each native read and flush ONLY at the existing admitted joined-order point,
   after rejected/fresh-row merging and before Store removal/upsert. Stored-copy
   lookup, exact Error text, restoring known unsupported rows and emission order
   stay native. Admitted partial joins still flush unsupported rows; only whole
   failed, noncurrent or incomplete joins skip flushing. Known-copy restoration
   stays outside suppression, and mapped/clean rows never re-arm Bybit memory.
   Add `AccountState::new()` selecting unbounded Once, and replace ONLY the two
   actual lib.rs construction sites (main account and extra product accounts).
   Audit all default/reset/move paths; no default bounded reporter may leak into
   a real Bybit account. Coordinate these narrow constructors/account-reporting
   fields with RequestContext/signing owners before source edits.

Phase2 scope: engine coverage; Crypto.com lib.rs, WOO account.rs, Bybit account.rs
and the two lib.rs constructors; focused engine/native tests; existing admission,
audit, quality and caller docs. No request/pending/Job/Rest/trading authority,
engine dispatch/Store/token, signing/clock, metadata/history, financial reducers,
wire payload, comparator, tape or manifest changes. Architecture gate YES:
grouping, summary-empty/repeat and deferred emission memory are cross-cutting;
the engine owns those policies while native formatting, identity and final
admission timing remain native. Two different explicit engine collectors avoid
forcing WOO's unique-ID count onto Crypto.com's duplicate-inclusive count.

Commit this concrete plan before tests/source; independently audit the API,
construction paths and architecture first. Native compatibility controls must
pass original production, not be mislabeled defect REDs: Crypto.com duplicate
known/anonymous/empty-first labels and exact one-summary health; WOO duplicate
known IDs, anonymous rows, order, changed summaries and clean re-arm; Bybit
unsupported diagnostics only after accepted joined snapshots, suppressed repeats
and healthy/known-row retention after rejected/stale pages. New engine tests pin
both count policies/first-label distinctions, deferred/empty summary behavior,
repeat re-arm, exact code/level/message and filtered health. Preserve all existing
coverage/snapshot/financial controls.

Run focused engine/Crypto.com/WOO/Bybit plus Binance native tests with four Cargo
jobs, strict all-target Clippy/fmt/bans, root checks and relevant native alias
fixtures. Independent source/adversarial/caller reviews precede the source
commit. Use current actual #1741/shared dependencies and post-#1762 all-suite
CI proof: all488 Rust/full IDs and all ordered gates exact, four standing TS
classes only with freshly authenticated complete controls. No local release
WASM/replays/timing, threshold/profile/allowance changes or live captures. On
full green, announce and run a fresh separate guard, merge separately, audit the
actual tree/parents and report its pin. Remaining seventeen bounded reporter
files are later bounded fan-out slices, not closed by this phase.

Phase2 plan/API review checkpoint: both independent correctness and adversarial
reviews are CLEAN. Preserve WOO retention/page-progress accessors and Bybit extra-
product account swapping. Original-GREEN compatibility controls precede migration;
no product defect or RED claim is made. Signing/RequestContext owners explicitly
cleared these AccountState constructors and accepted-join diagnostic regions.

Phase2 original compatibility checkpoint: all233 selected native account/public/
product tests pass on byte-identical actual1769 production, including the final
new stale-join and two-known-ID first-seen-order controls. Both native test/API
reviews are CLEAN; the consuming RowKeys ID accessor preserves WOO pagination
move semantics. Early extra-product fixture cadence and an intermediate WOO
fixture edit were corrected before this exact original-GREEN run; neither is a
product defect/RED. Native source/test/log hashes are sealed in the local receipt
`/tmp/codex-row-grouped-original-receipt.json`; CI proof remains pending.

Phase2 implementation checkpoint: the explicit collectors and optional summary
paths now serve Crypto.com/WOO; Bybit unsupported-order memory uses unbounded
Once at its unchanged accepted-join boundary. Both real construction paths and
product-state swaps preserve the policy. All1345 focused five-crate tests pass;
strict all-target Clippy, scoped fmt, bans, root44 typecheck/28 lint pass (root
tasks cached). A Clippy-only test assertion expression was corrected; exactfinal
233 original controls were rerun with all five original production blobs and
byte-exact restoration of the migrated source, with receipt
`/tmp/codex-row-grouped-original-exact-receipt.json`. Both independent source/API/
caller reviews are CLEAN; repeat-suppressed summary health wording was tightened.
Actual #1741 integration, current-head full CI and authenticated all-suite raw/
ordered-gate proof remain required. No merge, live or all-venue closure claim.


## Next reporter fan-out: BloFin and Kraken (plan, 2026-10-23)

Start from actual grouped/deferred reporter merge #1775,
`15c6334b514e9edd34ad2fa38034aab1277148ba`. Its current-head evidence is
strict975/976 with all488 Rust outputs and110 ordered gates unchanged; the sole
ApeX TS count variant has both complete pre-change controls authenticated. That
completed proof does not substitute for the new slice's proof.

Architecture gate: **YES, this is another consumer of cross-cutting reporting
policy.** Use existing engine `RowReports`, `RowRepeat`, `forget`, `clear` and
`rearm_namespace`; no new venue or engine policy. Scope is BloFin account.rs,
Kraken futures account.rs, Kraken Spot account.rs, their existing native account
tests and this plan/AUDITS/QUALITY/CALLERS. No execution arithmetic, row admission,
Store/freshness, signing, request authority, WS lifecycle or history changes.

1. BloFin position/order skip sets become separate bounded Once reporters.
   Keep the thin native formatting adapter, exact skip severity and empty IDs.
   Decode-based `forget` stays before mapping/final read acceptance, including
   reads that later fail or become stale. Accepted position revision clearing
   stays at its native site even for a held builder; order clearing stays only
   on its clean accepted path. Once never gains Warn-to-Error upgrades.
2. Kraken futures skip memory becomes bounded ErrorUpgrade, explicitly selected
   in the real `Account::new` constructor. Preserve order/position namespaces,
   raw-symbol position labels, stale WS no-rearm and `recovered` filtering of
   accepted/rejected duplicate identities. Use the shared namespace rearm at
   the existing sites. Its distinct unlisted-symbol memory becomes explicit
   **unbounded Once**, also explicitly initialized in `Account::new`: raw
   symbols remain shared across REST/WS/orders/positions;
   clean/mapped reads and reconnects never rearm it. Reporting stays during
   mapping before snapshot admission: stale/held reads still consume unlisted
   suppression, while unknown zero-quantity positions remain silent and consume
   no memory. Malformed catalogue holds remain a separate admission decision.
3. Kraken Spot skip memory becomes bounded Once with the native coin/order
   namespaces, including an empty order identity. Keep diagnostics before
   admission where native, mapped-only versus clean namespace rearm, and
   `StaleGuard` decisions: rejected stale reads do not rearm; admitted stale
   balance reads do. Keep REST orders' native unfiltered accepted-ID rearm
   iterator; balance and WS use their native `allows_absence` filtering. A
   valid-plus-rejected duplicate identity must not prompt a filtering cleanup.
   Do not tighten admission during a reporter migration.

Before production edits, add native original-GREEN compatibility controls for
those exact ordering/duplicate/severity/identity boundaries. In particular pin
BloFin decoded-but-unmapped and later failed/stale reads, all three order aliases,
futures Warn/Error progression and independent keys, permanent unlisted-symbol
suppression including stale/held and zero-quantity controls, and Spot
rejected-stale versus admitted-clean rearm. Run the selected
final tests against unchanged production and seal the source/test/log receipt;
rerun unchanged controls after migration. No product defect or original-RED claim.

Verification: focused engine/BloFin/Kraken/Binance native suites, all-target strict
Clippy, scoped fmt, bans and root checks, with `CARGO_BUILD_JOBS=4`. Independent
correctness/adversarial plan and source/caller reviews; preserve exact incoming
docs and source on integration. Fresh full green CI, complete32-suite raw and
ordered-gate proof with current whole-variant controls, separate porcelain guard
then separate merge. No local release WASM builds, full replays, timing tests,
threshold/budget/manifest changes or live captures. The #1781 clock-fixture merge
slot comes first; integrate its actual shared inputs before final proof. Coordinate
BloFin/Kraken whole-family signing with its owner; those signer edits remain held
for this slice's actual pin. Re-audit the remaining bounded and unbounded reporter
inventory after this slice; this plan makes no all-venue closure claim.
