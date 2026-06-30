# CLAUDE.md — safe-cex/src/webworker

The worker that runs an exchange instance off the main thread and
bridges store snapshots back via `postMessage`. Entry point:
`ExchangeWorker`. Receives commands (`init`, `dispose`, `exec`,
`callMethod`, …) via `handlePayload`, owns one `BaseExchange`
instance plus its `DefaultStore`, and pushes throttled store
snapshots to the main thread on every `store.notify()`.

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
- `WorkerShim.ts` — main-thread fallback that runs the same
  pipeline in a synchronous shim when Workers are disabled.
- `handlers/`, `chaserOrder/`, `postProcessors/` —
  per-subsystem detail; each has its own `CLAUDE.md` where the
  bar is met. The `exec` worker command is handled by
  `handlers/CommandHandler.ts` (there is no `commands/` directory).

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

## Other notes

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
- `instanceId` on outbound bridge messages is **optional** —
  store payloads omit it. The main-side listener's
  `if (payload.instanceId && payload.instanceId !== instanceId)`
  check falsy-passes when absent, so stale-instance filtering only
  fires on payloads that explicitly stamp the id (e.g. signing
  requests). Don't add a "must have instanceId" assertion to
  store messages — that will silently drop every store update.
