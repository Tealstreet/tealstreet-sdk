# Engine row admission and snapshot isolation

Owner: codex-mxb4mq. Base: actual landing `5569062869971dbfce492e3259bbcd5ebded8869`.
Status: plan before implementation; owner-directed standardization, not a new
per-venue repair. The shared Signer core owns its exclusive source merge window;
this lane avoids request/dispatch/pending-signature code and integrates its actual
merge before final row-core CI/merge.

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
