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

Maximum-size splitting copies the reference allocator exactly: it emits
`ceil(amount / max)` equal rounded lots followed by a separately rounded remainder.
It does not correct rounding loss or a remainder below the native minimum.
`legacy_lot_allocation` records all four profiles' wire quantities for 26/5/1
(`4,4,4,4,4,4,1`), 22/10/2 (`6,6,6,2`), and a below-minimum remainder
12/5/1 (`3,3,3,2`), where the inputs are amount/maximum/step.

Attached Spot and COIN-M SL/TP children each keep the whole adjusted intent
quantity. For amount 11/max 5/step 1, the primary entry is `[3,3,3,1]` (sum 10),
while SL and TP are each `[11]`. The kept entry under-allocation does not change
attached protection quantities; standalone stops retain the primary split policy.

After a definite native batch item refusal in a successful HTTP response, USD-M
and COIN-M continue later chunks and attached children, matching the reference's
per-item logging loop. Returned signature-error rows also continue without
resending the batch; whole-request signature handling retains its existing policy.
A genuine Unknown row instead stops all later writes for the action, including
later lots, chunks and attached children, and is never retried. This is the
recorded stop-after-UNKNOWN parity exception (Sean/daxx, 2026-11-08): the TypeScript
batch loop continues even for a native item timeout or uncertain rate response.
An HTTP request failure stops the serial placement path, including later algo
children. The putback-3356 brief preserves master's existing Rust protective
fallback: after a proven cancellation and a definitely refused stop replacement,
it attempts the captured original once, including its native trailing callback
rate and activation price. Unknown replacement outcomes never add a put-back.
The reference's `updateOrder` only cancels then places and propagates failure
(`binance.exchange.ts:1597-1618`); the retained fallback is explicitly master Rust
behavior, not a claim of TypeScript parity. Successful, refused and ambiguous
put-backs retain master's protection reporting and no-resend rules.

| Failure path | Legacy behavior | Reference evidence |
| --- | --- | --- |
| Split lots / chunks: USD-M, COIN-M definite native batch row refusal | Continue later writes | `binance.exchange.ts:1997`, `binanceinverse.exchange.ts:937` |
| Split lots / chunks: HTTP failure, including Spot and PM serial placement | Stop | `binance.exchange.ts:2012`, `binancespot.exchange.ts:586`, `binancepm.exchange.ts:1035` |
| Attached TP/SL: HTTP failure | Stop later children | `binance.exchange.ts:2029`, `binancepm.exchange.ts:1068` |
| Cancel-replace: replacement definitely refused after a proven protective cancel | Preserve master Rust's one exact original-stop put-back; no put-back after Unknown | TS cancel/place: `binance.exchange.ts:1617`, `binanceinverse.exchange.ts:808`, `binancespot.exchange.ts:459`, `binancepm.exchange.ts:834`; retained Rust: master `6890877e6`, `trading.rs:1388-1438` |

Placement failures include the reporting-only `WritePlan` journal. Its indexes
follow prepared step order, with one row per native batch element; a conflicting
cancel is an Applied effect. The accepted rows retain their native IDs, uncertain
rows remain Unknown and an unissued suffix remains notSent. Native batch row
attribution is kept separately from the engine's aggregate physical classification.
The journal adds no filter validation, reallocation, retries or dispatch decisions.
It also reaches engine-owned unproven HTTP seals through the existing read-only
capture hook. Venue-authored Unknown failures and warnings include exactly
`The outcome is unknown. Check Open Orders`; engine-owned seals retain their
existing canonical wording. Existing cancellation and amendment paths continue
to use their physical lifecycle journal.

| Pinned audit path | Recording-transport tests | Reference evidence |
| --- | --- | --- |
| Maximum-quantity splitting | `legacy_partial_placement::spot_split_lots_keep_the_legacy_partial_write_sequence`; `after_failure::split_lots_continue_after_a_definite_batch_item_refusal` | `binancespot.exchange.ts:535-547,576-594`; `binance.exchange.ts:1830-1844,1997-2011` |
| Batch chunks / below-minimum later order | `legacy_partial_placement::spot_native_minimum_is_not_a_new_whole_plan_gate`; `after_failure::later_batch_chunks_continue_after_a_definite_item_refusal` | `binancespot.exchange.ts:467-470,576-594`; `binanceinverse.exchange.ts:928-943` |
| Separate TP/SL children | `legacy_partial_placement::attached_algo_keeps_legacy_whole_quantity_after_split_parent`; `after_failure::children_follow_a_parent_batch_item_refusal_but_stop_after_a_child_refusal` | `binance.exchange.ts:1685-1714,1783-1816,1851-1905,2025-2040` |
| Cancel-replace | `after_failure::a_refused_stop_replacement_preserves_masters_protective_put_back`; restored `trailing_amend`, `coinm_algo`, `portfolio_margin` put-back controls; existing fill-race controls | TS cancel/place: `binance.exchange.ts:1597-1618`, `binancespot.exchange.ts:445-460`; fallback preserved from master Rust per putback-3356 |
| Unknown parity exception: split lots, chunks, children | Three `after_failure::a_native_item_timeout_stops_*` pins for USD-M and COIN-M; `request_context::a_native_batch_item_rate_limit_stops_lots_while_definite_refusals_continue` | TS continues at `binance.exchange.ts:1997-2011,1685-1714`; `binanceinverse.exchange.ts:937-941`; Rust stops by the recorded exception |

`after_failure::a_later_gateway_failure_keeps_accepted_refused_unknown_and_unissued_rows`
also pins an accepted batch followed by a mixed refused/accepted batch, then HTTP
503. The caller retains the complete 16-row journal and native accepted IDs,
including five Unknown dispatched rows and the final unissued row. The engine's
canonical unknown-after-refusal message remains unchanged.
