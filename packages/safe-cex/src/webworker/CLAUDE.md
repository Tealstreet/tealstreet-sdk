# CLAUDE.md — safe-cex/src/webworker

The worker that runs an exchange instance off the main thread and
bridges store snapshots back via `postMessage`. Entry point:
`ExchangeWorker`. Receives commands (`init`, `dispose`, `exec`,
`callMethod`, …) via `handlePayload`, owns one `BaseExchange`
instance plus its `DefaultStore`, and pushes throttled store
snapshots to the main thread on every `store.notify()`.

## History result completeness

Successful `callMethod:done` replies include optional `data.completeness:
{ incomplete: true }` beside `requestId` and `result` when the returned array
has the host marker. Array properties do not survive clone/JSON transports.
The matching BaseExchange receiver restores it before resolving or notifying
result observers; senders and receivers must upgrade together. Never infer a
complete window from missing metadata or attach a notice to another request.

## Write timing marks

User-write `callMethod` requests carry optional `data.timing` (write stage
marks, epoch ms), and their `callMethod:done` reply returns the merged marks.
The handler hands them to the exchange with `runWithWriteTiming`; the
main-thread caller emits the one `write.timing` event. See
`docs/plans/callers/rust-write-timing.md`.

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

## `exec` frames are untrusted bridge input

`CommandHandler` must tolerate malformed `exec` payloads. Hosted
exchange-server sessions receive frames from browser workers across reconnects
and version skew, and a missing `payload.data` used to throw while reading
`payload.data.reqId`, surfacing as a process-level unhandled rejection. Treat a
bad command payload as an `exec-log` failure instead of throwing; safe-cex does
not own the CLI layer and should not make malformed command frames page the
service.

## Ticker delta packing is browser-worker-only

`_prepareStorePayload` packs `tickerDeltas` against a mutable baseline to
shrink the structured clone across a real `postMessage`. It is gated on
`!options.hostedMode` and must stay that way.

In hosted mode the far side is exchange-server's `VirtualWorkerContext`,
which already does its own per-item partial diff of `tickers` AND throttles
payloads. Stacking the two is unsafe because they have different properties:
a full array is idempotent, so a dropped payload costs nothing, whereas
`_packTickerDeltasSinceBaseline` advances the baseline WHILE packing — so a
delta lost downstream is never re-sent. That froze the browser's ticker while
every WS topic still reported fresh, and BBO hotkeys priced orders ~60-75
points off a live book on 2026-09-14. See
`services/exchange-server/CLAUDE.md` → "Never stack two delta layers".

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

This bug was BitMEX-visible (BitMEX, since retired) because cancel is terminal: BitMEX
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

## Startup failures are identified by `worker_error_phase`, not by message text

`start()`'s catch posts `sentryTags: { worker_error_phase: 'startup' }`. That is
the only thing that marks it: the string "Error starting exchange" is the LOG
MESSAGE argument, not `error.message`, so the web sanitizer's
`worker_error_family` classifier — which reads `error.message` — cannot see it.
Before the phase tag, only bybit was findable, because
`BybitTradingMarketsRequestError` carries its own `safeCexSentryTags`.

The two tags are orthogonal and both ride: the family says what broke
(`timeout`, `network`, `rate_limit`), the phase says when. Controlled-error tags
spread last and would win a collision, so do not add `worker_error_phase` to any
`safeCexSentryTags`.

This matters because a thrown `start()` is not a cosmetic failure. Every venue
connects its public WebSocket only after a startup REST call resolves, so a
rejection can leave the exchange instance with **no public socket at all** for
its whole life — `publicInitialized` false forever, no
`messageHandlerStats.public` key, and the web watchdog then synthesizes a
`Disconnected` topic and reloads on a loop.

Exactly which rejection costs the socket is per-venue, so check before assuming:
binance awaits BOTH `fetchMarkets()` and `fetchTickers()` before connecting, so
either one loses it; bybit calls `connectAndSubscribe()` INSIDE
`fetchMarkets().then(...)`, so a `fetchTickersWithoutParse` rejection in the same
`Promise.all` still leaves a live public socket.
`auditSocketHealth` cannot rescue it: all of its cases require
`socket.ws?.readyState === OPEN`, and a socket whose `connectAndSubscribe` never
ran has no `ws`.

**The 30 s `START_TIMEOUT_MS` race rejects into the same catch**, so a timeout is
tagged `worker_error_phase: 'startup'` with `worker_error_family: 'timeout'`.
That pairing is the point, not conflation.

**The `try` deliberately spans the WHOLE of `start()`, not just
`_exchange.start()`.** Half of startup runs before the exchange exists — the
registry lookup, the dynamic `loader()` chunk fetch, the constructor,
`_initializeProcessorsAndHandlers`, `_setupExchangeEventListeners` — and those
used to reject out of `start()` altogether. The only caller
(`store.webworker.ts`) awaits it without a catch, so a chunk-load failure or an
unknown venue reached the global `unhandledrejection` handler as
`webworker-uncaught` with no phase tag, **and** skipped that caller's
`if (!started)` branch, leaving a half-initialized worker in the map. Every
failure now returns `false` and the caller cleans up. Do not narrow this back.

**A controlled error inside its own report cooldown is suppressed entirely.**
When an error carries BOTH `safeCexSentryReportKey` and
`safeCexSentryReportCooldownMs`, `logError` sets `effectiveSendToSentry = false`
until the cooldown expires — so repeated bybit startup failures compute the
phase tag and never ship it. Counts from this tag are a floor, not a census.

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
  `chaser:complete`, `grafana-info`, `grafana-error`, `grafana-pws`, `grafana-pws-sub`,
  `grafana-http-request`, `grafana-http-response`). Adding a new
  bridged event requires adding it both to the worker-side
  forwarder in `_setupExchangeEventListeners` and the main-side
  `workerListener` switch in `exchangeAccountController.ts`, plus
  `StoreWorkerOutgoingOp` in `StoreWorkerPayload.ts`.
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

## Rust engine traps and worker ownership

`engine-dead` is terminal for one native account. The worker owns recovery only
once startup has completed: it invalidates the old callback generation, detaches
its store listener, clears the trailing throttle/handlers, stops its own chasers
and exact TWAP instances, and makes one fresh account/store after a 500 ms delay.
A second panic or failed replacement stops recovery and warns the consumer.
Every fatal event invalidates loaded flags and derived active-algorithm lists,
including terminal shared-feed failures, while preserving last known main-thread
venue values until a real replacement snapshot arrives. The disposed native
adapter's local store may reset through its inherited dispose implementation.
Initial startup failures still return `false` to `store.webworker.ts` for cleanup;
explicit dispose/deactivate cancels both queued and in-flight replacement.
For a native trap, the adapter already emits the fatal error once; the worker
adds startup context as a warning log instead of reporting a duplicate error.

Every generation fences frame callbacks, store notifications, event forwarding
and handler state messages. A request already in flight may receive its one
correlated result from the old generation, preserving known write IDs or typed
unknown outcomes. A task fallback fails any interrupted executor that does not
report, and later duplicate replies stay fenced. Fresh state resets the object/delta baselines
and advances order/position freshness IDs; the adapter sends its native empty
full snapshot before starting the replacement, clearing the main decoder's
catalog/book baselines. Do not reuse a poisoned host or reset the shared WASM
module loader: other accounts in the same realm remain live.

Only current book/candle desired subscriptions survive reconstruction, keyed by
stream kind/subId and removed by subId-only unsubscribe even during recovery.
The latest UI state and active ticker symbols are also restored as safe demand. Trading,
CLI commands, method calls and algorithm state are never replayed. Requests
arriving during recovery receive a visible failure (with their existing request
ID where the protocol supports one). Interrupted writes may already have reached
the venue: recovery does not prove cancellation or authorize a retry.

A transferred shared-public-feed port belongs to the failed account and is
closed during disposal. That policy fails closed: recovery requires a fresh
provider attachment and explicit account init, rather than silently opening
direct public feeds or reusing the closed port. Shared-worker attachments during
an existing recovery retain that owner and receive a failed startup acknowledgement
and warning on the attaching port, without interrupting existing clients.

TWAP instances are tracked by actual handler-created instance identity, never
account name, so terminating one worker cannot stop another same-name account.
`TwapInstance.start()` checks termination again after its awaited placement;
otherwise a reply arriving after terminate could schedule another old slice.

## TWAP restore and the cross-tab owner lock

`twap:state` replays one tab's `sessionStorage['twaps']`, and a duplicated tab
copies it, so its own dedicated worker would run the same TWAP a second time.
Every TWAP holds the Web Locks owner lock `tealstreet-twap:<account>:<id>`
(`twap/twapLock.ts`) until it completes or stops. A restore builds the instance
synchronously (the worker tracks the TWAPs a dispatch creates), runs it only
once it holds the lock, and otherwise stops it and toasts "running in another
tab". It keeps asking for `TWAP_RESTORE_LOCK_WAIT_MS` (5 s) first: a reloaded
tab's previous worker holds its locks until the browser tears it down, and a
refusal on the first try would end a TWAP that no tab then runs. The browser
releases a dead worker's locks, so another tab can then restore it. `twapRestoreRefusal.ts` refuses finished, empty, inflated or
foreign-account rows and running rows whose schedule lapsed, mirroring the
exchange server. A runtime without Web Locks gets a no-op lock.

A slice is persisted as `state.inFlight` (its size and, for market and limit
slices, a minted `clientOrderId` sent on the order) before its order goes out,
and cleared when it is counted. A restore settles it before resuming: found by
client id counts it; a definitive miss puts the size back; anything else (an
uncertain lookup, a chaser slice, a venue without client ids) counts it with an
error toast, because placing it again could double it.

## Interrupted command replies

Normal disposal fences state immediately, then lets pending native command
rejections and acknowledgements already queued reach their original request IDs
before one task fallback. Repeated disposal preserves that grace window. Host
`ENGINE_DEAD`, `ENGINE_DISPOSED`, and `COMMAND_TIMEOUT` errors retain their
allowlisted command, timeout, caller client IDs, outcome (`notSent`/`rejected`/
`unknown`), Rust `ExchangeError` kind/venue code/message/retry delay and known
partial-order receipts through `commandOutcome.ts`; never spread arbitrary error
properties into worker messages. Known venue rejections are not relabelled as
unknown. Timeouts end the caller's wait; they do not prove cancellation or permit
placement replay.

The fallback knows only the RPC method and its arguments: dispatched
`WORKER_WRITE_METHODS` (leverage/margin/mode, `scheduleCancel`, and the
main-thread trading methods) are interrupted writes and carry the caller client
IDs in their placement arguments, while reads receive no write-outcome marker.
It must not parse CLI text or invent native IDs. `exec-result.error` and
`updatePositions:done.reason` remain strings; optional `errorDetails` carries the
original structured failure when it is still available.

Native `command-progress` is an additive typed worker event with object data
(`StoreWorkerCommandProgress`), including the host-session receipt key, native
command ID, exact typed receipt rows, completion/late flags, owning account and
original worker generation. It remains routable after the corresponding RPC
has timed out; it never depends on a pending request or enters the Store,
SCXF frames, or shared public feeds. Closed/replaced generations cannot emit
receipts. No receipt ledger or trading replay is maintained by the worker.
Existing logging consumers receive the adapter's late-receipt log fallback;
the worker does not produce a duplicate log for the typed event.

Command errors distinguish caller `clientOrderId(s)` from actual native
`orderIdentities` and `actualClientOrderIds`. A numeric `clientOrderIndex` or
`orderCode` stays an exact string in its original namespace. A receipt records
an identity selected for a write, not venue acknowledgement or proof of
acceptance; an interruption snapshot may be incomplete while native work lives.

Rust `engine-event` diagnostics use the existing generation-guarded event bridge
with `data: [EngineEvent]`. Legacy `error` options and the third `log` argument
retain the event for callers migrating in place. Consumers must choose one
channel per diagnostic to avoid double counting/toasting.
