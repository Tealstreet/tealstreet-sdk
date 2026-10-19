# Bybit variants port

Branch: `feat/rust-bybit-variants`, based on `origin/feat/rust-rewrite`.
Landing target: `feat/rust-rewrite`; merge only after all PR CI is green.

## Baseline

PR [#1298](https://github.com/Tealstreet/tealstreet-sdk/pull/1298) was watched
to completion before implementation. All six Rust checks, both TS CI runs
(5m49s and 8m56s), and both secret scans passed. No repair PR was necessary.

## Phases

1. Public/config: one `Variant` option and paths table in the existing Bybit
   crate; inverse USD contracts, spot precision/depth, demo routing, explicit
   Hyro endpoints. Preserve socket watchdogs, pagination and startup guards.
2. Private: category filtering, inverse coin/base/quote normalization, spot
   balances without derivative positions, and Hyro's correlated handshake,
   atomic reconciliation, readiness and session/snapshot guards.
3. Trading/queries: native inverse quantities, spot single writes and supported
   conditional shapes, demo routing, Hyro identity-preserving recovery; retain
   client-ID and reads-only retry contracts.
4. Integration/gates: register variants in wasm, TS shell and harness; documented
   private tapes and live public fixtures; run every venue-brief gate, Binance
   regressions, root typecheck/lint/tests and adversarial phase/PR reviews.
5. Push/open PR into `feat/rust-rewrite`, post findings, fix/review until passing
   or diminishing returns, wait for green CI, merge and cut a fresh branch.

## Review and verification

Implementation is in progress. Each phase receives a local sub-agent audit;
final numbers and review findings will be recorded here before merge.

Public tests and private tapes derive from current Bybit V5 documentation and
the repository's documented contracts. Hyro uses the October 6 simulator subset
documented in the Bybit CLAUDE.md; endpoints are explicit configuration and
credentials never come from simulator defaults.
