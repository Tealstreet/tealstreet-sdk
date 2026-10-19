# Crypto.com Rust port

Branch: `feat/rust-cryptocom`. Spec: `RUST_REWRITE.md`; process:
`VENUE_PORT_BRIEF.md` from origin/feat/rust-rewrite (2026-10-19).
Full auto; commit each phase after verification and a sub-agent review.

## Phases

1. Public data: typed venue crate; streamed nested REST lists; market/ticker
   seed and sockets; snapshot books, valuations and candles; socket watchdog.
   Verify documented payloads, bans, fmt, clippy and native tests. Review.
2. Private data: HMAC envelope signing, paced auth and subscriptions;
   streamed reconciliation with atomic order snapshots and stale-read guards;
   positions, settings, balance and execution deduplication. Review and test.
3. Trading: sequenced place/amend/cancel jobs; caller IDs preflighted across
   the whole batch; advanced stops; leverage; fail-closed unsupported margin
   switching. No retries of ambiguous writes. Review and test.
4. Queries: fills, order history, income and public trades using the existing
   JSON result contracts; candles and account validation. Review and test.
5. Integration and gates: register wasm/TS/harness, documented private tape,
   live two-book public recording and compressed fixture, TS defect register,
   Crypto.com and Binance correctness/performance gates, repository checks.
   Review and fix, then merge origin/feat/rust-rewrite into lane, push, PR to
   feat/rust-rewrite, adversarial review/comment/fix loop, merge PR and cut a
   follow-up branch. Never modify master or force/rebase.

## Venue-specific audit

- Public lists are under `result.data`; private reads are signed POSTs.
  Streaming must support both a nested array and a request body, and validate
  the success envelope even when its code follows the array.
- Default `book.<symbol>.50` is a complete snapshot: replace both sides.
- Both sockets answer `public/heartbeat` with the same id; silent sockets
  reconnect after 90 seconds; private auth waits one second after opening.
- Current `funding` differs from `estimatedfunding`; index topics use the
  instrument's underlying index; optional subscriptions respect the cap.
- TS has no parent bracket derivation; unsupported shapes must not silently
  lose caller IDs or risk controls.
- Failed reads must retain good state. REST must not overwrite newer stream
  events or terminal orders. Only executions emit fills, once per trade id.
- Fresh payload tests derive from official Exchange v1 documentation and the
  repository's documented runtime contracts, never deleted legacy fixtures.

## Progress

- Initial inspection and plan audit complete. No implementation yet.

### Public phase

Implemented typed market/ticker seed, valuations, default snapshot books,
candles, subscriptions and 90-second watchdog. Shared streaming supports nested
arrays and signed-body reads without changing existing callers. Review found
malformed list/UTF-8 acceptance; fixed with streamed syntax validation and
regressions. Checks: bans PASS; crate clippy PASS; engine 10 tests and venue 6
tests PASS. Public phase ready; private phase next.

### Private phase

Moved crate to `sdk-rs/venues/cryptocom` after merging origin's nested workspace
(`fe79b018cd`). Signed POST reads stream `result.data`; auth waits one second,
then subscribes to order/advanced-order/positions/balance plus `user.trade`.
Snapshots merge around newer events; failed reads retain state. Settings carry
leverage/margin without zero positions; terminal orders and position closures
have independent stale-event fences. Executions retain full-width ids and emit
once per trade id (bounded reconnect deduplication cache).

Sub-agent found incorrect position-id separator, lost closure timestamp fence,
and symbol-scoped snapshot fencing that dropped the other hedged side. Fixed
all with regressions. Bans/clippy PASS; 15 venue tests PASS (including signer).
TS defects intentionally corrected: failed reads previously committed empty;
order mappers lost `client_oid`; `user.trade` was never subscribed/emitted;
REST reset live leverage/liquidation enrichment; terminal/stale state could
reopen exposure. Register in harness KNOWN_TS_FIXES during gate phase.

### Trading phase

Preflighted typed jobs run sequentially with 20 ms pacing. Caller ids are sent
verbatim; duplicates and unsupported shapes reject the entire batch before any
write. Validation uses a conservative 36-byte ceiling (also rejects line
terminators) rather than introducing JS string-length emulation. Native amend
preserves caller ids. Regular/advanced cancel routing, best-effort batch cancel,
cancel-all, fixed hedged mode and fail-closed margin switching are implemented.

Official API audit found TS sends an invalid per-symbol/string account-leverage
request; Rust reads `account_id`, sends integer leverage, and updates global
settings. TS silently ignores attached brackets/opening triggers; Rust rejects
them. TS sends POST_ONLY as time_in_force; Rust sends the execution instruction
with GOOD_TILL_CANCEL and combines REDUCE_ONLY. Sub-agent found false HTTP
success, post-only wire shape, missing-id cancel abortion and Unicode length
mismatch; fixes and regressions added. Bans/clippy PASS; 25 venue tests PASS.

Next: history queries, exact nanosecond pagination, then wasm/harness gates.

### Queries phase

Added streamed fills/order history/transaction ledger/public trade reads in
Binance JSON shapes. Private paging uses exact u64 nanosecond boundaries,
overlaps and deduplicates boundary rows, walks short pages, and rolls back
partial pages before retrying. Ranged income walks the whole range without a
100-row total cap. Missing exact cursors fail closed when another page is
needed. Historical limit prices remain separate from trigger prices; absent
requested/average prices are omitted. SESSION_SETTLE uses the documented
realized_pnl field (official institutional get-transactions example).

Sub-agent found the income total cap, dead cursor-precision guard, trigger vs
limit history price and settlement-PnL mapping. Fixed all; regressions added.
Bans/clippy PASS; 33 venue tests PASS. Next: registration, tapes, cleanup, gates.
