# Bitget family parity

Branch: `feat/rust-bitget-parity`, based on latest `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process: current `VENUE_PORT_BRIEF.md`.
Full automatic execution, phase-end independent reviews, stable-point commits,
PR into `feat/rust-rewrite`, adversarial findings posted to PR, merge only on
green final-head CI. No stash, rebase, force push or master changes.

## Phases and verification

1. Public/inverse foundations: add coin-futures product and registration;
   preserve TS native USD placement quantities and documented display conversion.
   Verify contracts, precision, quote filtering, tickers/books/candles and routing.
   Audit shared model/frame support for collateral/native quantities before edits.
2. Private/inverse: per-coin position reads committed atomically; multi-coin
   balance valuation, settings and WS reconciliation; inverse order display and
   native quantities. Failed reads preserve authoritative state. Verify documented
   payloads, both coins, REST/WS races and empty/error responses; independent review.
3. Trading: per-symbol collateral throughout inverse writes; Classic and UTA
   trailing lifecycle using documented request/response shapes. Preserve immutable
   loss_plan/pos_loss identity, exact native sizing, fresh modify client IDs,
   reads-only retries and ambiguous-write safety. Independent review and tests.
4. Queries: UTA ledger bills with typed streamed records, signed fees, pagination,
   bounded time windows and explicit failure on incomplete reads. Retain shared
   history JSON shapes. Independent review and tests from official docs.
5. Harness/gates: inverse, UTA bills and trailing documented private-tape cases;
   exact outbound requests and command results. Register any demonstrated TS bugs
   explicitly; never weaken correctness/performance checks. Bans/fmt/clippy/native
   tests, wasm clippy/build, Bitget and Binance public/private gates, root quality
   checks. Independent phase and adversarial PR audits, fix and repeat.

## Plan audit

- Current inverse TS uses native USD order amount/filled, converts only remaining
  at order price and positions at ticker mark/last/entry to 8 decimal places.
  Do not apply a blanket base/quote conversion to writes.
- Inverse UTA stays gated as TS; Classic per-symbol collateral must support ETH
  as well as BTC. Existing engine representation needs audit before claiming parity.
- UTA TS currently rejects bills/trailing. New documented engine support needs
  explicit independent tape assertions as well as preserved existing TS oracle.
- Trailing callback units and v2/v3 payload fields must be checked against official
  Bitget docs, not guessed from the existing unsupported branches.
- Sub-agent work is restricted to disjoint files; parent owns model/host/harness
  integration and commits. All implementations receive an independent audit.

## Progress

- Fetched landing branch, read current brief/spec and Bitget guidance; clean fresh
  branch created. Inverse audit and documented ledger/trailing implementation
  research are running in separate bounded scopes.
