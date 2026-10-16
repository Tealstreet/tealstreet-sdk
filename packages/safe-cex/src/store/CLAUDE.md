# CLAUDE.md — safe-cex/src/store

The Tealstreet exchange store. One `DefaultStore` per exchange
account; mirrored across worker → main thread via the worker's
throttled postMessage.

`serialize()` carries every derived display bridge field, including
`displayedPositions` and `displayedMargin`. Hosted or force-full-store paths
must not depend on postprocessors happening to reconstruct a field after
serialization.

**A field added to `store.stub.ts` MUST also be added to `serialize()`.**
`apps/web`'s IndexedDB warm-boot cache validates snapshots with
`shapesCompatible(stubStore, storeData)`, which requires every stub key to be
present — so a stub-only field silently invalidates every cached snapshot and
cold-boots every account, with no error anywhere, because a cache miss is
indistinguishable from a cold start. `requestStats` shipped that way once;
`store.base.test.ts` now asserts the parity so the next one cannot.

### `requestStats` — REST outcome counters

Cumulative per-outcome REST counts written by the response interceptors in
`safeCexAxiosCreate.ts`. `recordRequestOutcome` mutates in place and
**deliberately does not `notify()`** — it runs on every request, and the 250 ms
throttled worker ship already carries the field. See
`packages/safe-cex/CLAUDE.md` → "REST request counters ride the store".

For exchange WS dispatch and per-frame patterns that interact with
this store, see `../exchanges/CLAUDE.md` → "Public WebSocket
Hot-Path Patterns". This doc covers the **store contracts** —
what callers can rely on, and what's load-bearing.

## Files

- `store.interface.ts` — public surface every store implementation
  must provide.
- `store.base.ts` — `DefaultStore`. The only implementation in
  production today.
- `store.stub.ts` — fully-populated typed stub used for runtime
  shape validation. Update when `StoreData` types change.
- `store.base.test.ts` — covers ticker/position/order mutation
  paths and delta application.

## Mutation contracts (don't break these)

### Single chokepoint per type

There is exactly **one** public mutation path for each top-level
collection. Don't add bypass paths even when they look like they'd
save an allocation:

| Collection | Public path                                                                                                                  |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------- |
| tickers    | `updateTicker(ticker, changes)` and the `applyTickerDeltas` family                                                           |
| markets    | `updateMarket(market, changes)`                                                                                              |
| orders     | `addOrUpdateOrders` / `updateOrder` / `removeOrders` / `setOrdersForSymbol` / `addOrUpdateOrdersFull` / `update({ orders })` |
| positions  | `updatePosition` / `addOrUpdatePositions` / `addOrUpdatePositionsFull` / `update({ positions })`                             |

Single chokepoints are what make these greppable, breakpointable,
and reliable as observation boundaries (delta tracking, audit logs,
replay, future observers). The temptation: "the caller already
holds the live ticker via `tickersBySymbol[symbol]` and could just
mutate the field, then call a `notifyTickerMutated` shim to tell
subscribers." It works mechanically — both paths land at the same
in-place mutation — but it splits the contract in a way that's
invisible to consumers and silently breaks any observer that
expected `updateTicker` to be called for every change. **The
~600-allocs/sec the bypass would save isn't worth the API split.**
The reusable-buffer pattern (see `../exchanges/CLAUDE.md` →
"Reusable update buffers") gets ~95% of the perf win without
splitting the API; prefer it.

### `updateIds` bump on bulk replacement

Every order- or position-mutating method in the store bumps
`updateIds.orders` or `updateIds.positions`. **Including
`update({ orders })` and `update({ positions })`** — those are the
wholesale-replacement paths used by:

- Bybit `fetchAndSetPrivateSnapshotHelper` (every tick loop)
- BloFin / Crypto.com / GRVT initial-orders fetch
- in-process-public-data fan-out

The worker payload bridge and other updateId-backed consumers gate
raw order/position snapshots on these counters. Without consistent
bumps from `update()`, a wholesale refresh can be pruned as
unchanged. If you add a new bulk-replacement helper, bump the
relevant `updateIds.*`.

An attempted mutation of a missing row is a no-op: `removeOrder` and
`updatePosition` must not advance an updateId unless they find and mutate the
target. Advancing the counter without notifying makes the next unrelated store
notification resend an unchanged collection.

### Tickers / markets are mutated in place

`updateInArray` writes fields directly onto the existing ticker /
market object — no spread, no new array. The hashmap entries
(`tickersMap`, `tickersBySymbol`, `marketsMap`, `marketsBySymbol`)
keep the same references; the post-mutation `tickersMap[id] =
ticker` writes are defensive (in case a future code path swaps the
underlying object), not load-bearing for correctness.

This is why the in-process-public-data provider can skip
`parent.store.update({ tickers })` on every source notify — both
stores share the same ticker object references, so per-frame
mutations are visible without any propagation work. Only
re-publish when the array **reference or length** changes (snapshot
replacement, ticker added/removed).

### `pendingTickerDeltas` is dormant

`updateTicker` deliberately does not populate `pendingTickerDeltas`.
The `applyTickerDeltas` / `applyPackedTickerDeltas` /
`consumeTickerDeltas` API stays as a legacy compatibility surface and for
deterministic store tests. Live worker delta transport is produced directly by
`ExchangeWorker._packTickerDeltasSinceBaseline`; no runtime producer writes
this store queue. Per-frame `Map.get` + `Object.assign`/spread for a queue
nothing reads is wasted work; don't reintroduce it.

## Hot-path internals (don't undo these)

### `updateInArray` uses `for...in`, not `Object.keys`

`updateInArray` iterates the changes object with `for...in` to
write fields onto the live item. The previous shape used
`Object.keys(changes)` + a `for-of` over the keys array — that
allocated an N-element array per call. With Bybit linear's ~600
ticker frames/sec and ~5–11 fields per update, the `Object.keys`
allocation alone was 600 throwaway arrays/sec.

`for...in` over plain object literals is equivalent here (callers
always pass `{}` literals from inline construction; no inherited
enumerables to worry about). Don't switch back without a real
reason.

### `updateTicker` reads via `tickersMap[id]`, not `tickerIndexMap.get(id)`

Both routes return the same reference. The hashmap path is one
fewer call frame per ticker frame. The index value isn't used for
anything else (the prior `pendingTickerDeltas.set(idx, ...)` path
was removed).

### `updateTicker` and `updateInArray` share a single field-write pass

The hot path inlines the empty-check and field-write in one
`for...in` over `normalizedChanges`. Don't refactor the empty-check
into its own pre-pass via `Object.keys(...).length === 0` — that
allocates the keys array just to ask "is anything here?".

### `auditPositions()` is hoisted out of `addOrUpdatePositions`'s loop

The audit does an O(N) `uniqBy` + filter; running it inside the
forEach multiplied the work by batch size for no extra signal.
Single call after the loop. If you add a new bulk-position
mutator, follow the same pattern.

### Batch order/position helpers build one transient id index

`addOrUpdateOrders`, `updatePositions`, `addOrUpdatePositions`, and
`addOrUpdatePositionsFull` build a local `id -> index` object only when
both the incoming batch and existing store list are multi-item. That
turns batch updates from repeated `findIndex` walks into one O(N)
index-build plus O(1) lookups, while preserving the cheap single-item
path. If the helper appends a new row, it also writes that new id into
the transient index so later items in the same batch can see it.

`removeOrders` / `updateOrdersStatus` similarly build an `id -> true`
lookup and return without `notify()` when nothing changed. Don't switch
these back to repeated `.some()` / `.findIndex()` loops or unconditional
notifications; the store bridge already posts at 4 Hz, so no-op wakes
are pure overhead.

### `recordMessageHandlerStats` touches one topic; the map walk is throttled

Lives in `../exchanges/base.ws.ts`, not the store, but worth
flagging here because it's the most-frequently-hit shared method.
The original shape refreshed `lastCallbacksCount` for **every** key
in `messageHandlersStats` per WS frame — N×N work. The frame path
now mutates only the current topic's entry, and the map-wide refresh
moved to `reconcileMessageHandlerStats`, throttled to 5 s.

**Do not delete that walk as redundant** — it was removed once on
exactly that reasoning and had to be restored. Optimise the frame
path, not the reconcile. Why, in full: `../exchanges/CLAUDE.md` →
"`messageHandlerStats` checks SUBSCRIPTIONS, not the connection".

## `notify` semantics

`notify()` fires every store mutation. With a throttled subscriber
(`ExchangeWorker._throttledStoreUpdate`, 250 ms cadence), the
expensive postMessage path is gated. Other subscribers (e.g.
`BaseExchange`'s `'update'` event re-emit, `in-process-public-data`)
fire per call — keep their work cheap, and gate per-frame work on
`updateIds.*` only when price-only updates are not semantic input.
`ChaserOrderManager.onUpdate` is the exception: order updates must
bypass throttling, and ticker-only notifies are rate-limited rather
than skipped so the chaser can still move closer to the touch.

## What lives elsewhere

- Per-exchange WS hot-path patterns: `../exchanges/CLAUDE.md`.
- Postprocessor pipeline (4 Hz cadence, runs on every store
  update) and per-postprocessor optimizations:
  `../webworker/postProcessors/`.
- The throttled worker → main bridge: `../webworker/ExchangeWorker.ts`.
- Live packed-ticker production:
  `ExchangeWorker._packTickerDeltasSinceBaseline`; the store's apply/consume
  methods are compatibility and test scaffolding, not an active producer path.


## `markOrdersTerminal` — a stale snapshot cannot reopen a closed order

A REST orders snapshot can read the venue BEFORE a cancel or fill and land
AFTER the private stream reported it; replacing the list wholesale then reopened
the order. `markOrdersTerminal(ids, Closed | Canceled)` sets the status AND
remembers the ids for 60s (bounded at 1,000, cleared by `reset`); every
orders-replacing or upserting path (`update({ orders })` except `fromWorker`,
`addOrder`, `addOrUpdateOrder(s)`, `addOrUpdateOrdersFull`, `setOrdersForSymbol`)
refuses a NON-terminal row for a remembered id — the store's terminal row stands
in, or the row is dropped if the order already left.

`isTerminalOrder(order)` shares the store's `Closed` / `Canceled` classification
with cancellation selectors and empty-snapshot diagnostics. Those rows may still
be present in `store.orders`; local in-flight states remain non-terminal.

**It is opt-in, and must stay that way.** Nothing is inferred from
`removeOrders` (Hyperliquid's reconciler, Lighter and ApeX remove live orders
and rely on them coming back) or from `updateOrdersStatus` (Bybit and Phemex map
a non-final "Triggered" to Closed; Binance sends `EXPIRED` when a stop
triggers). Only a venue whose stream event is FINAL calls it — OKX's `filled` /
`canceled` today. `fromWorker` is skipped because the worker's store has already
applied it. Proven by the gap-4 cases in `__tests__/faultBench/badLink.scenarios.test.ts`.
