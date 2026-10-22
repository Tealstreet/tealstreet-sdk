# Owned history completion and pagination

Owner: codex-cxxyqr. Planned from landing `556906286997` (2026-10-22).
Implementation pending; this plan does not claim migrations have landed.

## Contract and sequence

1. Reference PR: engine-owned history completion and cursor paging, exercised by
   BloFin closed positions. Bind a non-cloneable completion context to the real
   command at construction. Shared paging owns cursor/page counts, deduplication,
   budget and stall decisions. Finishing consumes the context and sends any
   command-owned `history.incomplete` before successful retained rows, including
   at Error-only logging. Failed pages reject; they never become complete empty
   success. Keep existing BloFin URLs, ordering, raw rows, short/duplicate-page
   completion and page-cap message. New policy changes need explicit regressions.
2. Fan-out on fresh branches: use the same completion path for remaining marker
   families (Binance, Bitunix, Bybit, Hyperliquid, Kraken, Lighter, OKX, Paradex,
   TxFlow, WEEX) and other paged history consumers. Extend the shared core for
   timestamp, range, offset, multi-stage and composite walks; do not create a
   second venue implementation. Preserve each endpoint's limits, ordering,
   continuation evidence, failed-page rejection and bounded/unbounded contract.
   Query-associated markers require command ownership; background diagnostics
   remain distinct. Add a CI prohibition on venue-owned completeness emission
   after conversion, and retain B11's downstream NX ingestion/coverage work.
3. Separate follow-up PR after history fan-out: capability/feature conformance in
   CI for combined and standalone family builds. Every advertised command must
   survive actual command decoding and reach a supported handler under that
   family's Cargo features. Prove the MEXC standalone order-book missing-feature
   regression red/green; cover aliases and intentional unsupported profiles.
   A grep of manifests or combined-only feature unification is insufficient.

## Reference implementation and verification

- Add `engine/src/history.rs`, export it from `engine/src/lib.rs`, and extend
  shared `paging.rs` only where the reference actually needs reusable policy.
  Completion carries the command, first incompleteness reason/message and
  consumes itself through success/failure; no free-form command field at marker
  sites. A shared cursor wrapper records stalls and produces completion states.
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
must include its actual signer-core merge before final CI/merge. Clock lane
(codex-5c57jl) owns time/provider/calibration. Row lane (codex-mxb4mq) currently
owns coverage.rs and Bybit/Bitunix account.rs; later snapshot freshness is its
scope. Coordinate before touching those files or selecting those consumers.

Each PR uses a fresh landing branch, plan-first commit, independent review,
green CI and a separate exact-path revert guard before a separate merge call.
Caller docs stay current against actual merge pins. No source merge during an
exclusive window without overseer release. Signing remains the signing lane's
scope; no request retry/outcome rewrite is mixed into history work.
