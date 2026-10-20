# Trading and stream race audit fixes

Branch: `feat/rust-order-races`; base: current `origin/feat/rust-rewrite` (audit references `df2d6a4a03`).

Scope is the requested trading medium/low items in `RUST_AUDITS.md`. Binance and Bybit are owned by the separate `feat/rust-audit-binance-bybit` lane. No TS venue deletion, cutover/default change, budget increase, or live private writes.

## Phases

1. Trading: reverify M1/M2/M3/M4/M11 and low Backpack positions/OKX status/KuCoin IDs. Add shared engine order-mutation admission only where it safely preserves command completion, and regression tests for pending placement, partial fills, overlapping amend/cancel, ambiguous writes and read errors. Review and commit coherent fixes.
2. Private reconciliation: independent stream execution baselines for Bitrue/BingX and Aster/WOO fallback, BloFin session cutoff, Lighter late executions. Regression tests must interleave REST and WS rather than assert final state alone. Review and commit.
3. Public recovery: engine explicit book invalidation; WOO prevTs, Kraken Spot checksum, Aster/Lighter recovery. Preserve legitimate empty/one-sided snapshots. Test resync and recovery and review.
4. Gates: native quality/tests, isolated and wasm quality, root quality, combined/selected WASM and Binance plus every touched venue public/private proof. Fixed budget remains 1,258,291 bytes. Record exact source, results and any limitations.
5. Landing: merge latest landing branch, repeat affected gates, push/open PR into `feat/rust-rewrite`, post correctness sub-agent reviews, fix/repeat, merge only green CI, cut clean follow-up branch. Strike only findings actually fixed in this PR, preserve unrelated findings.

## Progress

- Read origin audit and port process before editing; fresh branch created cleanly.
- Initial re-verification delegated by disjoint account/public/trading scopes. Shared order admission and cancel-all gaps remain with primary agent.
- Every phase requires a sub-agent correctness review; workers must not revert other scopes.

### Private reconciliation checkpoint

Reverified and repaired REST/stream execution baseline races for Bitrue, BingX, Aster and WOO fallback; BloFin pre-session executions now seed baselines without emission; Lighter processes late delivered executions once. Seven deterministic interleaving regressions added. Focused six-venue account suites passed (136 before the extra WOO gap regression). Independent review found and repaired WOO blind-gap pricing/quantity inference; reciprocal public review also passed. Full crate quality and final-source gates remain outstanding. No TS venue was deleted.
