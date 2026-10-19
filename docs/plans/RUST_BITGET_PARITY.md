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

## Public checkpoint

- Inverse registered through wasm, Rust SDK and harness, with coin/scoin product,
  USD filtering, inverse flag, native lot precision and TS maxTradeNum precedence.
- Independent public audit found zero leverage fallback mismatch; corrected to
  TS defaults 1/20. Documented-schema regression covers both product environments,
  two markets, wrong quote filtering, precision and unchanged book quantities.
- Public native tests: 16/16 PASS. Private/trading/query work continues separately.

## Private checkpoint

- Inverse snapshots read each collateral coin and commit positions atomically;
  preserve native amounts/filled, round display remaining/positions to 8 decimals,
  keep native notional/PnL, and value per-coin collateral into aggregate USD.
- Optional collateral columns cross the real transferable frame. Both worker
  consumers retain coin balances and inverse native extras from existing columns.
- UTA trailing fetch/read lifecycle added beside TPSL; native callback metadata
  stages until successful fresh snapshots. Classic-only inverse never promotes UTA.
- Independent review found duplicate collateral double-counting and fractional
  native-extra reconstruction; fixed with atomic duplicate rejection and decimal
  subtraction. Native private cases cover failed/late reads, two coins, snapshots,
  invalid data, native fills and shared stream/REST consistency.
- Verified engine48 tests, private account15 + inverse12 + review3 tests, Worker/wire
  26 tests, TS frame4 tests PASS. Native amend regression reserved for trading.
- TS defects explicitly listed in gate notes: inverse WS conversion/aggregate
  balance, native stop clamp and sized amend. No parity output normalization.

## Trading checkpoint

- All Classic inverse writes resolve collateral by symbol, preserve native
  quantities, clamp protective sizes against native position notional and retain
  fresh native modify client IDs. Direct and resolved amend acknowledgements both
  restore display remaining and native metadata.
- Classic trailing uses documented plan modify/cancel payloads; UTA uses strategy
  placement, distance parameters and no unsupported client ID. Amend and known
  rejection rollback retain callback settings, trigger basis and activation price.
  Ambiguous writes never retry or recreate protection.
- Independent trading review found callback drift and stale REST cache overwrite;
  corrected with staged native metadata, freshness guards and exact preservation.
  A second review found UTA activation basis loss; now parsed from native params.
- Native trading20 tests plus inverse amend regression and strict clippy PASS.
  Intentional TS fixes are listed in gate notes, including invalid callback
  preflight, documented Classic hedge side and native trailing amend/cancel.

## Queries checkpoint

- UTA income uses financial-records with streamed documented rows, bounded 30-day
  windows, cursor walking, exact IDs, product/symbol filters and atomic failure.
- Preserve raw signed amount and optional signed fee metadata. Funding, explicit
  fee and transfer enums are classified; trade remains other because docs do not
  establish gross/net PnL. No additive fee duplication or invented decomposition.
- Inverse missing fee currency resolves each symbol's base coin, including ETH.
- Independent query review clear. Existing history13 + new UTA ledger10 tests and
  strict clippy PASS; tests include byte-wise streamed decoding, late UTA signal,
  malformed lists, failed later pages, repeated cursors and nonoverlapping windows.

## Final boundary review

- Independent review found stale venue timestamps can make HTTP acknowledgements
  overwrite a later WS callback. Captured monotonic WS revisions now protect
  same-clock arrivals as well as old exchange timestamps; regression passes.
- Real Worker hydration now retains the trailing SL trigger basis. Native replay
  already exposed that field, so an adapter regression pins the production path.
- External limit/index trailing configurations remain visible/cancelable, and
  unsupported edits fail before cancel. Supported market-triggered trailing paths
  retain their exact native parameters across size edits and rollback.
- Native trailing21 + existing trading25 PASS, inverse amend direct/resolved paths
  PASS, strict clippy PASS. Flat ETH metadata test clears positions first and
  verifies ETH collateral in metadata, leverage, margin and cancellation.
