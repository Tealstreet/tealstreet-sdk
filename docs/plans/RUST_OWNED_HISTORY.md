# Owned history completion and pagination

Owner: codex-cxxyqr. Planned from landing `556906286997` (2026-10-22).
Reference merged in #1710 at `1910baa65a3`; fan-out remains active.

## Contract and sequence

1. Reference PR: engine-owned history completion and cursor paging, exercised by
   BloFin closed positions. Bind a non-cloneable completion context to the real
   command at construction. Shared paging owns cursor/page counts, deduplication,
   budget and stall decisions. Finishing consumes the context and sends any
   command-owned `history.incomplete` before successful retained rows, including
   at Error-only logging. Failure before accepted rows rejects; a later failed
   page returns only previously accepted rows with owned incompleteness. Failed
   response bodies never become accepted rows or complete empty success. Keep
   existing BloFin URLs, ordering, raw rows, short/duplicate-page
   completion and page-cap message. New policy changes need explicit regressions.
2. Fan-out on fresh branches: use the same completion path for remaining marker
   families (Binance, Bitunix, Bybit, Hyperliquid, Kraken, Lighter, OKX, Paradex,
   TxFlow, WEEX) and other paged history consumers. Extend the shared core for
   timestamp, range, offset, multi-stage and composite walks; do not create a
   second venue implementation. Preserve each endpoint's limits, ordering,
   continuation evidence and bounded/unbounded contract, with the shared failure
   policy: first-page/no-accepted-row failure rejects; later failure retains only
   accepted rows with a command-owned incomplete marker.
   Query-associated markers require command ownership; background diagnostics
   remain distinct. Add a CI prohibition on venue-owned completeness emission
   after conversion, and retain B11's downstream NX ingestion/coverage work.
3. Small follow-on after query fan-out: route the six background account-coverage
   diagnostics (Bitunix/Lighter/TxFlow) through one engine-owned account/background
   coverage notice, retaining the code and explicit scope without a command ID.
   These are not query outcomes and must not use fabricated request attribution.
4. Separate follow-up PR after history/background fan-out: capability/feature
   conformance in CI for combined and standalone family builds. Every advertised command must
   survive actual command decoding and reach a supported handler under that
   family's Cargo features. Prove the MEXC standalone order-book missing-feature
   regression red/green; cover aliases and intentional unsupported profiles.
   A grep of manifests or combined-only feature unification is insufficient.

## Reference implementation and verification

- Add `engine/src/history.rs`, export it from `engine/src/lib.rs`, and extend
  shared `paging.rs` only where the reference actually needs reusable policy.
  Completion carries the command, first incompleteness reason/message and
  consumes itself through success/failure. `HistoryRows<T>` stages streamed rows
  separately from accepted pages and owns encoding/recovery; no free-form command
  field at marker sites. A shared cursor wrapper records stalls and produces completion states.
- Convert `venues/blofin/src/closed.rs`; remove its raw marker, cursor/page state
  and dedupe ownership in favor of the shared pager. Keep schema-specific ID
  comparison and URL construction local.
- Tests: engine concurrent command attribution; complete vs incomplete vs failed
  completion; markers before results at Error-only level; repeated cursors,
  budget boundaries, short/duplicate pages and read failure. Pin BloFin request
  sequences before migration. No accidental account/store mutation.
- Focused gates: engine + BloFin + Binance tests, touched-crate fmt and strict
  Clippy, bans. `CARGO_BUILD_JOBS=4`; no local release WASM or replay sweeps.
  Full CI supplies family builds, replay and timing gates. Independent correctness
  and adversarial reviews must explicitly answer the architecture-gate question.
- Update caller/audit/quality docs only for the implemented slice, preserving
  historical evidence and unconverted exceptions. No NX changes.

## Shared-file ownership and landing

Signer lane (codex-uufub4) owns engine dispatch/action/signing sections and host
adapters. History completion/paging is disjoint, and final reference integration
stays outside signer dispatch files. The overseer explicitly permits this disjoint
reference PR to merge on green before signer core; signer integrates it later. Clock lane
(codex-5c57jl) owns time/provider/calibration. Row lane (codex-mxb4mq) currently
owns coverage.rs and Bybit/Bitunix account.rs; later snapshot freshness is its
scope. Coordinate before touching those files or selecting those consumers.

Each PR uses a fresh landing branch, plan-first commit, independent review,
green CI and a separate exact-path revert guard before a separate merge call.
Caller docs stay current against actual merge pins. No source merge during an
exclusive window without overseer release. Signing remains the signing lane's
scope; no request retry/outcome rewrite is mixed into history work.

## Plan audit (before implementation)

Independent read-only audit: reference design is cross-cutting core work, not a
per-venue fix. Preserve duplicate/short/missing/unchanged-cursor termination,
untrimmed dedupe versus trimmed numeric continuation, and caller-limit versus
safety-budget completion. Shared cursor policy will explicitly represent legacy
end-on-duplicate/unchanged behavior. The original plan's strict later-page rejection was superseded by the owner:
first failure before accepted rows rejects; later failure retains only accepted
rows with owned incompleteness. Failed serialization and lifecycle invariant
failures still reject without a marker or successful result. A consuming token prevents its
own reuse, not arbitrary reconstruction or bypass through legacy Cx APIs; final
fan-out must include the venue-emission prohibition. Multi-stage query owners must
survive pager restarts without clearing incompleteness.

## Reference implementation checkpoint

The shared `history::Completion` retains the owner and first missing-coverage
cause until final success/failure. `CursorHistory` owns the cursor walk, dedupe,
stall recording and terminal-state check; construction consumes the same owner
used for no-I/O early returns. BloFin supplies numeric cursor selection and page
facts. All its completion/error paths use that owner, with no raw semantic marker.
Short pages, missing IDs, repeated identities and immediate unchanged cursors
retain their existing endpoint termination. Non-adjacent cursor cycles now stop
early with retained rows and owned incompleteness instead of spending the remaining
budget on a cycle; a dedicated regression pins this intentional improvement.

Independent correctness and adversarial reviews found no blocking issues and
answered the architecture gate: shared engine choke point with BloFin as reference,
not a separate venue implementation. Both asked for the non-adjacent-cycle
regression; it is now included. Remaining emitters are not claimed converted.

Verification so far: the BloFin request-sequence/termination pins passed against
pre-migration source, the new cursor-cycle regression failed at its intended
assertion against original source and passed after restoration, and three core
integration tests plus six closed-position tests pass. Touched engine/BloFin
strict all-target Clippy, fmt, bans and touched relative doc links pass. Full
engine/BloFin/Binance tests and CI remain final merge gates; no local WASM build
or full replay/performance sweep is being run on the shared machine.

## Owner failure-policy revision (before final reference CI)

Overseer direction explicitly supersedes the original later-page rejection:
first failure with no accepted rows is a typed rejection, never empty success;
a later failed page returns prior accepted rows with one owned incomplete marker.
The engine now owns staged/accepted row buffers and encoding, so a failed envelope
cannot leak its streamed rows. An accepted empty page is insufficient for row
recovery. Serialization failure and premature success are strict errors, not
recoverable page failures. The independent implementation reviews caught the
premature-success-after-accepted-page edge; it now has a strict path and regression.

Actual row core `b2f074bf2637` is integrated by porcelain merge `06b9f5cff29`;
its README/quality/source additions are retained. The source overlap remains
outside signer dispatch and clock/row implementation paths.

Revised-policy verification: original BloFin source fails both first-page typed
rejection and later-page accepted-row recovery regressions at their intended
assertions; the corrected seven-test closed-position suite passes. After actual
row-core integration, 695 engine/BloFin/Binance tests pass, as do touched-crate
strict all-target Clippy, fmt, bans and touched doc links. Independent correctness
and adversarial delta reviews are clean after the strict invariant fix. Full CI
must run on the revised final head before merge.

## Fan-out phase 1: windowed / seeded closed-position reads

Fresh `feat/rust-owned-history-windows` from actual `1910baa65a3`.
Plan before source changes; source merges held for row phase 2 #1711 until release.

- Engine `history.rs`: restart the cursor stage without replacing the completion
  owner, accepted rows or first missing-coverage cause. Restart resets per-stage
  cursor/budget history; premature restart is a strict lifecycle error. Preserve
  cross-stage dedupe state (consumers which retain duplicate rows need not opt in).
- Engine `paging.rs`: seed an initial request cursor, needed by OKX's explicit
  upper close-time bound. Preserve decimal strings; do not coerce IDs through f64.
- Bybit `closed.rs`: replace local page/cursor counters and raw rows with the
  shared owner/pager and staged response rows. Keep seven-day windows, category
  selection, URL encoding, per-window budget and raw duplicate rows. Require the
  success response's result/list before page admission. Native response codecs
  remain venue facts, not a separate completion or paging algorithm.
- OKX `closed.rs`: preserve initial after=to, before=from, oldest uTime-1,
  100 ms page delays, current missing/unchanged-cursor exhaustion and raw rows.
  Move row acceptance, counters, completion and failure recovery into the core.
- Both consumers: first-page/no-accepted-row errors reject; later failed responses
  discard staged rows and return accepted rows with owned incompleteness. Non-
  adjacent cursor cycles stop earlier with incomplete coverage. Serialization and
  lifecycle errors stay strict. Do not modify account/row/store/fence/signing code.
- Tests before migration pin existing request sequences; add original-red/core-
  green first/later failure and malformed-success tests. Cross-window tests must
  prove independent budgets/cursor reset, preserved early incomplete cause,
  marker ordering at Error-only level, raw duplicate preservation, and later
  failure recovery. Guard premature reset even after accepted rows.
- Verify engine/Bybit/OKX/BloFin and Binance native tests; strict touched Clippy,
  fmt, bans, docs links. Independent plan/correctness/adversarial review and full
  CI before separate guard/merge. No local WASM release or replay sweeps.
- Caller doc: pin #1710 and document this slice's later-failure recovery, required
  incompleteness handling and unchanged method args. Do not close B11 or claim
  other histories migrated. Follow subsequent fan-out with account/background
  notice core, then standalone/combined capability conformance.

Phase-1 implementation audit: independent correctness and adversarial reviews
are clean. Stage reset poisons a pending/dirty reset before any recoverable page
failure can hide it; accepted rows, identities and first incomplete cause survive
valid resets while cursor/budget history resets. Bybit keeps raw duplicates and
requires result/list before staging. OKX keeps its seeded bound and delay, and
outer HTTP/envelope validation precedes page admission. Bybit repeated adjacent
cursors now stop early too; OKX retains its immediate unchanged-cursor exhaustion.
The pre-migration request suites passed (six Bybit, five OKX tests); new first/later
failure and missing-list regressions are checked against original source before
final focused gates and full CI. Architecture gate: one shared engine extension
with two protocol adapters, not separate per-venue completion implementations.

Phase-1 focused verification: 1,225 engine/Bybit/OKX/BloFin/Binance tests pass;
touched strict all-target Clippy, fmt, bans and doc links pass. Four original-
source regressions fail at their intended assertions (first/malformed + later
failure per Bybit/OKX), then corrected closed-position suites pass. Source merge
still waits for #1711 priority release, full final-head CI and a separate guard.

Final windowed-slice integration includes actual row phase2 #1711
`bd1a69072c7`, CI bootstrap #1713 `04bec2ac52f` and clock #1709
`6f815755c37`. All seven reviewed history source/test files remain byte-identical;
incoming source and plan changes are retained. The caller refresh also records
#1709's retired hostClock/bit14 and shared clock ingress, superseding historical
venue opt-ins. Both integration reviews are clean; all1,225 focused native tests,
strict touched-crate Clippy, formatting, bans and touched links pass again.
