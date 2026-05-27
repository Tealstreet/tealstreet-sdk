# CLAUDE.md — safe-cex/src/webworker/postProcessors

Pipeline of post-processors that runs on every throttled store
update (4 Hz default). Each one mutates the snapshot before
`postMessage` ships it to the main thread.

For the broader hot-path rule set, see `../../exchanges/CLAUDE.md`
→ "Public WebSocket Hot-Path Patterns".

## Pipeline cadence

`ExchangeWorker._throttledStoreUpdate` is throttled to 250 ms
(configurable via `tuning.dataUpdateThrottleMultiplier`). Per
firing:

1. `_exchangeDependentPostprocessors[*].process(processedData)` —
   classes that need the live `Exchange` instance (chaser, twap,
   message-handler-stats).
2. `_postprocessors[*].process(processedData)` — classes that
   only need account/options/store data (displayed-positions,
   chaser-order-enrichment, orders-risk, snapshot).
3. `_sanitizeForPostMessage(processedData)` — strips circular
   `info` fields off orders / positions / displayedPositions
   **only when at least one item carries `info`** (one-pass scan
   gates the `.map()` allocation).
4. `postMessage({ op: 'store', data: sanitizedData })`.

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

### `DisplayedPositionsPostprocessor` skips idle algo lookups

The displayed-position pass always enriches real positions, but
chaser/TWAP side maps are only useful while those algos are active.
When `activeChasers` and `activeTWAPs` are empty, don't build
`activeKey`, `activeChaserKeys`, `activeTwapKeys`, or the real-position
key map. Fake positions are only a chaser feature, so
`realPositionKeys` is intentionally allocated only when chasers exist
(not merely when TWAPs exist).

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

If a postprocessor needs the live `Exchange` instance (e.g. for
`chaserOrderManagerInstance`, `twapManager`, or to read off
`exchange.publicWebsocket`), put it in
`exchangeDependentPostprocessors`. Otherwise prefer
`independentPostprocessors` so the work can run client-side in
hosted mode.
