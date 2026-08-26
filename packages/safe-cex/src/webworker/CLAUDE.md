# CLAUDE.md — safe-cex/src/webworker

The worker that runs an exchange instance off the main thread and
bridges store snapshots back via `postMessage`. Entry point:
`ExchangeWorker`. Receives commands (`init`, `dispose`, `exec`,
`callMethod`, …) via `handlePayload`, owns one `BaseExchange`
instance plus its `DefaultStore`, and pushes throttled store
snapshots to the main thread on every `store.notify()`.

## Time synchronization

The browser main thread owns public time polling. It synchronizes once per
tab, then sends the `sync-time` payload with the measured epoch milliseconds
to every local worker; `store.webworker.ts` applies it before exchange routing.
Do not make each exchange worker call the public endpoint: one tab may have
many accounts, and foreground/page-show handling already forces a fresh
main-thread sync after sleep.

## The `exchangeWorkers` registry is keyed by accountName — single-tenant only

`store.webworker.ts` keeps module-global `exchangeWorkers` /
`anonymousExchangeWorkers` maps keyed by **`accountName`** (and
`exchangeName` for anonymous). `accountName` is captured **once** from
the `init` payload into the handler closure and reused for
`getMyExchangeWorker` / `deleteMyExchangeWorker` / dispose — so the key
is whatever the `init` message carried.

This is correct for the **browser** (one device = one tenant, account
names unique, and the `ports`/`shared` machinery fans one worker out to
multiple _tabs_ of the same user). It is **NOT safe for a multi-tenant
host** that runs many clients through one process: two clients using the
same account name collide — the second `init` overwrites the first, the
first `ExchangeWorker`/exchange is orphaned (never disposed,
`isDisposed` stays false), and its `tick` signs REST forever.

**Any multi-tenant consumer (e.g. the hosted `exchange-server`) MUST
pass a connection-scoped `accountName` into the `init` payload** (it does:
`${connId}:${accountName}`). Don't "fix" that back to a bare account
name, and don't assume `accountName` is globally unique here. See
`services/exchange-server/CLAUDE.md` → "Multi-tenant invariant".

## Files

- `ExchangeWorker.ts` — the class.
- `store.webworker.ts` — the dedicated/shared Worker entry that
  instantiates `ExchangeWorker`.
- `StoreWorkerPayload.ts` — the wire-format union of every `op`
  the bridge speaks in either direction.
- `commandExecutor.ts` — parses and dispatches worker CLI commands;
  `handlers/CommandHandler.ts` owns the `exec` wire operation.
- `TelgramWorker.ts` — legacy Telegram worker bridge (filename spelling is
  load-bearing).
- `handlers/` — worker-operation handlers; there is no directory-local
  `CLAUDE.md` and no `commands/` directory.
- `chaserOrder/`, `postProcessors/` — documented subsystems with their own
  `CLAUDE.md` files.
- `twap/` — TWAP instance and manager lifecycle.

The main-thread worker fallback is
`apps/web/src/utils/shim/worker/WorkerShim.ts`; it is not part of this
directory.

## Worker → main thread bridge: structured-clone race

`_storeUpdate` runs on every store notify (throttled to 250 ms
trailing). It calls `_prepareStorePayload`, which calls
`_sanitizeForPostMessage(data)`, which **MUST clone every
reference-type field whose value drives an omit/include decision**
— at minimum `updateIds`, `loaded`, and `socketHealth`.

Why this matters. `_sanitizeForPostMessage` returns a payload
where `payload === { ...data }`. The spread is shallow; nested
object refs are shared with the live `store.state`. The
`omitOrders` / `omitPositions` decisions inside
`_prepareStorePayload` are locked in _before_ `_ctx.postMessage`
queues the structured clone. The structured-clone serialization
does not necessarily complete in the same microtask as the
`postMessage` call (browser/Node implementation detail). Between
the omit decision and the clone, another task can mutate the live
store — e.g. an inbound WS `order:update Canceled` fires
`addOrUpdateOrders` synchronously, which bumps
`store.state.updateIds.orders++`. Because `payload.updateIds`
points at the same object, the bump bleeds into the still-pending
payload. The clone snapshots
`updateIds.orders = N+1, orders = undefined` (the omit decision
was final).

The main-thread `exchangeAccountController` receives that payload,
sees `updateIds.orders = N+1`, and **advances its own
`store.updateIds.orders` to N+1 without applying any orders
array**. The _next_ payload (the one that should carry the real
canceled-orders array) gets `payload.orders` deleted by
`pruneUnchangedWorkerStorePayload` (in
`exchangeAccountController.ts`) because
`payload.updateIds.orders === store.updateIds.orders` (both N+1).
The canceled state never lands on the main thread. The UI keeps
showing the order as `open`. Subsequent cancel attempts hit
"Unable to cancel order" because the exchange has already
canceled it.

The defensive clone in `_sanitizeForPostMessage` breaks the shared
reference so a post-decision mutation can no longer mutate the
locked-in payload. **Do not remove it** — and if you add a new
top-level field to `StoreData` whose value influences an
omit/include decision (or whose value is reused on the main side
as a freshness gate), clone that field too.

The arrays themselves (`orders`, `positions`, `displayedPositions`,
`tickers`, `markets`) deliberately stay as live refs at this
stage — the structured clone deep-copies them, so mutations to
_contents_ between prepare and clone are fine (we want the latest
content). It is only the omit-decision fields where the race
matters.

This bug was BitMEX-visible because cancel is terminal: BitMEX
sends exactly one WS frame with `ordStatus: 'Canceled'` and no
follow-up. Drop that frame at the bridge and the order is frozen
open on the main thread forever. Other exchanges' order state can
self-heal via subsequent frames overwriting the stale row, which
is why this hid for a long time. The race itself is
exchange-agnostic; the same path handles every exchange's
`addOrUpdateOrders` call.

## Update-id contract (do not break)

`store.state.updateIds.orders` / `.positions` are monotonic
counters bumped by `addOrUpdateOrders`, `addOrUpdatePositionsFull`,
`removeOrders`, `update({ orders })`, `update({ positions })`,
and the other granular mutators in `store.base.ts`. The worker
bridge uses these counters two ways:

1. `_prepareStorePayload._shouldSendUpdateIdBackedSnapshot` — if
   `updateId === _lastSentXxxUpdateId` AND there are no chasers
   needing the orders payload, omit the array to save bandwidth.
2. `pruneUnchangedWorkerStorePayload` on the main side — if the
   incoming `updateIds.X === store.updateIds.X`, delete the
   incoming `X` field (it's redundant by definition).

The contract assumed by (2) is: **any payload that carries
`updateIds.X = N` must also carry the `X` array** (or the
recipient should treat the X array as unchanged from the last
N-bearing payload). The shared-ref race violates that contract by
letting `updateIds.X` advance _without_ the array following. The
fix at `_sanitizeForPostMessage` restores the invariant.

## Market-load subscription buffering telemetry

`handlers/BaseHandler.ts` buffers orderbook/OHLCV subscriptions while
`exchange.store.markets` is empty. If markets still have not loaded after 30 s,
it drops the pending subscriptions and posts an `op: 'error'` payload with
`sendToSentry: true`. This is intentionally still app-neutral: safe-cex does
not import Sentry, it only sends structured data for the web app to sanitize.

The wait-for-markets check is a self-rescheduling `setTimeout`, not
`setInterval`: each 500 ms check runs to completion before the next one is
armed, and `dispose()` clears both the next check and the 30 s drop/report
timer. Do not change this back to an interval; a blocked worker should not build
up overlapping market-load checks.

Repeated timeout reports from the same handler instance are cooldowned for 5
minutes. The subscriptions are still dropped on each 30 s timeout; only the
Sentry/log report is dampened, and the next emitted report carries the number
of suppressed timeout reports. Once markets load and buffered subscriptions
flush, the episode resets and a future outage reports normally. A recreated
worker/handler may report again because it carries new startup context.

That payload carries only bounded, non-user fields: exchange name, handler name,
pending op counts, market loaded/count state, public-data/testnet flags, and
socket-health scalars. Do not add raw account names, symbols, topics, payload
objects, or request bodies here; the web Sentry sanitizer is allowed to index
the resulting `worker_error_family=markets_not_loaded` tag.

## Other notes

- `dispose()` tears down every chaser owned by the current exchange before it
  disposes the exchange itself. This synchronously detaches listeners and
  removes singleton references, starts best-effort cancellation for live IDs,
  and emits a user-visible warning for each interrupted active chase.
- Both deactivate paths cancel the trailing store throttle and clear handlers,
  independent postprocessors, and exchange-dependent postprocessors. This
  prevents a queued throttle firing from processing a disposed exchange or
  posting an unstamped stale store payload after `deactivate-status`.
- `_throttledStoreUpdate` uses lodash `throttle(fn, 250ms,
{ trailing: true })` — leading edge fires immediately when
  the throttle window is open; subsequent calls within the window
  schedule a trailing fire. `sendSnapshot()` calls `.cancel()`
  before invoking `_storeUpdate` synchronously so init / reconnect
  paths don't race with a pending trailing fire.
- The bridge re-emits exchange events to the main thread by
  postMessaging `{op: '<event>', data: [...args]}` for a fixed
  set (`error`, `warning`, `log`, `exec-log`, `exec-result`,
  `grafana-info`, `grafana-error`, `grafana-pws`, `grafana-pws-sub`,
  `grafana-http-request`, `grafana-http-response`). Adding a new
  bridged event requires adding it both to the worker-side
  forwarder in `_setupExchangeEventListeners` and the main-side
  `workerListener` switch in `exchangeAccountController.ts`.
- Startup failures should go through `ExchangeWorker.logError(...)` once. That
  path sends structured `{ error, tags, extra }` data for the web app's Sentry
  sanitizer; do not also post a bare string `op: 'error'`, because it loses the
  original stack and creates a duplicate controller/sanitizer-looking event.
- `instanceId` on outbound bridge messages is **optional** —
  store payloads omit it. The main-side listener's
  `if (payload.instanceId && payload.instanceId !== instanceId)`
  check falsy-passes when absent, so stale-instance filtering only
  fires on payloads that explicitly stamp the id (e.g. signing
  requests). Don't add a "must have instanceId" assertion to
  store messages — that will silently drop every store update.

## Benchmarking the trade page: use the perf fixture, not page instrumentation

`perfFixture.ts` records this worker's OUTBOUND message stream and replays it
at a fixed cadence with the venue disconnected. It is the only supported way to
get comparable trade-page timings.

Two findings forced it, both worth not rediscovering:

- **Live data makes runs incomparable.** Idle blocking on one unchanged
  production build measured 18.3% / 24.0% / 25.5% / 26.6% at 6x CPU throttle.
  The spread was the venue's data rate, and it is larger than most
  optimisations are worth — anything under ~8 points cannot be seen.
- **Instrumenting from the page breaks the app.** Wrapping
  `requestAnimationFrame` and `Worker` from the page to attribute cost blanked
  the chart, orderbook, depth chart, place-order and screener modules while
  still producing plausible numbers. Only passive `PerformanceObserver` is safe
  there, and it misses frames under 50ms.

Recording sits on `ctxWrapper` in `store.webworker.ts`, not on
`_originalPostMessage` — `ExchangeWorker` posts through the former, so hooking
the latter captures nothing on dedicated workers. A replay run never
constructs an `ExchangeWorker` at all; a live venue alongside the fixture would
put the variance straight back.

Replay deliberately ignores each frame's captured timestamp and emits on a
metronome. Reproducing the original burstiness would reproduce the problem.

Driven from the page by `apps/web/src/utils/dev/perfFixture.ts`
(`?perfFixture=record`, then `__tsPerfFixture.save()` / `.replay()` / `.off()`).
Opt-in: with no URL param and no stored mode the worker stays in `off` and the
cost is one boolean per outbound message.
