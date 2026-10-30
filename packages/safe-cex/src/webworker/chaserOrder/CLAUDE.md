# CLAUDE.md — safe-cex/src/webworker/chaserOrder

The TS chaser-order manager: places, watches, and re-places orders
that follow a target price (typically chasing the bid/ask). One
`ChaserOrderInstance` per active chase; lifecycle managed by
`chaserOrderManagerInstance` (singleton).

Only in-process CLI callers run it (cli-core `chase`/`twap chase`, `apps/cli`).
The worker never does: its chases run in the engine's Rust algos (see
`../CLAUDE.md` → "Chasers, TWAPs and exec run only in the Rust algos").

Creation synchronizes the exchange's current native account state before
capturing its starting position. A pending frame flush must not make the chase
target start from a stale position; unavailable native state fails before any
placement. TS exchanges keep their ordinary local store read through a no-op
Base seam.

Completed chasers are not history rows. Every internal completion path routes
through `cleanupInstance`, which detaches listeners, cancels remaining IDs, and
splices the instance from the singleton, so completed chasers cannot pin full
orders payloads or grow memory without bound.

`cleanupExchange` stops an exchange's chasers before it is disposed: matching
instances are stopped, their listeners and singleton references are removed
synchronously, cancellation is started for known IDs, and a bridged warning
tells the user the chase was interrupted. Do not defer this cleanup to a future
store update.

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

Once a tracked order has appeared, a single wholesale snapshot that omits it
does not complete the chase. REST snapshots can be fetched before placement and
applied after the private WS frame. The manager requires two consecutive,
distinct order-updateId changes with the tracked IDs absent; ticker-only
notifications do not count, and seeing the order again resets the confirmation.
On confirmed loss it uses `cleanupInstance`, so the last known IDs are cancelled
instead of leaving a resting order without a supervisor.

## A chase must never die silently

Two paths end a chase without any error having been emitted: confirmed loss of
the tracked orders, and falling off the end of `processInstance` with no order
placed. Both emit a `'warning'` on the exchange emitter. Keep it that way — the
Bybit demo bug went unreported for two weeks precisely because these paths were
quiet, and the user could not tell a stopped chaser from a working one. Paths
that already emitted their own error, and user-initiated stops, stay quiet.

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

**A dead Bybit Demo private WS degrades the chase to REST reconciliation — it
never refuses it.** `getBybitDemoDeadPrivateWsReason` classifies the socket as
not running when `store.socketHealth` reports `privateReconnectPaused` (it
crossed its failure-streak threshold and stopped retrying) or
`privateInitialized !== true` after the short `waitForBybitDemoPrivateWs` grace
window. On that signal `processInstance` emits one `'warning'` naming the
degraded connectivity and the VPN / Tealstreet-IP remediation, then chases
anyway.

Refusing was tried and was wrong: absent private connectivity is an *expected*
mode on Bybit Demo in geo-restricted regions, so the gate deleted the feature
for exactly the users who needed the REST path. The fallback that makes chasing
viable without private frames is `bybitdemo` in `useFillBasedTracking` plus the
target-position completion check below.

Four things this check must NOT do, each of which broke chasers in production:

- **Do not turn it back into a refusal.** Warn and degrade; the REST fill and
  position-size paths exist to carry the chase.
- **Do not treat `privateHealthy === false` alone as a dead socket.** That is a
  socket that is down but still retrying, and it recovers within seconds.
- **Do not run the check on every store update.** It used to, so a single
  routine `1006` close stopped a live chase and cancelled its resting order —
  converting a self-healing gap into certain failure.
- **Do not treat missing `socketHealth` as a dead socket.** Absent telemetry is
  not evidence of failure; `privateInitialized` only flips true on the socket's
  first inbound frame and the store republishes health on a 10s watchdog, so a
  cold start momentarily looks dead.

**Target-position completion is the Bybit Demo fill signal of last resort.**
`ChaserOrderInstance` snapshots `existing same-side contracts + size` as
`targetPositionContracts` at construction (entry chases only; reduce-only has
its own position-gone check). `hasReachedTargetPosition` completes the chase
once the live position reaches it, because position rows still arrive over REST
polling when order frames do not. Position `updateIds` therefore also trigger a
price check, but a position-only notify must never set `confirmMissingOrders` —
it is not evidence that an order vanished.

What keeps an order safe across a WS gap is not stopping the chase — it is
Bybit's `recentlyPlacedOrders` snapshot shield (see `exchanges/bybit/CLAUDE.md`),
which prevents a wholesale REST order snapshot from erasing a just-placed order
before its first sighting.

Do not compensate by forcing Bybit Demo REST order snapshots after generic
create/amend/cancel mutations. Those snapshots can be fetched before the
chaser's just-placed order is visible and applied after it, making the tracked
order disappear from the store and causing the chaser to stop and cancel itself.
The only Bybit REST snapshot backstop that should remain is the
`SET_TRADING_STOP` path, because that endpoint returns no concrete order IDs and
is not the Chaser limit-order loop.

## A lost placement is looked up, never placed again blind

`tryPlaceOrders` mints a client order id (`exchange.mintClientOrderId`; none on
venues that refuse caller ids, which keep the old behaviour). A throw that may
still have placed the order (`utils/unknownSendOutcome.isUnknownOutcome`: a
Rust `ExchangeError` with `outcome: 'unknown'`; for unclassified errors, no
venue answer, a 5xx, Binance -1007) is looked up by that id at 1/2/4/8s. A Rust
`outcome` of `notSent` or `rejected` is definitive and never looked up:

- found **open** → the chase adopts it;
- found **filled** or **cancelled** (a post-only reject) → its fill is counted,
  then what is left is retried;
- **not found** → the chase STOPS with an error telling the user to check open
  orders. Re-placing could double the position, and the order may still land
  after the lookups (a venue honours a signed request until ~30s).

A venue REFUSAL still retries at once, and an EMPTY result gets one immediate
lookup only: most adapters return `[]` for an ordinary rejection, so the full
wait would turn every rejection into a stopped chase. An adapter that swallows a
LOST send into `[]` defeats this, which is why Bybit's single-order create now
rethrows it (`bybit/CLAUDE.md`). A STOP during the lookups gets one last look:
a found order is cancelled by the existing `isStopped` branch, and a miss still
emits the "check open orders" error. TWAP slices place through this same path.

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

`subtractFilledSize` tracks fills in the same unit as
`PlaceOrderOpts.amount`, because that is the unit used for replacement sizing.
Most adapters already expose `Order.filled` in placement units. Contract-count
adapters that publish `Order.filled` in base/display units (BloFin, KuCoin
Futures, MEXC, OKX) must set `Order.extra.filledInPlacementUnits` to their
cumulative native filled size, or a partially filled chaser can replace for the
wrong remaining amount.

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
`chase cancel` CLI command) is
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

## `chaser:complete` reports the filled size exactly once

Every ending emits one `chaser:complete` (`ChaserCompleteEvent`) on the exchange
emitter, bridged to the browser, which uses it to attach TP/SL to a finished
chase (`apps/web/src/order/chaseOrder.ts`). Three rules keep `filled` right:

- **The first `cleanupInstance` claims it on entry** and emits after its own
  cancel loop. A concurrent second cleanup finds `currentOrderIds` already
  cleared and would otherwise report before the cancel landed.
- **Listeners are detached by then**, so `emitComplete` re-reads the store row of
  every id in `everTrackedOrderIds` (not `currentOrderIds`, which
  `checkAndMarkIfComplete` clears for Closed orders) through the monotonic
  `setFilledAmount`, then — only for a chase with a `clientId`, on
  hyperliquid/grvt/bybitdemo — asks `fetchFillsForOrderIds` (Hyperliquid/GRVT
  drop a filled order's row at once; it is paged and sequential, so never for
  every chase). bybitdemo falls back to the position delta only when that REST
  read fails: the delta counts any same-side trade and over-counts.
- **It can still undercount** a fill the store has not seen yet, or one on an
  order placed while STOP was in flight (`tryPlaceOrders` cancels those itself). That is the
  safe direction for a reduce-only TP/SL; never pad it upward.
