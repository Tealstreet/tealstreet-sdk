# CLAUDE.md — safe-cex/src/webworker/postProcessors

Pipeline of post-processors that runs on every throttled store
update (4 Hz default). Each one mutates the snapshot before
`postMessage` ships it to the main thread.

**Hosts add their own independent post-processors** through
`registerIndependentPostprocessor` (`registry.ts`); they are appended, so the
built-in order holds. Tealstreet's account snapshot upload is one, and it lives
in `apps/web/src/utils/snapshot/` because it posts to Tealstreet routes — this
package ships in the OSS mirror and must not.

For the broader hot-path rule set, see `../../exchanges/CLAUDE.md`
→ "Public WebSocket Hot-Path Patterns".

## Layer boundary contract — what belongs here vs. core vs. UI

This package sits between two other layers. Use this to decide where a
new computation lives. More specific rules elsewhere win.

- **safe-cex core** (`../../exchanges/*`, the store in `store.base.ts`)
  — exchange-specific normalization, signing, and anything that needs
  exchange API/WS semantics or per-exchange tier data. Owns the **raw**
  `StoreData` fields: `balance`, `positions`, `orders`, `tickers`,
  `markets`, `fetchedLeverage`, and exchange-reported values like
  `liquidationPrice` / `unrealizedPnl`.
- **post-processor** (this dir) — pure, exchange-agnostic derivations
  over the canonical store + account context, plus stateful
  time-windowed bookkeeping (rolling PnL). Writes the **derived**
  `StoreData` fields: `displayedPositions`, `displayedMargin`, `ordersRisk`,
  `messageHandlerStats`. (`activeChasers`/`activeTWAPs` come from the
  engine's Rust algos, not a post-processor.) Output **must
  be identical for every consumer** — no dependence on user display
  settings.
- **UI render** (`apps/web/src/components`, atoms, hooks) —
  view-dependent presentation only: USD-vs-% toggles, formatting,
  privacy masking, sorting, per-user-setting thresholds, and anything
  sourced outside the store.

**The hoist test** — move work into a post-processor only if **all
three** hold:

1. Output is identical for every consumer (independent of user view
   settings).
2. It does non-trivial or shared work — not a single
   division/subtraction the UI can do per visible row.
3. It needs only the store + account context (no live `Exchange`
   instance → it can be an _independent_ processor and run client-side
   in hosted mode; otherwise it's an exchange-dependent one).

Worked examples, applied to the Open Positions derivations:

| Derivation                                     | #1 same for all?              | #2 non-trivial?      | Verdict                                                          |
| ---------------------------------------------- | ----------------------------- | -------------------- | ---------------------------------------------------------------- |
| SL/TP coverage (`hasFullStop`/`hasFullTP`)     | yes                           | yes (order scan)     | **hoisted** here                                                 |
| `accountLeverage` (`notional / balance.total`) | yes                           | no (one division)    | stays in UI                                                      |
| 1m/5m PnL delta (`uPnL − baseline`)            | yes                           | no (one subtraction) | stays in UI                                                      |
| max-drawdown tag                               | no (`maxAccountDrawdownAtom`) | —                    | stays in UI                                                      |
| realized PnL / breakeven                       | —                             | —                    | stays in UI (sourced from the history controller, not the store) |

> **Known boundary blur (not yet fixed):**
> `apps/web/src/components/riskProfiler/riskProfiler.calculations.ts`
> recomputes order-risk scenarios on the main thread that
> `OrdersRiskProfilePostprocessor` already wrote to `store.ordersRisk`.
> That one is substantial (a full risk recompute), so it's a candidate
> for consuming the derived field instead — left as a follow-up.

## Pipeline cadence

`ExchangeWorker._throttledStoreUpdate` is throttled to 250 ms
(configurable via `tuning.dataUpdateThrottleMultiplier`). Per
firing:

1. `_exchangeDependentPostprocessors[*].process(processedData)` —
   classes that need the live `Exchange` instance (message-handler-stats).
2. `_postprocessors[*].process(processedData)` — classes that
   only need account/options/store data (displayed-positions,
   displayed-margin, chaser-order-enrichment, orders-risk, snapshot).
3. `_prepareStorePayload(processedData)` — applies updateId-gated
   order/position omission, packed ticker deltas, and market-version gating.
4. `_sanitizeForPostMessage(processedData)` (inside preparation) — strips circular
   `info` fields off orders / positions / displayedPositions
   **only when at least one item carries `info`** (one-pass scan
   gates the `.map()` allocation).
5. `postMessage({ op: 'store', data: preparedData })`.

The receiver structuredClones at the boundary, so any deep clone
inside the post-processors is **redundant** with that work — see
the `MessageHandlerStatsPostprocessor` rewrite for an example
(was running `clone(websocket.messageHandlersStats)` per WS-type
per pass; now aliases the live ref because the synchronous window
between assignment and `postMessage` cannot be interrupted by a
WS frame).

## Hot-path rules

### Pre-build symbol-keyed maps once per call

`DisplayedPositionsPostprocessor` and `OrdersRiskProfilePostprocessor`
both need `tickerBySymbol` / `marketBySymbol`. Build them via
indexed for-loop into a plain `Record<string, T>`:

```ts
const tickerMap: Record<string, Ticker> = {};
for (let i = 0; i < tickers.length; i++) tickerMap[tickers[i].symbol] = tickers[i];
```

**Don't** use `new Map(arr.map(t => [t.symbol, t]))` — that
allocates an N-element tuple array, N two-element arrays, and a
Map per call. The for-loop is one allocation.

### Cache symbol maps across passes via reference equality

The store mutates ticker/market fields **in place** per WS frame
but only swaps the array reference on full snapshot replacement
(initial fetch, `markets:changed`, etc.). When the source array
reference is unchanged, the cached map is still correct because
its values are the same ticker objects. Cache on the postprocessor
instance:

```ts
private _cachedTickerMap: Record<string, Ticker> = {};
private _cachedTickerSource: Ticker[] | undefined;

// in process():
if (this._cachedTickerSource !== store.tickers) {
  // rebuild
  this._cachedTickerSource = store.tickers;
}
```

See `DisplayedPositionsPostprocessor.process` and
`OrdersRiskProfilePostprocessor.process` for canonical shapes.

### One-pass orderId → owning-chaser map

`ChaserOrderEnrichmentPostprocessor` was building a `Set` of
chaser-owned orderIds for O(1) membership, then re-scanning
`activeChasers` with `find(c => c.currentOrderIds?.includes(...))`
per matched order. Now: single pass over chasers builds an
`orderId → ActiveChaser` map; per matched order is O(1).

Frame mode sends native orders in binary frames and `activeChasers` in derived
store payloads. The main exchange reuses `chaserOrderUpdates` after the first
frame, so either arrival order restores the same chaser tags without forwarding
another orders array. It commits changed tags through `addOrUpdateOrders`,
with a reentrancy guard, so update-ID observers see derived-only tag changes.
Unchanged metadata and ticker-only notifications do not mutate orders. Fresh
native snapshots must not erase tags while the matching chase remains active.

### `DisplayedPositionsPostprocessor` skips idle algo lookups

The displayed-position pass always enriches real positions, but
chaser/TWAP side maps are only useful while those algos are active.
When `activeChasers` and `activeTWAPs` are empty, don't build
`activeChaserKeys`, `activeTwapKeys`, or the real-position key map.
Fake positions are only a chaser feature, so `realPositionKeys` is
intentionally allocated only when chasers exist (not merely when TWAPs
exist). Only positive-contract rows are registered as real positions: full
snapshot refreshes retain zero-contract placeholders, and those must not hide
the pending-entry fake position for an active chaser. The per-position
`activeKey` string is built when
`needsPositionKey` **or** there is stop/TP coverage to look up
(`hasStopTpCoverage`) — it's reused for the chaser/TWAP flags and the
`hasFullStop` / `hasFullTP` lookup, so it stays a single allocation per
position even though it now serves two features.

Rolling 1-minute and 5-minute PnL queues are retained only for positions (real
or fake) emitted by the current pass. When a full snapshot leaves a zeroed row
or removes a symbol, both queue maps evict that account/position key so symbol
rotation cannot grow worker memory for the lifetime of the tab.

### Last-price UPnL on inverse markets uses USD contracts, not `contracts`

When `calculateUnrealizedPnl` recomputes from `ticker.last` (Bybit forces it via
`alwaysCalculateUnrealizedPnlFromLastPrice`; others when the user opts in or the venue
reports 0), an inverse position is `extra.contractsNative × contractSize × move / entry`.
`contracts` is coin converted at MARK, so `move × contracts` is off by entry/mark
(BTCUSD showed $19.30 against the venue's $23.94). OKX counts `contractSize`-dollar
contracts; Bybit inverse leaves `contractSize` unset ($1).

### `DisplayedMarginPostprocessor` derives `store.displayedMargin`

`DisplayedMarginPostprocessor` (independent, registered **after**
`DisplayedPositionsPostprocessor` because it reads
`store.displayedPositions`) aggregates per-account exposure into
`store.displayedMargin` = `{ longExposure, shortExposure, totalNotional }`.
This is the margin analog of `displayedPositions`: a worker-derived
internal layer the margin UI reads instead of looping positions on the
render path. It's a single order-preserving pass using the exact formula
the UI used inline (`Math.abs(notional || 0)` summed in array order, per
side and overall), so the sums are floating-point identical to the old
filter+reduce — locked by an oracle-parity test in
`DisplayedMarginPostprocessor.test.ts`.

It mirrors `displayedPositions` at every bridge point: defaulted in
`defaultStore` / `store.stub`, exposed via the `Store` interface getter,
in `EXCLUDED_FIELDS` (server delta) so hosted mode recomputes it
client-side, normalized in `ExchangeServerWorker`, and stripped with
`displayedPositions` during the cache-hydration fence. It is **not**
gated by an optimistic `_lastSent*` id, so it does **not** need the VWC
pre-gate exemption that orders/positions require. Only the pure exposure
aggregation lives here; the margin UI keeps view-dependent work
(effective-leverage `isolated ? 0` rule, uPnL%, currency slicing,
cross-account combination, ticker/funding for the active symbol).

### `hasFullStop` / `hasFullTP` are derived here, not in the UI

`buildStopTpCoverage(store.orders)` maps each `symbol:closingSide` to
its open stop/take-profit coverage (a Sell order closes a Long, a Buy
closes a Short; `amount === 0` is the close-entire-position marker →
full coverage). Each displayed position then carries `hasFullStop` /
`hasFullTP` = full marker **or** summed amounts `≥ contracts`. This is
order-derived, consumer-agnostic state, so it belongs on the data
object — the Open Positions status column reads the booleans off the
row with no order scan. The **max-drawdown** tag stays in the UI
instead: it depends on the per-user `maxAccountDrawdownAtom` and needs
the raw stop orders for worst-case-loss math.

Keep this work in the worker unless a later profile proves otherwise.
The UI consumes `displayedPositions` with ticker/market/balance fields
already attached; moving the postprocessor to the main thread would
trade a small worker cost for more foreground render-side work.

### `calculateOrdersRisk` accepts pre-built maps

`risk.ts` exports `buildOrdersRiskMaps(markets, tickers)` so the
postprocessor can cache the `(tickerBySymbol, marketBySymbol)`
pair across passes via source-array reference equality, then pass
it through `calculateOrdersRisk(..., precomputedMaps)`. The
function falls back to building them itself when the optional
arg is omitted (tests, ad-hoc callers).

### Don't deep-clone payloads before `postMessage`

`postMessage` already runs `structuredClone` at the worker → main
boundary. Any deep `clone()` inside a postprocessor duplicates
that work. The synchronous window between your assignment and the
postMessage call cannot be interrupted by a WS frame, so aliasing
the live reference is safe — the receiver still sees a snapshot
at send time.

The `clone(websocket.messageHandlersStats)` per WS-type per pass
was the canonical example (~2.4K array allocations/sec on Bybit
linear). Now aliases.

## Per-exchange-vs-independent postprocessors

`registry.ts` splits processors into two lists:

| List                              | Constructor args                                         | Runs in                             |
| --------------------------------- | -------------------------------------------------------- | ----------------------------------- |
| `independentPostprocessors`       | `(accountName, exchangeName, options)`                   | Worker side OR client-side (hosted) |
| `exchangeDependentPostprocessors` | `(accountName, exchangeName, options, exchangeInstance)` | Server side (hosted) OR worker side |

If a postprocessor needs the live `Exchange` instance (e.g. to read off
`exchange.publicWebsocket`), put it in
`exchangeDependentPostprocessors`. Otherwise prefer
`independentPostprocessors` so the work can run client-side in
hosted mode.
