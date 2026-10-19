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

## Implementation checkpoint

Public/config, private, trading/queries and native-unit frame transport are
implemented and cross-reviewed locally. Public/config has 6 native cases,
private variants 15, trading/recovery 25, plus the original 43 Bybit cases.
The shell has 12 wasm integration tests and 4 independent binary/copier tests.
Root typecheck (44 tasks) and lint (28 tasks) pass. Final integrated gates are
in progress; the landing branch's new OKX port will be merged before final checks.

Review fixes include strict Hyro URL/row validation; correlated handshake and
atomic stale-safe snapshots; lost/duplicate create recovery; snapshot/WS fences;
Demo REST acknowledgement publication; exact inverse native units across frames;
and Hyro order-only incremental fills with reconnect snapshot seeding.

TS defects fixed: inverse quantity clamp/truncation before splitting; inverse
coin PnL summed into USD balance; spot instrument precision/limits and invalid
pagination; market-buy quote/base ambiguity; derivative-only spot controls and
position-stop endpoints; spot cancellation families and `tpslOrder` mapping;
`PartiallyFilledCanceled` events ignored; Hyro historical partial fills replayed
after reconnect. Each is listed in `gate.mjs`'s `KNOWN_TS_FIXES` comments.

Live 20-second captures for inverse/spot/demo are committed as trimmed fixtures.
Hyro origins are not configured in this environment: its public fixture is
explicitly documented/synthetic, and live Hyro connectivity remains unverified.
All private fixtures use documented payloads without credentials. Classic
CONTRACT-wallet valuation without aggregate USD fields retains the pre-existing
TS limitation; these account gates cover Unified accounts. Ambiguous Hyro
position-stop writes are reported/fenced because that endpoint has no client ID
with which to authorize a safe automatic replacement.

The harness now includes module allocations symmetrically in retained memory
(the prior baseline excluded TS imports but counted all wasm memory), compares
market flags, isolates concurrent output files and fails bounded I/O/readiness
waits. No correctness or performance threshold was relaxed.
