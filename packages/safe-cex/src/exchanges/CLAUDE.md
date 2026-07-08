# CLAUDE.md — Exchange Integrations

Shared mechanics for the exchange integrations under this directory. This doc
is the single source of truth for boilerplate that would otherwise be repeated
in every per-exchange `CLAUDE.md`. **Read this first** before diving into any
individual exchange doc.

For per-exchange specifics (endpoints, quirks, auth signing details), see
the `CLAUDE.md` inside each exchange's directory.

For proxy/tunnel architecture (CORS proxy, HTTP-over-WebSocket adapter), see
`packages/safe-cex/CLAUDE.md`.

## Directory Layout

```
packages/safe-cex/src/exchanges/
├── base.ts                     # BaseExchange — every exchange extends this
├── base.ws.ts                  # BaseWebSocket — every WS class extends this
├── external-public-data.ts     # External public-data provider (multiplexing)
├── in-process-public-data.ts   # In-process public-data sharing
├── registry.ts                 # Exchange name → class mapping
├── types.ts                    # Shared types
├── __tests__/                  # Cross-exchange tests
└── <exchange>/                 # One directory per exchange
```

## Per-Exchange File Convention

Every exchange follows the same file layout. When adding a new exchange or
reading an existing one, expect these files:

| File                          | Purpose                                                 |
| ----------------------------- | ------------------------------------------------------- |
| `<name>.api.ts`               | REST client — axios setup, signing, endpoint helpers    |
| `<name>.exchange.ts`          | Exchange class extending `BaseExchange` — orchestration |
| `<name>.types.ts`             | Type defs, endpoint constants, enum mappings            |
| `<name>.ws-public.ts`         | Public WebSocket — tickers, orderbook, trades, klines   |
| `<name>.ws-private.ts`        | Private WebSocket — orders, fills, balance, positions   |
| `CLAUDE.md`                   | Per-exchange doc (see template below)                   |
| `.apikey.json` _(gitignored)_ | Local dev API keys — never commit                       |
| `.gitignore`                  | Ignores `.apikey.json`                                  |

Variants (spot, inverse, USDC, demo, PM) typically live in the same
directory under different file prefixes (e.g. `binancespot.api.ts`,
`binancepm.exchange.ts`) and are exposed as separate `exchangeName`s.

## BaseExchange Contract

`base.ts` defines the `Exchange` interface and `BaseExchange` class. Every
exchange class extends `BaseExchange` and overrides the methods it needs.
Methods left as the base implementation throw `'Not implemented'` and will
fail at runtime if called.

Key methods exchanges typically override:

- `start()` — bootstraps the exchange (markets → tickers → WS → private
  state). Default throws.
- `fetchMarkets()`, `fetchTickers()`
- `fetchPositions()`, `fetchBalance()`, `fetchOrders()`
- `placeOrder()`, `updateOrder()`, `cancelOrder()`, `cancelOrders()`
- `setLeverage()`, `changePositionMode()`, `changeMarginType()`
- `fetchOHLCV()`, `fetchExecutions()`

Check `Exchange` interface in `base.ts` for the full surface.

### Common Startup Sequence

Most exchange `start()` implementations follow this shape:

1. `fetchMarkets()` — REST call(s) to load symbols, filters, leverage tiers.
   Set `loaded.markets = true`.
2. Disposal check — if `disposed` after any await, return.
3. `fetchTickers()` — REST call(s) for 24h stats, book ticker, mark/index
   prices. Merge into `Ticker[]`. Set `loaded.tickers = true`.
4. `publicWebsocket.connectAndSubscribe()` — non-blocking.
5. Branch on `publicDataOnly` flag:
   - **Public-data-only:** `_startPublicDataOnly()` — empty
     orders/positions/balance, mark all loaded.
   - **Full:** `_startPrivate()` — listen-key fetch, private WS connect,
     `tick()` loop, `fetchOrders()` / `fetchPositions()`.

Per-exchange variation lives in REST endpoint shape, signing, and how
positions/balance get reconciled — not in the high-level sequence.

### Logging

`BaseExchange.log()` is the standard logger:

```typescript
this.log('Markets loaded', LogSeverity.Info);
this.log('Listen key refresh failed', LogSeverity.Warning);
this.log(`Order rejected: ${err}`, LogSeverity.Error);
```

- Signature: `log(message: string, severity: LogSeverity = LogSeverity.Info)`
  defined in `BaseExchange` (`base.ts`).
- `LogSeverity` is exported from `packages/safe-cex/src/types.ts`. Values:
  `Info`, `Warning`, `Error`, `Debug`.
- Always pass severity explicitly for warnings and errors — the default is
  `Info`.
- The default `Info` is what `console.log`s show in app logs and what the
  emitter forwards to subscribers.

### Pre-flight validation throws must emit first

**Any `throw new Error(msg)` inside an exchange method must be preceded by
`this.emitAndLogError(msg)`** (or the equivalent `this.emitter.emit('error', msg)`).
A raw throw bypasses `BaseExchange.emitter`, so
`exchangeAccountController.on('error')` never fires, and the user-facing
toast pipeline silently drops the error — the caller's `try/catch` and the
React onClick promise rejection are the only paths left, and most callers
don't surface those (they re-throw into an unhandled rejection).

Canonical shape used across binance/bybit/okx/lighter/blofin/bitget:

```ts
if (!position) {
  const msg = `No ${positionSide} position open for ${opts.symbol} - nothing to reduce`;
  this.emitAndLogError(msg);
  throw new Error(msg);
}
```

Lighter's `assertReduceOnlyLimitHasPosition` (`lighter.exchange.ts`, near
the `buildNoPositionReduceOnlyMessage` helper) is the direct analog for
reduce-only-no-position validation; mirror that shape when adding similar
pre-flight checks.

**Don't wrap `placeOrder` / `placeOrders` in an outer try/catch that
calls `emitAndLogError`** — inner catches around `xhr.post` already emit,
so a wrapping catch would double-toast every real API error. Emit at the
point of failure, not at the wrapper.

Past regression: Bitunix's `formatCreateOrder` raw-threw "nothing to
reduce" and "Missing positionId" for ~6 months. Users on Bitunix who
clicked Limit Buy with Reduce-Only enabled on a flat account saw an
unhandled-promise red error in DevTools, an `[CreateOrder]` structured
log, and **no toast**. The Ento bug report had this exact silent-fail
loop firing 11×.

### Reduce-only order downsizing — use the metadata flag, never hand-roll

When a reduce-only order is placed larger than the open position, it must
be capped to the position size. **The canonical mechanism is the
`autoDownSizeReduceOrders` metadata flag**, which runs the shared
`apps/web/src/utils/order/downSizeReduceOrder.ts` util inside the web order
builders (`apps/web/src/order/createLimitOrder.ts` /
`apps/web/src/order/createMarketOrder.ts`) _before_ the amount reaches
`placeOrder`. The util also handles **hedge-mode side matching** and
**orders-ahead accounting** (it subtracts already-queued reduce orders at a
better price) — a bare `Math.min(amount, contracts)` misses both.

**Scope boundary:** the flag is wired into the **limit and market** builders
only. The stop/conditional builders (`createStopLossOrder.ts` /
`createConditionalOrder.ts`) do **not** apply it — TP/SL reduce-only oversize
is a pre-existing cross-exchange gap (see the
`createStopLossOrder.ts` "fat finger and oversize protection" TODO) that
affects every flag-based exchange uniformly, not a per-exchange concern.
Don't close it by re-adding a per-exchange `formatCreateOrder` clamp; if you
extend `downSizeReduceOrder` to stops, note its orders-ahead logic was
written for price-laddered _limit_ reduce orders and needs review for the
trigger-order path first.

Each exchange does exactly one of:

- **Set `autoDownSizeReduceOrders: true`** if it does _not_ auto-cap
  reduce-only orders server-side (woo, bitrue, bitget, bitunix, weex, grvt).
- **Leave it `false`** (the default) if the exchange auto-caps reduce-only
  orders itself (bybit, okx, binance, …) — the order comes back already
  clamped, so no client-side downsizing is needed.

**Do NOT hand-roll a reduce-only clamp in `formatCreateOrder` / `placeOrder`.**
Bitrue carried a `Math.min(opts.amount, foundPosition.contracts)` there for
~2 years; it was the lone exception, blind to hedge mode and orders-ahead,
and was removed in favor of the flag. The downsize lives at the web-app
layer for every flag-based exchange — keep it there.

### Disposal Pattern

Long-running async work in `start()` and timer callbacks must check
`this.disposed` after each `await`, because the exchange may be torn down
mid-startup:

```typescript
const markets = await this.api.fetchMarkets();
if (this.disposed) return;
this.store.update({ markets });
```

This avoids late writes to a disposed store and unhandled promise
rejections during account swaps.

**Reconnect + private-snapshot paths must also honor disposal — not just
`start()`.** The tick loop already guards its re-arm on `isDisposed`, but two
other paths re-sign requests independently of the tick and previously ignored
disposal:

- `BaseWebSocket.send()` — a queued auth/subscribe/ping after dispose hit the
  "not OPEN → `reconnect()`" branch, which re-armed the
  `ReconnectingWebSocket` (`_shouldReconnect=true`) and resurrected a socket
  whose `onOpen` re-authenticated forever. `send()` now bails when
  `this.parent.isDisposed`, and `dispose()` sets `forceClosed=true` so the
  native reconnect path bails too.
- `BaseExchange.fetchAndSetPrivateSnapshot()` — runs on every private-WS
  (re)connect via the housekeep listener with `tries=10`. On a disposed
  account (browser gone / remote signer dead) it would retry `fetchOrders()`
  10×, each signing a REST request that throws "Remote signer unavailable" —
  flooding logs/CPU indefinitely. It now checks `isDisposed` at entry, each
  loop iteration, and after the `await`.

This was a real exchange-server incident: disposed accounts looped
"Remote signer unavailable" ~10×/s for 15+ min after disconnect (9k+ errors),
contributing to 9-wide e2e "stuck" load failures via signer/CPU contention.
**If you add another reconnect-triggered REST/auth path, gate it on
`isDisposed` the same way** — a disposed account must never sign again.

### Reconnect caps are streak-scoped, not lifetime

The `> 100` guards on `BaseWebSocket.onCloseCount` / `reconnectCount` (mirrored
in the Lighter ws classes and `external-public-data.ts`) bound a single
**unhealthy streak**: they reset once the connection is confirmed healthy again
(base: the first-message branch of `trackLastReceived`; each mirror: its own
healthy-traffic point). Don't turn them back into lifetime counters — long-lived
sessions (flaky networks, 24h WS session limits) legitimately accumulate 100+
closes, and a lifetime cap silently kills the socket forever.

### virtualClock

Don't use `Date.now()` for request timestamps. Use
`virtualClock.getCurrentTimeMs()` (imported from
`packages/safe-cex/src/utils/virtualClock.ts`) — it accounts for clock drift
between the client and the exchange server, which is critical for HMAC
signatures with strict `recvWindow` enforcement.

### Big-Integer JSON Parsing

For exchanges with large numeric IDs (Binance, Bybit, etc.), parse responses
with `bigIntAsStringJsonParse` (from `@tealstreet/json-parse`) to avoid JS
number precision loss. Configure on the axios instance via the
`transformResponse` option.

## Public WebSocket Hot-Path Patterns

The `<name>.ws-public.ts` file in every exchange runs at the highest message
volume in the entire app — orderbook deltas at 30–60 Hz/symbol, ticker
arrays once/sec for hundreds of symbols, trades at hundreds/sec on busy
markets. Hot-path code here directly determines main-thread idle budget and
React render smoothness. The patterns below are load-bearing across every
public-WS implementation; each one was added after the equivalent
unoptimized form was found and removed in a real exchange. Audit these
whenever you touch a `.ws-public.ts` file.

### Topic dispatch — extract once, look up in a Map

**Don't:** iterate `Object.entries(this.messageHandlers)` per WS frame and
substring-scan the raw message body for each registered topic
(`data.includes(\`topic":"${topic}\`)`). With many per-symbol subscriptions,
that's N substring scans per frame over multi-KB payloads.

**Do:** extract the routing field once via targeted `indexOf`, then dispatch
via direct Map lookup. Bybit/BloFin do this with the channel + instId
fields; Binance does it with `e` + `s`; Hyperliquid does it via
`switch (json?.channel)` after a single up-front parse. The exact extraction
varies by exchange wire format, but the pattern is uniform.

For exchanges where the routing key is split across nested fields
(e.g. Binance `kline.{interval}.{symbol}` — interval lives in `k.i`), keep
a small prefix-iteration fallback that only walks `${eventType}.*` keys —
not the full handler map. See `binance.ws-public.ts` →
`dispatchBinanceMessage` for the canonical three-tier shape (broadcast,
exact, prefix-fallback).

**Sub/unsub-ack suppression** still belongs in this dispatch. Don't add a
second full-body substring scan for it — one targeted `indexOf` is fine
(see BloFin's `'"event":"subscribe"'` check).

### `updateOrderBook` already maintains sort — never trail a `sortOrderBook`

`packages/safe-cex/src/utils/updateOrderBook.ts` does sorted-insertion via
binary search. Snapshots from every exchange we integrate (Bybit, Binance,
BloFin, Hyperliquid checked) arrive **pre-sorted** (bids DESC, asks ASC)
and are pushed in order. This means the trailing
`sortOrderBook(orderBook)` call that several adapters used to have was
re-sorting an already-sorted array on every WS frame — O(N log N) waste on
a 1000-level book per delta.

**Rule:** never call `sortOrderBook` after `updateOrderBook` or after
pushing a snapshot. If a parser appears to need it, the parser is wrong (a
delta isn't being applied through `updateOrderBook`, or the snapshot order
contract changed). Check with the exchange WS docs before adding it back.

`calcOrderBookTotal` is **not** redundant — `Orderbook.tsx` and
`useSmoothOrderbook.ts` consume `.total` directly. Keep it.

The cumulative `total` field is rendering-only (depth bars +
`useSmoothOrderbook` interpolation). It's deliberately accumulated with
plain `+` rather than `safe-math.add`. IEEE-754 drift (e.g.
`0.30000000000000004`) is invisible at any pixel scale, and amounts in
practice stay well under `2^53` so overflow isn't a concern. Don't
"fix" the drift back to safe-math — see commit
`8ce8bed530` for the reasoning.

### Hot-path orderbook deltas — `updateOrderBookByPrice`

`packages/safe-cex/src/utils/updateOrderBook.ts` exports two API shapes:

- **`updateOrderBook(orderBook, side, { price, amount, total: 0 })`** —
  the original object-arg form. Most exchanges still use this.
- **`updateOrderBookByPrice(orderBook, side, price, amount)`** — the
  primitive-arg form. Use this on hot delta paths (1000-level books at
  30–60 Hz/symbol).

The wrapper object passed into the object-arg form is allocated by the
caller per delta level but only persists in the book on the
_insertion_ path. On Bybit V5's 1000-level deltas ~95% of incoming
levels are existing-level mutations or removals — `update.amount` is
read for dispatch and `update.price` for binary search, then the
wrapper is discarded. Switching to `updateOrderBookByPrice` only
allocates inside the function on insertion. Bybit linear/inverse/spot
already migrated; other exchanges can move incrementally — both
forms are kept for backward compatibility.

### Reconnect-banner stats are append-only — never walk the full map

`base.ws.ts → recordMessageHandlerStats(topic)` is called on every
WS frame. The earlier shape walked **every** key in
`messageHandlersStats` per call to refresh `lastCallbacksCount` —
N×N work where N = subscription count (Bybit linear has ~600). The
walk was redundant: `cleanupMessageHandlerStats(topic)` deletes the
entry on handler-removal, and the only consumer
(`getGroupedMessageTopicStats`) treats `lastCallbacksCount` as
truthy/falsy. Stale-by-one between frames is harmless.

**Rule:** when adding stats-touching code on the WS hot path, only
update the current topic's entry. Don't iterate the stats map.

### Hot-path lookups — `tickersBySymbol` / `tickersById` over `tickers.find`

Every store carries a `tickersBySymbol` (and for some exchanges
`tickersById`) hashmap maintained alongside the `tickers` array. Use it
inside any public-WS handler that processes per-symbol updates.

**The biggest single win across the four exchanges audited so far** was
Binance's `!ticker@arr` handler: an array of ~600 events arrives once per
second, and the prior `tickers.find((t) => t.symbol === event.s)` pattern
inside `events.forEach` ran ~600 × ~600 = 360K linear comparisons per
frame. Switching to `tickersBySymbol[event.s]` makes it ~600 hashmap reads.

Same pattern applies to `markets.find(...)` on the hot path — but most
public-WS hot paths only need the market once at subscribe time, so this is
usually a non-issue. If you find a market lookup inside a parser callback,
it's worth replacing with `marketsBySymbol`.

**Pass the ticker reference, not `{ id }` wrapper.** Once you've done
the `tickersBySymbol[symbol]` lookup, the resulting ticker has the
`.id` field the store's `updateTicker` reads — pass it directly. The
`{ id: symbol }` literal that older code constructed allocated a
fresh wrapper per ticker frame (600/sec on Bybit linear) just to hand
the store the same string back. See `bybit.ws-public.ts:handleTickersEvent`
for the canonical shape.

**Avoid `positions.filter(...)` per frame on PnL-side-effect paths.**
Some ticker handlers update unrealized PnL on matching positions
(spot/inverse). `positions.filter(p => p.symbol === d.symbol)`
allocates a new array every frame even when the position list is
empty. Use an indexed for-loop with `continue`, and hoist any
loop-invariant lookups (`marketsBySymbol[d.symbol]`) above the loop —
all matched positions share the same symbol, so the market lookup is
invariant. See `bybitspot.ws-public.ts:handleTickersEvent`.

### Loop allocations — for-loop, not forEach/map/reduce, on hot paths

Per-frame closure allocations stack up: a 100-level orderbook delta with
`forEach` allocates 100 callback frames, GC churns, main thread stutters
become visible at ~250 frames/sec aggregate.

**Don't:**

- `Object.entries({ bids: update.b, asks: update.a }).forEach(...)` —
  allocates the entries-tuple array, plus a closure per side, plus a
  closure per level.
- `levels.forEach(({ px, sz }) => ...)` — closure + destructuring frame
  per level.
- `.reduce((acc, x) => [...acc, x], [])` — quadratic; allocates a fresh
  array on every iteration.
- `[...newArr, ...existingArr.slice(newArr.length)]` — three array
  allocations to merge top-N replacement levels into an existing book.

**Do:**

- Direct `.b` / `.a` (or `.bids` / `.asks`) access, indexed `for` loops.
- Plain `for (let i = 0; i < arr.length; i++)` with explicit index access
  — beats both `forEach` and `for...of` on hot paths (no iterator
  protocol, no closure).
- For top-N replacement of an existing orderbook, write in place via
  index assignment (`orderBook.bids[i] = level`) and only `push` past the
  current length. See BloFin's `secondaryParser` for the canonical shape.

`for...in` on plain objects (`for (const k in data)`) avoids the
`Object.entries` / `Object.keys` allocation when you don't need the array
form — useful for high-frequency per-symbol map iteration like
Hyperliquid's `allMids`.

### Direct handler lookup over `Object.keys.filter`

If your dispatch already parses the JSON up-front (Hyperliquid pattern),
you still want direct Map lookup, not
`Object.keys(messageHandlers).filter(k => k === full).forEach(...)`. The
filter walks every handler key and allocates an intermediate array; a
direct `messageHandlers[fullKey]` lookup is what was intended. Same for
"all handlers under this prefix" — sample the message to construct the
exact key when possible (see Hyperliquid's candle dispatch:
`candle.${sample.s}.${sample.i}`).

### `safe-math` is for placement/display, not chart accumulation

`packages/safe-cex/src/utils/safe-math.ts` exports `add` / `subtract` /
`multiply` / `divide` / `adjust` that snap results to the input's
decimal precision via `String(num)` + `afterDecimal` + `Math.round`.
Cheap when called once; very expensive when run per delta level or
per trade.

**Use `safe-math.add` / `subtract` for:** order placement (price/qty
adjusted to tickSize/qtyStep), balance math that feeds back into
order signing, anywhere a value goes into a string the user / API
sees and decimal drift would surprise.

**Don't use it for:** chart-volume accumulation in candle parsers
(consumers display via `.toFixed`), orderbook `total` cumulative
sums (rendering-only), or any internal aggregate that's never
serialized. Plain `+` is dramatically faster and the drift is
invisible. See commits `bac1839d28` (orderbook),
`74b1b462f5` / `7e4733c377` (candle volume) for examples.

### Cache module-scope constants — never compute per frame

`inverseObj` (and similar reduce-spread helpers) allocate a fresh map
on every call. The Bybit linear `listenOHLCVs` dispatcher used to
call `inverseObj(INTERVAL)` inside the per-frame closure — for the
12-key INTERVAL map that's ~78 property copies per kline frame.

**Rule:** if a parsed lookup table is built from a static enum / map
(intervals, sides, statuses, types), hoist the inverse to a
**module-level `const`** and reference it from the closure. Same
applies to substring constants used in repeated `slice` /
`startsWith` calls (e.g. `KLINE_TOPIC_PREFIX_LEN = 6` for
`"kline.{interval}.{symbol}"` parsing replaces a `topic.split('.')`
that allocated a 3-element array per frame).

### Avoid `..._rest` destructuring on the hot path

Object destructuring with a rest spread (`{ T, p, v, ..._rest }`)
**collects unused fields into a fresh object** on every call —
exactly the allocation we'd want to avoid. For Bybit V5 publicTrade
frames that carry `s/L/i/BT/RPI` we don't read, the rest wrapper
fired per trade. Read fields by key from the object directly:

```ts
const trade = data[0];
const T = trade.T;
const p = trade.p;
// no rest — fields you don't read just stay unread.
```

### Don't propagate full snapshots through `parent.store.update` per source notify

`exchanges/in-process-public-data.ts` is the canonical example. The
provider subscribes to a source store (e.g. one Bybit account's
public WS) and forwards tickers/markets to a parent store (a second
account that shares the public feed). Calling
`parent.store.update({ tickers })` on every source notify rebuilds
the parent's `tickerIndexMap` + `tickersMap` + `tickersBySymbolMap`
**from scratch** — `Map.clear` + N `Map.set` per frame. On Bybit
linear that's ~360K hashmap inserts per second per shared parent.

The source store mutates ticker fields **in place**, and
`stripTickerInfo` returns the same reference when there's no
`info` field, so the parent's maps already point to the same
ticker objects. Per-frame mutations are visible without any
propagation work.

**Rule:** when fanning out a store snapshot, only re-publish the
array when the **reference or length** actually changes (snapshot
replacement, ticker added, ticker removed). Cache `lastTickers` /
`lastMarkets` references to gate the call.

### `pendingTickerDeltas` is dormant — don't repopulate it

The store still exposes `consumeTickerDeltas` /
`applyTickerDeltas` / `applyPackedTickerDeltas` for the
virtual-worker / relay-shim layer, but `updateTicker` deliberately
**does not** populate `pendingTickerDeltas` anymore. Delta
patching moved out of the store; the queue is only exercised by
test scaffolding. Don't reintroduce the
`Map.get` + `Object.assign`/spread per ticker frame "for
completeness" — it's wasted work on the hottest path in the app.

### Postprocessor pipelines run at 4 Hz — pre-build per-symbol maps once

`_storeUpdate` runs the postprocessor pipeline at the throttle
cadence (~4 Hz). Each independent postprocessor receives a
**serialized** snapshot — `tickersBySymbol` / `marketsBySymbol`
hashmaps live on the live store and aren't visible there.

When a postprocessor needs symbol-keyed lookups inside a position
loop or a fake-position synthesis pass, build the maps **once**
at the top of `process()` via an indexed `for` loop with a plain
`Record`:

```ts
const tickerMap: Record<string, Ticker> = {};
for (let i = 0; i < tickers.length; i++) tickerMap[tickers[i].symbol] = tickers[i];
```

**Don't** use `new Map(arr.map(t => [t.symbol, t]))` — it allocates
the tuple array, N two-element tuples, and a Map. The raw `for`-loop

- object literal is one allocation total.

Same shape applies to per-symbol order/position bucketing inside
`utils/risk.ts → calculateOrdersRisk`, where the prior `find` /
`filter` per position iteration ran ~65K comparisons per call
twice per pipeline pass.

### Sanitize-on-postMessage: don't `.map()` arrays that don't need it

`webworker/ExchangeWorker.ts → _sanitizeForPostMessage` strips
the `info` property off orders / positions before postMessage to
avoid circular-reference errors. `Array.prototype.map` allocates
a new array of the same length **even when every callback returns
the input** — when nothing carries `info`, that's pure waste.
First scan the array once for any item with `info`; only call
`.map()` when at least one needs sanitizing.

### Don't deep-clone payloads before postMessage

`postMessage` already runs `structuredClone` on the worker → main
boundary. Any deep `clone()` you add to a snapshot field
_duplicates_ that work for nothing — the synchronous window between
your assignment and the postMessage call cannot be interrupted by a
WS frame, so the receiver still sees a clean snapshot at send time.
The previous `MessageHandlerStatsPostprocessor` doing
`clone(websocket.messageHandlersStats)` per type per pass was a
canonical example.

### `virtualClock.getCurrentTimeMs` is `Date.now() + offsetMs` — no dayjs

`virtualClock.getCurrentTime()` still returns a `Dayjs` for callers
that need it, but `getCurrentTimeMs()` is the hot-path entrypoint
and is `Date.now() + this.offsetMs`. The offset is computed once at
sync time. Don't reintroduce `dayjs.utc().valueOf()` here — the
allocation amortizes to ~1ms/sec just on Bybit `mapOrder` /
`mapPosition` calls, and the math is identity-equivalent.

### Cache pre-built lookup maps across postprocessor passes

When a postprocessor builds `tickerBySymbol` / `marketBySymbol`
from `store.tickers` / `store.markets`, cache the result on the
instance keyed on the source-array _reference_. The store mutates
ticker/market fields in place per WS frame but only swaps the
array reference on full snapshot replacement (initial fetch,
`markets:changed`, etc.). Reference-stable arrays mean the cached
map is still correct (its values are the same ticker objects).
See `DisplayedPositionsPostprocessor.process` for the pattern.

### Bybit `parseTickers` and `deriveAlgoOrdersFromNormalOrdersOpts`

Both used to build their result via the
`reduce + [...acc, ticker]` quadratic anti-pattern called out
above. Both are now indexed for-loops with direct push. New
exchange parsers that build a result array from a raw payload
should follow the same shape — never use `reduce + spread` for
list-building.

### Hoist subscription-invariant work out of the per-frame parser

In `listenOHLCV.secondaryParser` (per-trade candle construction),
two values were being recomputed on every trade:

- `getCandleStartTime(opts.interval, T / 1000)` — looks up
  `timeframeToSeconds[opts.interval]` and runs a ms-vs-seconds
  normalization branch that's a no-op when the caller already
  divides by 1000. Inline as
  `Math.floor(T / 1000 / intervalSeconds) * intervalSeconds` with
  `intervalSeconds` cached at subscription time.
- `opts.interval.endsWith('s')` — invariant per subscription, hoist.

For Bybit publicTrade on a busy market (hundreds of trades/sec), this
removes per-trade function call + branch overhead.

### Indexed for-loop over `for (const cb of arr)` in WS dispatch

The Bybit dispatch's
`if (hasExact) for (const cb of exact!) cb(json)` pattern allocates
an iterator object per frame. Hot-path frames overwhelmingly hit
single-element handler arrays — the iterator wrapper is real cost.
Use indexed for-loop:

```ts
const arr = exact!;
for (let i = 0; i < arr.length; i++) arr[i](json);
```

Same applies to the pong dispatch fallback.

### Mutate locally-owned arrays instead of `.map()`-cloning a status flip

`bybit*.ws-private.ts → handleOrderTopic` runs
`orders.map(o => ({ ...o, status: newStatus }))` on every terminal
order event. The `orders` array is locally owned (returned by
`mapOrder` for our exclusive use) and the next consumer
(`addOrUpdateOrders`) either mutates the matching store entry in
place or pushes our object straight in, so direct mutation is
safe and skips the per-event spread allocations.

### Reusable update buffers on per-frame ticker/asset paths

When a frame iterates ~600 entries and calls `store.updateTicker(ticker, { ...fields })`
per entry (e.g. Hyperliquid `handleAllMids` /
`handleHip3AllMids` / `handleWebData2`), the per-entry update
object is allocated solely for the iteration of `updateTicker`'s
internal `for...in` loop and is immediately garbage. Hoist a
**single reusable buffer** to the class instance, mutate its
fields each iteration, pass to `updateTicker` — the store does
not retain the reference, so reuse is safe.

```ts
private _midsUpdateBuf = { last: 0 };
// per frame:
for (const id in mids) {
  buf.last = parseFloat(mids[id]);
  this.parent.store.updateTicker(ticker, buf);
}
```

This pattern is **only** valid when the buffer always carries the
exact same set of fields each iteration. If the field set varies
(some entries set `bid`, others set `ask`), you'd need to clear
stale fields, which costs roughly the same as a fresh allocation.

Same applies to the `Set<string>` tracking which symbols updated
(used to gate `syncBalanceFromLivePositions`): skip allocation
entirely when `positions.length === 0`. The downstream consumer
bails fast in that case anyway.

### Spread + uniqBy → Map-based merge

The pattern
`uniqBy([...incoming, ...preserve, ...fakes], 'id')` allocates the
triple-spread array (sized to the sum), then `uniqBy` walks it and
allocates a deduped result + an internal Set. For Hyperliquid HIP-3
webData2 frames the merged array is ~610 entries (mostly fakes).

A single Map-based merge does the same job with one Map + one final
`Array.from`:

```ts
// uniqBy keeps FIRST occurrence; Map.set keeps LAST. To preserve
// "incoming wins over preserve wins over fake" precedence, insert
// in REVERSE: fakes first (lowest), preserve, then incoming last.
const merged = new Map<string, T>();
for (let i = 0; i < fakes.length; i++) merged.set(fakes[i].id, fakes[i]);
for (let i = 0; i < preserve.length; i++) merged.set(preserve[i].id, preserve[i]);
for (let i = 0; i < incoming.length; i++) merged.set(incoming[i].id, incoming[i]);
this.parent.store.addOrUpdatePositionsFull(Array.from(merged.values()));
```

### `filter().map().filter(o => o !== null)` → single for-loop

Three sequential array passes, three intermediate arrays, two
closure pools per call. Replace with one indexed for-loop using
`continue` for skip cases and `push` for the build step:

```ts
const result: T[] = [];
for (let i = 0; i < input.length; i++) {
  const x = input[i];
  if (skipReason1(x)) continue;
  if (skipReason2(x)) continue;
  result.push(buildResult(x));
}
```

Applies to webData2 openOrders / clearinghouseState position parsing
and any other `Array → Array` transform with mid-pipeline drops.

### Pre-build `existingOrdersById` for batched WS handlers

Handlers like `handleWebData2` and `handleOrderUpdates` look up the
existing store order multiple times per incoming entry (HIP-3 type
preservation, tif fallback, ...). With ~50 batched updates × ~50
stored orders × 2 lookups, that's ~5K linear comparisons per frame.

Build the map once at the top of the batch:

```ts
const existingOrdersById: Record<string, Order> = {};
const storeOrders = this.parent.store.orders;
for (let i = 0; i < storeOrders.length; i++) {
  existingOrdersById[storeOrders[i].id] = storeOrders[i];
}
```

Per-update lookup drops to O(1).

### Frozen singleton for invariant config returns

If a method returns the same object literal on every call when no
state changes (e.g. `_getSymbolConfigurationFromAssetData`'s
defaults branch, Hyperliquid spot's "always 1x cross" config),
hoist the literal as a class-static `Object.freeze`'d singleton.
Callers that always spread the result into a new object
(`...config`) won't notice; callers that mutate would crash —
freeze makes the contract explicit.

### Don't add a "mutate-then-notify" escape hatch on the store

It's tempting to bypass `updateTicker(ticker, changes)` with a
"caller mutates the live ticker, then calls
`store.notifyTickerMutated(ticker)` to signal subscribers" path —
saves the per-frame `update` object allocation. **Don't.**

The store's single chokepoint for ticker mutation is what makes
`updateTicker` greppable, breakpointable, and reliable as the
boundary for delta tracking, audit logging, replay, and any future
observer that depends on "every ticker change goes through this
function." Splitting the contract into two paths is invisible from
the consumer side and silently breaks any of those observers when
the bypass is taken. The reusable-buffer pattern (above) gets ~95%
of the perf win without splitting the API; prefer it.

If you ever genuinely need the bypass, add a real metric showing
the cost dominates and propose with the consumer-side audit
attached.

### Per-event vs per-frame: the bar for new store-wide hashmaps

A `findIndex` / `find` over `store.orders` or `store.positions` on
the WS dispatch path looks like an obvious "add a `positionsBySymbol`
hashmap to the store" win — until you count how often it actually
runs.

Per-event paths (e.g. `applyOptimisticCorePositionFromFill` —
fires per **fill**, not per WS frame) hit the find at user-trading
frequency: at most a few times per second during active sessions,
zero otherwise. Even with 50 stored positions, that's a couple
hundred comparisons per second peak.

Adding a new store-wide hashmap means maintaining it across
**every** mutation path: `push`, `splice`, `addOrUpdate*`,
`removeOrders`, `setOrdersForSymbol`, `update({ positions })`,
`addOrUpdatePositionsFull`, etc. — 8+ touch points across the
store, plus tests, plus a new invariant the next agent has to
learn. `splice`-induced index shifts make the maintenance especially
fiddly.

**Bar:** add a new store-wide hashmap only when a `find` is hit on
the **dominant per-frame path** of at least one exchange (i.e.
inside a ticker / orderbook / kline handler that fires hundreds of
times per second). Per-event-frequency paths get an indexed
for-loop with `continue`, not a hashmap.

### Subscribe-time throttles — review before adding

It's tempting to add `await sleep(N)` between subscribe chunks to avoid
upstream rate limits. Bybit needs it (~5–10ms); most exchanges either
don't, or document an explicit per-request constraint. Before adding
sleeps, check the exchange's published WS subscription rate limit and pick
the shortest sleep that respects it — first-frame latency on connect
multiplies by chunk count. Default to documented limits, not folklore.

### What's deliberately out of scope here

These items are tracked but not yet shipped — flag them for the relevant
PR rather than fixing inline:

- **Dynamic ticker subscription on Bybit linear / BloFin** — both still
  subscribe to all market tickers on connect. Switching to "active
  symbols only" is a behavior-change PR (consumers must tolerate first-use
  ticker latency). See Bybit's inverse / spot variants for the
  `activeTickerSymbols:changed` pattern that's the migration target.
- **Throttle orderbook callbacks at the parser level** —
  `useSmoothOrderbook` and the depth chart consume the orderbook reference
  on every callback. A 100ms throttle would significantly reduce React
  state updates but risks tick-by-tick consumers; needs an opt-in flag.
- **Defer `calcOrderBookTotal` to consumer side** — `Orderbook.tsx` only
  renders ~50 levels; computing cumulative totals for all 1000 is wasted
  but the consumer-side refactor touches every orderbook caller.

## Per-Exchange CLAUDE.md Template

Per-exchange docs should focus on what's **specific** to that exchange. Do
**not** restate the conventions above — link to this parent doc instead.

### Required sections

1. **Exchange information** — name, variants, production/dev status, testnet
   support.
2. **API documentation** — links to upstream docs and CCXT reference (REST
   and WebSocket).
3. **File structure** — only if it deviates from the standard layout above.
   If it follows the standard, write "Follows the standard layout — see
   `../CLAUDE.md`."
4. **Authentication** — signing scheme, header convention, anything unusual
   about timestamps, recvWindow, request body handling.
5. **Known quirks / implementation notes** — exchange-specific
   gotchas: silent fields, undocumented behavior, broker prefixes, rate-limit
   surprises, listen-key TTL oddities, etc. **This is the highest-value
   section.** It's what isn't discoverable from the upstream docs.

### Optional sections

- Startup sequence — only if it diverges meaningfully from the common
  shape above.
- REST endpoints — link to upstream rather than restating.
- WebSocket implementation — channel mapping, subscription quirks.
- Order types and mappings — exchange-specific order type → safe-cex enum
  table.
- Order lifecycle — only if non-obvious.
- Symbol format — only if the exchange uses something unusual.
- Data precision and conversion — tick size, lot size, contract size
  handling.
- Rate limiting — only if more aggressive than the default backoff.
- State reconciliation — how often `fetchPositions`/`fetchBalance` runs,
  any tick-modulo logic.
- Exchange metadata — `exchangeMetadata.ts` flags specific to this exchange.
- Testnet / demo — base URL differences, auth differences.
- Error handling — exchange-specific error code mapping.
- Testing / validation — anything beyond the standard `yarn test-ci`.
- Variants — sub-table per variant if multiple exchange names share the
  directory.
- Monitoring / Grafana — links to dashboards.
- SDK references — upstream SDK link, CCXT link.
- Known gaps — features not yet implemented.

### What NOT to put in per-exchange docs

- The file layout convention (covered here).
- The `BaseExchange` contract (covered here).
- The `this.log()` / `LogSeverity` pattern (covered here).
- The disposal pattern, virtualClock, big-int parsing (covered here).
- Proxy/tunnel architecture (covered in `packages/safe-cex/CLAUDE.md`).
- Long copy-pastes of upstream docs — link instead.
- Dated review notes, TODO dumps, `UNVERIFIED` blocks — those belong in
  PR descriptions or issues, not in load-bearing docs.

### Source line references

Per-exchange docs sometimes reference exact lines (e.g. `binance.api.ts:99`).
These drift fast. Prefer:

- Quoting the function/method name (`createApiWithSecondaryKey`) over a
  line number.
- Using a line number only when the file is large and the reference would
  otherwise be hard to find. When you do, mention the function name too so
  the reference survives a refactor.

## Adding a New Exchange

1. Use `yarn add-exchange` (skill: `add-exchange`) to scaffold the
   directory and files in the standard layout.
2. Implement REST + WS following the shape in a similar exchange (look at
   one with comparable architecture: Binance for HMAC HMAC + listen-key,
   OKX for OAuth-style passphrase, Hyperliquid for EIP-712, etc.).
3. Add metadata to `exchangeMetadata.ts` — see `packages/safe-cex/CLAUDE.md`
   for the proxy/tunnel flag matrix.
4. Register in `registry.ts`.
5. Write the per-exchange `CLAUDE.md` against the template above. Link back
   to this parent doc instead of restating shared mechanics.
6. Run `yarn test-ci` from `packages/safe-cex` — `exchangeMetadata.test.ts`
   has invariant checks that catch impossible flag combinations.

## Reference

- Upstream CCXT: https://github.com/ccxt/ccxt — read the relevant exchange
  file when implementing or debugging signing/endpoints.
- Proxy/tunnel architecture: `packages/safe-cex/CLAUDE.md`.
- Tunnel infrastructure: `packages/safe-cex/src/httpOverWebsocket/`.
