# CLAUDE.md — safe-cex/src/store

The Tealstreet exchange store. One `DefaultStore` per exchange
account; mirrored across worker → main thread via the worker's
throttled postMessage.

Rust frames keep the venue adapter's published SDK quantity units. COIN-M
orders carry native contract counts while positions carry base quantity; the
COIN-M applier preserves the USD contract multiplier in native sizing extras.
For MEXC, BloFin, KuCoin futures, Coinbase and the OKX family,
`FrameApplier` also retains native placement-unit order extras in both worker
and main stores. Prefer the exact native remaining column when present; legacy
frames reconstruct placement quantities from `contractSize` so chaser arithmetic
retains its native-volume contract. Use decimal-safe division here: a base fill
of 0.3 at multiplier 0.1 must reconstruct exactly 3 lots. Position frames also
restore `extra.contractsNative` for native-volume venues, which copier closes
consume as their placement quantity. `SETTINGS` optionally carries
separate buy/sell leverage columns; do not collapse these hedge rails to the
single representative leverage value when applying frames. `POSITIONS` may append
string references for venue-defined position IDs; preserve these when present
so simultaneous Cross and Isolated rows for a symbol remain distinct. Older
frames still derive identity from symbol and position mode.

Revised SCXF sections carry explicit revisions with fixed tails. Preserve venue
`prettySymbol`, bracket trigger types, conditional prices, remaining notionals,
and ticker `midPrice`, including clearing midpoint on packed and object worker
updates. Shared public ticker updates also preserve midpoint: omitted keeps the
cached value, a number replaces it, and null clears it across native polling. Orders, positions, fills and settings retain explicit symbols for rows
missing from MARKETS. `FrameApplier` owns fetched-leverage writes, including rare
query results; a SETTINGS replacement remains authoritative. Frame forwarding
also sends changed query-derived leverage and includes it in late snapshots,
since such reads need not cause an engine SETTINGS frame. Recovery snapshots
carry a copied leverage map after flushing pending SETTINGS. Cached rails with
missing, nonfinite or nonpositive sides are unknown, not market maxima.

A failed book or account section must not suppress independent valid sections.
Rejecting a book batch invalidates its baselines until fresh full books arrive;
rejecting an envelope invalidates indexed identities until fresh MARKETS arrive.
Stage identities before adopting a new table, retain the old table when market
metadata fails, and use explicit fill identities until a snapshot repairs it.
Apply failures request a bounded local engine snapshot or worker snapshot RPC;
snapshots contain no executions, so deliver valid original FILLS once. Listener
failures are distinct from protocol failures and do not trigger snapshot loops.

FILLS revision 2 carries optional native execution IDs and the existing financial
columns. Event `id` uses that ID when present, falling back to `orderId`; order
attribution always uses the separate `orderId`. Legacy revision-1 frames remain
readable both with and without the optional #1430 financial tails.

Initial Rust SDK readiness requires successfully decoded catalogue and account
sections as well as native STATUS flags. A failed section revokes its decoded
evidence until a valid replacement arrives; independent valid sections remain
usable, and validated empty tables count. Invalid catalogue identity revokes
all indexed evidence. A shared catalogue must pass compatibility validation
before its store notification can complete startup. Private startup does not require
tickers. Public-only startup still requires an accepted ticker table, and shared
startup requires a matching finite quote; a null-only midpoint clear is not a
first quote.

Aggregate venue ticker streams may still contain delisted symbols absent from
the current market table. `TICKERS` then carries optional symbol/ID references;
`FrameApplier` uses those identities for orphan rows and preserves packed delta
indexes. Keep those ticker rows without synthesizing markets or filtering them.

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

Native `ACCOUNT_READ` frames are the explicit exception: FrameApplier uses
`update(changes, false, 'account-read')` to apply and notify their current values
without advancing order/position freshness. Reading the same native state is
not another venue observation; the pending ordinary stream frame owns that
freshness increment. The marker travels with transferred frames so worker and
main apply the same rule. Ordinary bulk updates retain their existing counters.

`FILLS` may append reported realized-PnL/fee columns; absent metadata stays
absent, while zero remains a reported value. Keep each venue's live-event PnL
convention separate from historical execution normalization. Forward the native
order identity as both the existing fill `id` and `orderId`: chaser tracking
requires `orderId` for attribution. Preserve generic `Order.triggerType` beside
the existing stop-loss/take-profit aliases.

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

**It is opt-in, and must stay that way.** Nothing is inferred from
`removeOrders` (Hyperliquid's reconciler, Lighter and ApeX remove live orders
and rely on them coming back) or from `updateOrdersStatus` (Bybit and Phemex map
a non-final "Triggered" to Closed; Binance sends `EXPIRED` when a stop
triggers). Only a venue whose stream event is FINAL calls it — OKX's `filled` /
`canceled`, and BloFin's `filled` / `canceled` / `effective` / `order_failed`. `fromWorker` is skipped because the worker's store has already
applied it. Proven by the gap-4 cases in `__tests__/faultBench/badLink.scenarios.test.ts`.

## Rust frame books

`FrameApplier` retains book baselines by symbol and market ID, independent of
market-row ordering. `BOOK` replaces both sides, including an empty reset;
`BOOK_DELTA` upserts price/amount pairs and deletes zero amounts. Prices may
be any finite value, including zero-price prediction levels; only amount zero
deletes a level. Consumers
receive best-first sorted levels with recomputed cumulative totals. A delta
without a matching full baseline throws a snapshot-required error; reconnect
must create a new stream from its snapshot before applying deltas. Frame book
sections are ingested once, even when `apply` and `books` both read the frame,
and subsequent updates do not mutate books already handed to listeners. Actual book readers retain copied numeric baselines so transferring a source
frame cannot detach future delta state. Forwarding workers call
`apply(frame, false)` and leave book ingestion to the receiving main thread.

## Rust frame native units

`FrameApplier` preserves inverse sizing metadata across the worker bridge.
For Bybit/Bitget inverse, position `notional` is the native quote-contract
size, so the applier also publishes it as `extra.contractsNative`. Binance
COIN-M uses fixed USD contract sizes: its worker and main appliers divide
notional by market `contractSize` for native contract counts, and multiply exact
order leaves by `contractSize` for `remainingNotional`; `extra.unrealizedPnlNative`
keeps the native PnL. Copier closes consume the native size rather than the
rounded base-coin `contracts` field.

An optional trailing `ORDERS.remainingNative` frame column carries the venue's
exact native leaves quantity into `extra.remainingNative`; inverse markets also
publish `extra.remainingNotional`. Do not infer it by multiplying rounded base remainder
by price, or from an amount-zero Full position stop. Legacy seven-column frames
remain readable. Bybit leaves those extras absent on legacy frames; Bitget
inverse publishes native amount/filled, so its adapter can subtract those values
when the exact optional tail is absent. Order rejection metadata must survive
alongside the native sizing fields.

OKX frame bridges keep native contract-count placement amounts separately from
base-sized fills and remaining quantities. Exact native leaves populate
`remainingNative`, with inverse `remainingNotional` equal to those counts times
the quote contract value. A finite exact `filledInPlacementUnits` tail is
authoritative; legacy regular-order frames derive native fills as amount minus
leaves. Position `contractsNative` is base contracts divided by ctVal for linear
markets and quote notional divided by ctVal for inverse markets. OKX inverse
PnL is already converted to USD, so do not relabel it as coin-native PnL. Both
the worker and main-thread frame appliers use this OKX profile; other venue
quantity profiles keep their existing conventions. Strictly admitted X-Perp
market IDs retain their dated identity and their base/quote XPERP display label.

## Prediction-market frame metadata

Optional typed `PREDICTION_MARKETS` and `PREDICTION_HOLDINGS` sections carry
outcome condition/question/tick/neg-risk metadata and redemption/merge flags.
The numeric market and position tables retain their existing shapes. Both
worker and main-thread FrameApplier instances retain metadata across later
position-only frames; explicit empty sections clear it. Holdings removed from
the authoritative position table lose their flags before later reopening.

Each market's MARKETS flags carry its quantity units (bits 8-11, declared by the
venue's `Capabilities::quantity_units`): native placement (MEXC, BloFin, KuCoin
futures, Coinbase), inverse native (Bitget inverse), contract count (OKX) or
inverse contract-sized (Binance COIN-M). `FrameApplier` reads them per market,
so worker and main-thread appliers need no venue arguments; rows outside the
table use the venue's units, kept across an empty table. OKX native
contract-count metadata stays separate from Binance COIN-M contract multipliers.

## Instrument and derivative frame metadata

Optional `UNDERLYING_TOKENS` carries the venue token behind a market whose
display name differs from it: Hyperliquid's merged spot pairs publish the
token name and full name (`UBTC`, `Unit Bitcoin` behind `BTC/USDC`), applied as
`market.info.hlTokenName`/`hlTokenFullName`, which the web's Underlying Token
disclosure reads. Rows index the current market table and reset with it.

Optional `INSTRUMENT_CLASSIFICATIONS` preserves native `symbolType`,
`marketRegion` and `underlyingTicker` without an arbitrary JSON payload.
Absent and explicitly empty labels are distinct; Bybit's empty label and
`innovation` positively identify crypto, while absent/new values remain unknown.
Both frame consumers reconstruct these minimal `Market.info` fields for the
shared classifier and actual CLI TradFi preflight.

`DERIVATIVE_PRODUCTS` carries typed Future/Call/Put identity and known terms.
Unknown expiry/strike/settlement remain absent; only complete European cash
option terms create `Market.option`. A market replacement clears old indexed
metadata; metadata-only changes re-emit their matching market table.
`POSITION_GREEKS` uses exact position IDs and clears on position closure, so
reopening cannot resurrect stale Greeks. The optional second `ORDERS` tail
retains exact `filledInPlacementUnits` after native leaves. Inverse fills cannot
be reconstructed by dividing base fills by a quote contract multiplier. OKX
reports USD-normalized position PnL, so its frame consumer does not label that
number as native coin PnL.
