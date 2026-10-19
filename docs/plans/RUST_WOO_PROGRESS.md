# WOO perpetual Rust port

Scope: `woo` only. `woofi` and `kodiakfi` inherit OrderlyExchange and belong
with the Orderly family. Follow VENUE_PORT_BRIEF.md and RUST_REWRITE.md.
Branch `feat/rust-woo`, after MEXC parity PR #1311 merges.

## Phases

1. Public data: borrowed market/ticker/funding schemas, legacy stream,
   buffered timestamp-anchored full depth, candles and public reads, watchdog.
   Use current active TS protocol, not inert `woo.v3.*` migration foundations.
   Native tests and independent phase review before commit.
2. Private data: exact V1/V3 signing; account mode and balances; paged regular
   and algo snapshots; positions/settings; auth and private events; monotonic
   fills, terminal precedence and stale-read guards. Failed reads preserve
   known state. Independent review and native tests before commit.
3. Trading: typed sequenced jobs for ordinary/split/bracket/stop/trailing
   placement, native amend, parent-aware cancel, cancel-all, leverage and mode.
   Complete batch preflight before writes. Review and regression tests.
4. Queries: streamed histories, route-specific paging/time units, V1 ledger
   income and V1 per-order realized PnL; retain WOO's order-based journal fold
   in the shell. Query results never mutate live state. Review and tests.
5. Harness/gates: registrations, exact signed private tape, two-book live
   capture and committed fixture, all brief checks plus Binance regressions,
   CPU/memory/wasm budgets. Review, commit, merge latest landing branch, PR
   into feat/rust-rewrite, adversarial reviews posted, fixes/rereviews, green
   CI, merge and fresh follow-up branch.

## Venue-specific decisions and risks

- Active API is a V1/V3 hybrid. V1 signs sorted form data plus `|timestamp`;
  V3 signs timestamp + method + exact path/query + exact JSON body. Signed
  bytes must be the bytes sent. Reads retry boundedly; ambiguous writes never
  retry. Private WS signs `|timestamp`, uses applicationId in socket URL.
- Public snapshot subscription precedes HTTP. Legacy updates have timestamp,
  not V3 prevTs. Buffer boundedly, discard buffered updates at/before snapshot,
  invalidate generations on reconnect/unsubscribe, validate whole frames.
- Preserve IOC market emulation with bounded slippage/current touch, post-only
  crossing guards, hedge close side, broker IDs, positional paired protections,
  sibling preservation, absolute-distance trailing callback and full-position
  quantity. Native regular amend always sends quantity, including price-only.
- Caller client IDs must be canonical positive decimal safe-integer strings,
  <=9007199254740991, exactly one native plain order, no split/bracket/algo.
  Validate all batch legs and duplicate IDs before writes.
- TS supports cross margin; isolated mutation has no implementation. Opening
  Conditional is not an implemented TS algo type. Record capability limits
  with source evidence; do not confuse sized/full-position TP/SL with opening
  triggers.
- V3 transactionHistory PnL is absent, not zero. V1 ledger supports REALIZED_PNL
  (possibly daily account aggregates with no symbol) and other income kinds.
  Preserve aggregate scope. `/v1/orders?realized_pnl=true` supplies per-order
  PnL, fees and position side. Order request times are milliseconds; response
  created_time/updated_time are fractional seconds. Do not use hist_trades
  (recorded persistent500) or pretend ignored V3 page/fromId parameters page.
- Preserve WOO's order-based historical-position fold; inheriting the generic
  fill-based fold would lose journal rows when fill PnL is absent.
- Genuine TS repairs must be fixture-backed and listed in KNOWN_TS_FIXES and
  the PR. Never weaken gate comparisons or copy Rust output into TS.

## Preparation

Read current TS, cross-venue guidance, official legacy docs and live public
REST payloads. Public info/futures/orderbook routes work without credentials.
Independent inventory confirms the scope and protocol decisions above.
No WOO implementation started before Part 1 merge.
