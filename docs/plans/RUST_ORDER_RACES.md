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

### Trading/public checkpoint before landing refresh

Implemented and peer-reviewed: shared feature-gated mutation admission across concurrent/preplanned cancel+replace adapters; serial-intent adapters retain FIFO behavior; seven affected adapters also track pending placements and ambiguous outcomes. Retained native acknowledgement routing closes pre-stream cancellation, including regular/conditional/TPSL/UTA families. SDK cancelAll now retains attempted placement scopes so it reaches native cancellation with an empty store. Explicit partial-business rejections remain cancelable; missing acknowledgement remains uncertain. Complete fresh REST snapshots use causal read epochs and venue snapshot-generation fences; Phemex has no causal REST orders census, so ambiguous sent placement conservatively prevents symbol/global cancellation until recreation (known-native individual cancels remain available).

Backpack remaining-size/cancel-racing execution and 404 preservation, KuCoin collision-resistant fallible IDs, OKX partial-effective/nonfinal status, OKX/Orderly ambiguous-write refreshes implemented and independently reviewed. Private followup seeds BloFin startup cumulative/quote baselines once without letting periodic REST consume stream executions (17 account tests and strict Clippy pass). Wrapper regressions 105 tests plus typecheck/lint pass.

Public fixes peer-reviewed: empty invalidation frames, dropped-clear snapshot recovery, WOO prevTs, Kraken Spot exact-decimal CRC32, Aster/Lighter recovery clearing and stale-REST fences. Engine focused tests 90+ pass, 74 public cases pass. Intentional TS corrections include WOO continuity, Kraken Spot CRC, Backpack 404 preservation, documented in gate comments; replay comparisons are not weakened.

Exploratory selected Paradex build measured 1,258,877 bytes, exceeding fixed 1,258,291 by 586. This is a real gate failure, not accepted evidence. Replace global snapshot subscription bookkeeping with the existing venue book iterator contract, then rerun final-source budgets.

Next: checkpoint all verified work, merge current origin landing (includes other lane PR1338), reconcile shared feature/command changes, lean snapshot repair, strike verified audit items, freeze source and run final gates. Existing focused logs are developmental evidence, not final frozen-source results.

### Lean recovery and merged-source review

Merged current parity landing in `c2eb6aa8ca`, preserving prediction/polling feature isolation, remote-sign request events and KuCoin Spot. Removed global snapshot subscription allocation in favor of the documented `Venue::books` subscription census: snapshots emit clears when `book()` is unavailable. Non-Binance/Bybit iterators now comply; their lane was notified through Aimux of its two readiness-filtered iterators. Engine snapshots retain valid one-sided books and clear stale consumers even after a missed invalidation frame. Developmental engine tests (95+), WOO/Kraken/Aster suites (354) and strict Clippy pass. Merged exploratory Paradex size returned below the unchanged budget; final frozen-source budgets remain required.

Integration review fixed causal-token leaks on rejected/expired delegated signing, duplicate unsupported-command IDs, and late signer errors erasing active HTTP provenance. Five focused signing regressions and strict Clippy pass. Production-impact guidance from Sam governs all scope decisions; lifecycle differences alone are not a reason to change code.
