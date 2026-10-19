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

## Public phase

- Implemented borrowed REST seeds, independent ticker topics, deferred funding
  refresh, buffered full depth with timestamp anchor and generations, candle
  reads/streams, subscription reference counts and ping/watchdog recovery.
- Sub-agent review found four defects: delayed snapshots reviving invalidated
  depth, non-ASCII interval panic, early funding request, and premature price-cap
  publication. Fixed all four, with regressions; bounded rereview PASS.
- 15 public tests, native clippy and formatting PASS. Includes 1,501 depth
  levels, bounded buffering, pre-seed subscriptions, stale snapshots, control
  precedence and candle output decoding. Registration remains for gate phase.

## Private, trading and query phases

The three state machines were prepared in disjoint files and integrated
together, so their shared hooks compile without unused placeholder code.
Each received its own independent phase review before this checkpoint.

- Private: staged account/positions/paged ordinary+algo reads, exact V1/V3
  signing and WS auth, hedge settings, monotonic lifecycle/fills, per-execution
  dedup, bounded signature-aware position reconciliation, metadata expiry and
  bounded tombstones. Review fixed failed-page atomicity, equity/uPNL races,
  stale config in positions, object algo children, cache expiry and lifetime
  growth. STOP kind follows closing side/source; buy trailing uses plus distance.
  Independent bounded rereview PASS.
- Trading: complete batch preflight, safe numeric caller IDs, exact splitting,
  protected IOC touch reads, paired/sized/trailing protections, native quantity
  amendments, parent-aware cancel, both-family cancel-all and merged symbol
  cancellation. Config intent guards and authoritative mode/leverage readback.
  Review fixed rounded-zero amendments and deferred trailing replacement sizing.
  Independent bounded rereview PASS.
- Queries: streamed V3 fills and V1 legacy fallback with distinct pagers,
  V1 orders requesting realized_pnl, and paced V1 ledger with aggregate scope.
  Bounded income ranges walk beyond 500 by default. Native queries never write
  the store. Review fixed absent/zero updated_time; rereview PASS.
- WOO-only order-based journal folding is retained in the SDK shell through a
  shared pure mapper. `woofi` and `kodiakfi` remain with the Orderly lane, as
  explicitly confirmed by the overseer.

## Harness work in progress

- Public live capture: 20 seconds, two books. Correctness matches after handling
  a live null index as an absent frame value, rather than rejecting the seed.
- CPU initially failed on the small recording. Numeric visitors avoid generic
  untagged decoding, native ticker indexes avoid repeated symbol allocation and
  lookup, and WOO retains level-3 optimization inside a level-z aggregate.
  Initial reruns were marginal (63.2 ms TS / 64.8 ms Rust overall;
  32.4 / 32.6 ms steady). The next optimization removes redundant REST decoding
  and preallocates atomic WS batches. Acceptance is pending an integrated rerun.
- Eight-venue wasm was 2,741,039 bytes at level-s. Level-z aggregate plus WOO
  level-3 is 2,605,267 bytes, below the unchanged 2,621,440-byte budget.
- Private fixture is being reconciled against actual command stages. Exact
  request bodies/signatures, required write consumption, auth and subscriptions
  remain enforced. TS reliability repairs and final replay gates are pending.

## Integration checkpoint

- Integrated landing `b265a0bab0` (Backpack and BloFin). Preserve all protocol
  guards in the new shared `tape.mjs`, including WOO connection resets and form
  handling, and the generic global-cancel intent bridge. Integrated harness:
  78 tests PASS. Landing now owns a 3 MiB budget; no budget increase by this lane.
- Shared `maxSlippagePct` is optional; every explicit order constructor, including
  new landing venues, defaults to None. No behavior change for those venues.
- Crypto cleanup: deleted unused HMAC-SHA512. Retained SHA256 after landing
  introduced a Backpack caller; no unused WOO replacement helpers retained.
- WOO native phase coverage: 2 signing + 18 public + 26 account + 22 trading +
  12 query tests. Public batch regressions retain atomic rejection and work past
  the allocation capacity hint. Complete workspace checks are running.
- Independent gate audit fixed sorted V1 parameter validation and server-ping
  replies. Final fixture: 129 records, 18 commands, 22 seconds, 53 replay frames.
  Includes opening entry with attached SL+TP, sized STOP inference, positional
  sibling preservation, trailing, mode/leverage, and both V1 PnL/income axes.
- Independent SDK audit fixed order-axis source provenance, implicit public-only
  metadata reads, and JSON-aware exact integer decoding that preserves quoted
  venue error messages. The original regex helper corrupts numbers inside text.
- Root typecheck 44 tasks and lint 28 tasks passed before integration. Full tests
  and the integrated release artifact are running; final acceptance remains open.
