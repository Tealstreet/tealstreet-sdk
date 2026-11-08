# Binance portfolio-margin best quotes

PM uses USD-M public market data. Its store ticker's bid and ask come from
`<symbol>@bookTicker` on the public socket, for every discovered market. These
subscriptions work without private credentials or an order-book subscription,
replay after reconnects, and expand when discovery adds markets.

Binance documents the all-market `!bookTicker` stream at a five-second cadence
and individual symbol book tickers as real time. Using the all-market stream
can leave ticker quotes behind the independently streamed depth book during a
price move. See [Binance's public stream documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/ws-streams/public).

REST `ticker/bookTicker` still seeds startup prices. Daily ticker and mark-price
frames update their own fields without overwriting PM bid/ask. The fixtures in
`tests/pm_ticker_bidask.rs` model a 0.1% move between all-market samples and
assert that the ticker and depth book agree on the venue's best prices.

The PM replay adapter in `sdk-rs/harness/binance-protocol.mjs` derives the same
real-time subscriptions from the TypeScript reference's own market catalogue.
Every per-symbol topic remains visible to strict replay comparison.

# Legacy TypeScript placement policy

Placement retains the existing venue behavior. Formatting a batch before I/O
does not validate every lot against native minimum quantities or notionals.
A venue refusal can therefore follow an accepted order. USD-M/PM attached algo
orders retain their whole intent quantity, and derivatives can continue later batches
after a definite per-row refusal. These are known legacy partial-placement paths.

| Audit path | Legacy reference evidence | Verdict |
| --- | --- | --- |
| Maximum-quantity splitting | `binancespot.exchange.ts:535`, `binance.exchange.ts:1830`, `binanceinverse.exchange.ts:915`, `binancepm.exchange.ts:902` | TS-partial |
| Batch chunks | `binance.exchange.ts:1970`, `binanceinverse.exchange.ts:928`, `binancespot.exchange.ts:576`, `binancepm.exchange.ts:1005` | TS-partial |
| Separate TP/SL children | `binance.exchange.ts:1851`, `binanceinverse.exchange.ts:887`, `binancespot.exchange.ts:550`, `binancepm.exchange.ts:916` | TS-partial |
| Cancel-replace | `binance.exchange.ts:1597`, `binanceinverse.exchange.ts:794`, `binancespot.exchange.ts:445`, `binancepm.exchange.ts:817` | TS-partial |

Reference files live under `packages/safe-cex/src/__tests__/reference/binance/`.
The PM Algo Service migration at `274a20d5e` changes its conditional route while
preserving its allocation and regular-before-conditional dispatch policy.
Native lot rebalancing, whole-plan filter refusals and new fail-stop rules are
outside this parity change. Engine-owned transport classification remains in
place. Binance does not adopt `WritePlan` as a dispatch gate.

After a native batch item error in a successful HTTP response, USD-M and COIN-M continue later
chunks and attached children, matching the reference's per-item logging loop.
This includes an item timeout; its native outcome remains Unknown, and that row
is never resent. Returned signature-error rows also continue without resending
the batch; whole-request signature handling retains its existing policy.
An HTTP request failure stops the serial placement path, including later algo
children. After cancellation succeeds but its replacement is refused, the
command ends without an automatic protective put-back: the reference's
`updateOrder` propagates the placement error. Snapshot callback-rate caches used
only for that put-back have been removed. The requested replacement still uses
the venue's existing trailing-rate validation.

| Failure path | Legacy behavior | Reference evidence |
| --- | --- | --- |
| Split lots / chunks: USD-M, COIN-M native batch row error | Continue later writes | `binance.exchange.ts:1997`, `binanceinverse.exchange.ts:937` |
| Split lots / chunks: HTTP failure, including Spot and PM serial placement | Stop | `binance.exchange.ts:2012`, `binancespot.exchange.ts:586`, `binancepm.exchange.ts:1035` |
| Attached TP/SL: HTTP failure | Stop later children | `binance.exchange.ts:2029`, `binancepm.exchange.ts:1068` |
| Cancel-replace: cancel or replacement failure | Stop; no automatic put-back | `binance.exchange.ts:1617`, `binanceinverse.exchange.ts:808`, `binancespot.exchange.ts:459`, `binancepm.exchange.ts:834` |
