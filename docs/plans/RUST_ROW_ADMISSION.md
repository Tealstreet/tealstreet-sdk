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
