# CLAUDE.md — safe-cex/src/webworker/chaserOrder

The chaser-order manager: places, watches, and re-places orders
that follow a target price (typically chasing the bid/ask). One
`ChaserOrderInstance` per active chase; lifecycle managed by
`chaserOrderManagerInstance` (singleton).

Completed chasers are not history rows. Every internal completion path routes
through `cleanupInstance`, which detaches listeners, cancels remaining IDs, and
splices the instance from the singleton. The postprocessor also excludes a
stopped instance during the asynchronous cancellation window, so completed
chasers cannot pin full orders payloads or grow worker memory without bound.

Exchange disposal owns chaser teardown too. `ExchangeWorker.dispose` calls the
manager before disposing the exchange; matching instances are stopped, their
listeners and singleton references are removed synchronously, cancellation is
started for known IDs, and a bridged warning tells the user the chase was
interrupted. Do not defer this cleanup to a future store update.

## Hot-path: order updates are immediate; ticker-only checks are rate-limited

`ChaserOrderManager.processInstance` registers an `onUpdate`
handler on the exchange's `'update'` event. That event fires on
**every store notify**, including every ticker frame — at hundreds
of times per second on Bybit linear or Hyperliquid.

The chaser must re-evaluate on ticker-only updates. For buys, when
the best maker price moves up, and for sells, when it moves down,
the current order should be cancelled and re-placed closer to the
touch quickly. Do not gate all work on `updateIds.orders`; that
silently turns the chaser into an order-status watcher and misses
price-only moves.

Ticker-only updates can arrive in storms, so the manager rate-limits
price checks with one trailing `setTimeout` at a time
(`CHASER_PRICE_CHECK_MIN_INTERVAL_MS`). Order updates bypass that
rate limit and are processed immediately; do not delay fills,
cancels, rejects, or order removal behind the ticker throttle.

If an order update arrives while a cancel/reprice pass is awaiting network
I/O, its fill state is applied immediately but the follow-up price evaluation
is coalesced into one trailing 50 ms check. A fresh direct order update cancels
that queued check and runs immediately. Do not restore recursive
`runPriceCheck` calls from the pending pass's `finally`; they can chain
cancel/replace work without a bound during update storms.

```ts
const onUpdate = async (data: StoreData) => {
  if (instance.isStopped) { ...; return; }

  if (data.updateIds.orders !== lastOrdersUpdateId) {
    await runPriceCheck(currentOrders(data.orders));
    return;
  }

  if (priceCheckRateLimitAllowsNow()) {
    await runPriceCheck(currentOrders(data.orders));
    return;
  }

  scheduleOneTrailingTickerCheck();
};
```

Order mutations must still bump `updateIds.orders` for the
updateId-backed store bridge and other raw-order consumers, but
this manager's price-chasing path must not use that counter as an
early-return condition. See `../../store/CLAUDE.md` → "updateIds
bump on bulk replacement".

## Missing-order snapshots require confirmation

After `placeOrder` returns IDs, those IDs must become visible in `store.orders`
before the supervisor relies on normal ticker/order updates. Bybit demo can miss
private order frames while an early REST snapshot also predates the just-placed
order, leaving the UI with a pending chaser that has `currentOrderIds` but no
tracked order row. The manager therefore waits after each placement, forces a
bounded sequence of private snapshots while the IDs are missing, and only starts
or resumes normal monitoring once at least one tracked row is visible. If the
IDs still never appear, it emits a warning, marks the chase failed, and cancels
the last-known IDs by ID so the user is not left with a silent orphan.

Once a tracked order has appeared, a single wholesale snapshot that omits it
does not complete the chase. REST snapshots can be fetched before placement and
applied after the private WS frame. The manager requires two consecutive,
distinct order-updateId changes with the tracked IDs absent; ticker-only
notifications do not count, and seeing the order again resets the confirmation.
On confirmed loss it uses `cleanupInstance`, so the last known IDs are cancelled
instead of leaving a resting order without a supervisor, and it emits a
user-visible `warning`. The same rule applies to `processInstance`'s
could-not-start tail: a chaser dying without a stop reason must emit, never
vanish. Keep both loud.

## Supervisor failures are contained locally

`processInstance`, store-update handlers, queued ticker checks, and fill
handlers route thrown errors through the manager's failure path. That path
preserves an already-specific stop reason, otherwise marks the chase failed,
emits an actionable exchange error, and runs `cleanupInstance`. Do not rely on
the worker-global `unhandledrejection` listener for chaser lifecycle cleanup.

## Why the chaser subscribes to `'update'` and not direct WS events

Order events arrive via per-exchange private WebSockets
(`orderUpdates` on Hyperliquid, `order` channel on Bybit, etc.) —
shapes vary, channel names vary, dedup logic varies. The store is
the canonical normalized view, and the `'update'` event is
exchange-agnostic. The chaser doesn't need to know how Bybit
reports a fill vs how Hyperliquid does.

## Hold-steady window after partial fills

`updateOrders` short-circuits the cancel+replace path when
`Date.now() - instance.lastPartialFillAt <
CHASER_HOLD_STEADY_AFTER_PARTIAL_FILL_MS` (1000 ms). The check sits
**after** the favorable-price-move guard and **before** the
`orderPrice !== lastPrice` cancel branch, so price-moved-favorably
returns first, but a price move against us is also suppressed while
fills are landing.

Why this guard exists. The order is at the touch and actively trading
when partials are coming in. Canceling now races the in-flight fills:
the REST cancel round-trip is on the order of tens of ms, while
another fill can complete on the exchange in <1 ms. If we cancel
mid-fill, the cancel arrives after the order has fully filled —
Binance replies `-2011 "Unknown order sent"` (silent in our handler),
our local store still shows `filled` from the last partial we saw,
and `tryPlaceOrders` reads `remainingSize` based on that stale total
and places a fresh order for size that's already been filled on the
exchange. The user ends up with `original + extra-fill` notional
(roughly 1.7× a 1.0 BTC target in the wild bug report). The
hold-steady window absorbs a normal multi-tranche fill burst; if no
more fills arrive within 1 s, normal chasing resumes.

`lastPartialFillAt` is bumped by `ChaserOrderInstance.subtractFilledSize`
(WS order-status path), `addFillAmount` (Hyperliquid/GRVT fill-event
path), and `setFilledAmount` (Hyperliquid REST reconciliation) — any
path that observes a fill increment. Don't bump it for cancels or
status-only updates; those don't represent active fill flow.

This guard becomes relevant only because the per-exchange WS handlers
now mirror cumulative fill onto the store row on `PARTIALLY_FILLED`
(see `../../exchanges/binance/CLAUDE.md` → "Order Reconciliation").
Before that fix, partial fills didn't bump `updateIds.orders` for
Binance, so the chaser never saw a per-partial-fill order-update
edge; it only re-evaluated on the ticker throttle. With the partial
fills now visible, we *must* hold steady — otherwise the chaser
re-evaluates and tries to cancel inside the fill window.

## Fill-based tracking (Hyperliquid and GRVT)

For Hyperliquid and GRVT, the chaser also subscribes to the `'fill'` event
(`useFillBasedTracking`) — an order-status lifecycle can lag behind the actual
fill, and we want to mark the chase complete as soon as the fill event arrives
instead of waiting for the order to be marked `Filled` in the next snapshot.

Other exchanges rely on the order status transition.

## Cancel-replace reconciliation fails closed

After a cancel attempt, a fill-based exchange must provide a complete
`fetchFillsForOrderIds()` result before the manager places a replacement. A
cancellation can already have succeeded when its response is ambiguous; if
reconciliation is unavailable or throws (including a pagination/integrity
failure), mark the chaser failed, emit an actionable error, detach listeners,
and clean it up. Never continue to replacement on that path: an unknown fill
could overfill the user's target.

## User-cancel must tear down listeners and just-placed orders synchronously

User-initiated cancel (`ChaserOrderManager.remove` → STOP button,
`chase cancel` CLI command, `chaser:terminate` worker payload) is
the hot path users hit repeatedly. Three races used to leak fresh
resting orders past STOP, manifesting as "the chase order keeps
coming back":

1. **`tryPlaceOrders` ignored `isStopped`.** A `placeOrder` await
   that resolved AFTER the user cancelled would still hand its
   new ID back to the caller, which then reassigned
   `instance.currentOrderIds` *after* `cleanupInstance` had
   already read the stale value and spliced the instance. The
   new resting order became orphan. `tryPlaceOrders` now checks
   `instance.isStopped` at entry, after `placeOrder`, and after
   the retry sleep — and if `isStopped` flipped true while an
   `orderIds.length > 0` result was in flight, it cancels those
   IDs itself before returning `[]`.
2. **`cleanupInstance` left `onUpdate`/`onFill` subscribed.** The
   supervisor's local `cleanup` closure was only invoked on the
   next exchange `'update'` event firing — during that window an
   in-flight `updateOrders` could pass its `isStopped` guard and
   still call `tryPlaceOrders`. `ChaserOrderInstance` now exposes
   `registerDisposer` / `dispose`; `processInstance` registers the
   `cleanup` closure as a disposer, and `cleanupInstance` calls
   `instance.dispose()` first thing so the listeners detach
   synchronously. The closure's `isCleanedUp` guard keeps it
   idempotent.
3. **`cleanupInstance` filtered `exchange.store.orders`.** A
   freshly placed order whose WS frame hadn't arrived yet was
   missing from `store.orders`, so the filter-and-cancel loop
   skipped it entirely. The current shape snapshots
   `instance.currentOrderIds` synchronously, clears it, then
   cancels by ID — falling back to a `{ id, symbol }` stub when
   the order isn't in the store yet. All exchanges' `cancelOrders`
   implementations only need `id` + `symbol` to build a cancel
   request, so the stub is sufficient.

When extending the chaser, treat `isStopped` as a cooperative
abort signal at every `await` boundary that can lead to a fresh
order being placed, and register every long-lived listener /
timeout as a disposer.
