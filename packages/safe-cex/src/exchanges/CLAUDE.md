# CLAUDE.md — Exchange Integrations

Shared mechanics for the exchange integrations under this directory. This doc
is the single source of truth for boilerplate that would otherwise be repeated
in every per-exchange `CLAUDE.md`. **Read this first** before diving into any
individual exchange doc.

For per-exchange specifics (endpoints, quirks, auth signing details), see
the `CLAUDE.md` inside each exchange's directory.

For proxy/tunnel architecture (CORS proxy, HTTP-over-WebSocket adapter), see
`packages/safe-cex/CLAUDE.md`.

**Retired venues.** BitMEX (closed 2026-09-23) was removed in 2026-10 following the
Apex Pro pattern: directory, registry and metadata deleted, literal removed from
`ExchangeName`; stored accounts hide via `isExchangeAvailableInCurrentEnv`.
Remaining mentions of bitmex in this doc and others are history.

API factories that select HOW or another Axios transport must call
`setInnerAdapterForRemoteSigning`; direct `defaults.adapter` assignment
silently removes hosted browser-delegated signing. This applies to public and
private client factories because exchange-server may construct either with a
remote signer.

## Directory Layout

```
packages/safe-cex/src/exchanges/
├── base.ts                     # BaseExchange — every exchange extends this
├── base.ws.ts                  # BaseWebSocket — every WS class extends this
├── external-public-data.ts     # External public-data provider (multiplexing)
├── in-process-public-data.ts   # In-process public-data sharing
├── registry.ts                 # Exchange name → class mapping
├── registry.native.ts          # React Native exchange-name → class mapping
├── types.ts                    # Shared types
└── <exchange>/                 # One directory per exchange
```

This tree contains production runtime code only. Put freshly authored,
deterministic exchange regressions in
`packages/safe-cex/src/__tests__/audit-regressions/`. Do not add `__tests__/`,
`__manual__/`, `*.test.*`, or live/debug scripts under `src/exchanges/`; the
legacy corpus there was untrusted integration-generation scaffolding and was
removed rather than migrated. See `packages/safe-cex/CLAUDE.md` for the test
trust boundary.

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

### A venue method means "the venue said so" — derivation composes above it

Some venues can only produce an axis by folding another one: hyperliquid has no
closed-position endpoint, but its fills carry `closedPnl`, so closed positions are
derivable. **Do not put that fold behind the venue method's name.** `fetchClosedPnl`
means exactly one thing — the exchange reported this — and laundering a fold through it
gives polymorphic parity at the cost of making the two indistinguishable at every call
site afterwards.

That distinction is not cosmetic. Reported closed PnL is low-multiplicity with error
local to the row. A fold of fills is **path-dependent**: one bad fill corrupts every
position after it, and it cannot validate the axis it came from — comparing them compares
data to itself and agrees by construction.

The pattern:

- venue method (`fetchClosedPnl`) — reported only, branded base stub when absent;
- venue derivation (`deriveClosedPositions`) — explicitly named, clearly a fold;
- `utils/resolveClosedPositions.ts` — composes them, returns `{ entries, provenance,
independentlyVerifiable }`.

**Provenance rides ON the data** (`ClosedPnlEntry.provenance`), not in a capability table.
Derived data is indistinguishable from reported data at the call site — a fold of fills
does not look derived, it looks like a closed position — and a table someone must remember
to consult is the same shape as the former `supportsTradesHistory`, which drifted to
advertising `true` on three venues that always returned empty and `false` on one that
worked, precisely because nothing in production ever read it. It was deleted 2026-08-16.

Consumers branch on provenance, never on whether a method exists:

| Consumer                                   | Contract                                                                                                                          |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| Journal / UI                               | renders either, but must not present `derived` rows as verified, and should not back an export from them without a visible caveat |
| Validators (`--closed-pnl`, health checks) | require `independentlyVerifiable`; SKIP with a reason otherwise                                                                   |
| Anything counting "trades"                 | `derived` granularity is per closing FILL; venue-reported is usually per closed POSITION                                          |

The resolver also fixes the duplicate-fetch problem the naive shape creates: two sibling
methods that each fold fills fetch fills twice, whereas the resolver fetches once.

### Before declaring a venue unsupportable: enumerate the VENUE's API, not ours

**What this tree implements is a statement about our backlog. It is never evidence about the
exchange.** That inference was made three times in one survey and was wrong every time —
woo was called UNSUPPORTABLE, phemex and kraken were recorded as having no history axis. All
three publish full history surfaces; see `docs/VENUE_COVERAGE_SURVEY_v1.md` → "A fourth trap".

Two failure shapes, both of which look like findings once written down:

1. **A missing method read as a missing capability.** `kraken.exchange.ts` implements none of
   the five history methods, so the axis probe reports none — while the venue serves
   `history/v3/account-log`.
2. **A measurement generalized past the route it measured.** woo's `realizedPnl` really is
   `null` on 534/534 fills — of `/v3/trade/transactionHistory`, the one route the adapter
   reads. The v1 ledger next to it carries `REALIZED_PNL` and `FUNDING_FEE` rows.

The second is the dangerous one, because a live number makes the conclusion feel earned.
**A measurement's scope is the route it measured.** Before writing "this venue cannot", read
the venue's docs or the CCXT reference (`../CLAUDE.md` → Reference) and cite the source.

**The axis is often on a different API generation than the adapter uses**, so re-reading the
adapter's own routes can never surface it:

- woo — adapter on `/v3/trade/transactionHistory`; realized PnL is on the **v1** family.
- kraken — adapter entirely on `futures.kraken.com/derivatives/api/v3`; the ledger is on
  `futures.kraken.com/api/history/v2`, a different base URL **and** version.

**Mixing versions inside one adapter is normal here, not a workaround** — bitget already runs
33 `/api/v2` routes alongside 3 `/api/v3`. Implement the hybrid; do not port the adapter to a
new generation wholesale just to reach one endpoint, and do not conclude the endpoint is
absent because the generation the adapter happens to speak lacks it.

### Paging: the "first page of a sorted list" family

Four defects in this repo have been the same mistake wearing different clothes — **drawing a
conclusion about a whole set from one page of it, without establishing the sort direction**:
an activity probe called a bitget account idle off the OLDEST 100 orders; the first weex
pager walked a backwards cursor and stopped after one page against an ascending venue; the
income pager inferred which end of a window had been served; and okx/blofin derived their
cursor from `rows[rows.length - 1]`.

Rules for any new paging code, and for the 9th and 10th ledger axes especially:

- **Never take a cursor by array position.** Use `utils/oldestCursorId` (smallest id in the
  page, order-agnostic). It compares numeric strings by length-then-lexicographic rather than
  parsing — okx billIds exceed `MAX_SAFE_INTEGER`, so `parseInt` collapses distinct ids.
- **Completeness is a SPAN question, not a row count.** `page.length === limit` is a guess
  about the venue: a server-side filter or a cap below the asked limit returns a short page
  with more behind it. `exchanges/income-paging.ts` decides by whether the requested window
  has been covered, and terminates a branch only on an empty page or on no fresh rows.
- **A failed page is not an empty one.** Give a pager a RETHROWING fetcher; the public
  `fetchIncomeHistory` swallowing into `[]` makes a 429 indistinguishable from an idle
  window. Measured on binance: 492 of 512 records returned while reporting success.
- **Dedupe on a key unique per RECORD, not per row.** A ledger row commonly yields both a
  realized-PnL and a commission record. okx/bybit/bitunix/hyperliquid/bitget/bitmex all
  suffix (`${id}:pnl` / `:fee` / `:adj`); bitget and bitmex did not until the collision
  discarded every bitget commission — 820 records where the venue's own row census predicted
  ~1190.
- **Verify against a venue-supplied census or running balance, not a self-consistency
  check.** The live paging probe compares two walks through the same mapper, so it PASSED at 820. Arithmetic against the row census caught it; bitget's `balance` column is what pinned
  its gross-vs-net convention.

**Known-latent, deliberately unchanged:** okx (`bills`, `orders-history`, `closed-pnl`) still
adds `rows.length < PAGE_LIMIT` as an early stop alongside its cursor check, and
blofin/bitunix do the same behind `added === 0` / `freshRows === 0` guards. These are
correct while those venues only return a short page at the end, which is true today. They
were left alone because changing paging on venues that currently PASS, without an
independent census to verify the change against, risks more than it fixes — but do not copy
the idiom into a new venue.

### `<name>.<concern>.ts` — extract pure mappers, don't inline them

Beyond the five files above, a venue may carry extra single-concern siblings
(`bybit.order-params.ts`, `bybit.response.ts`, `hyperliquid.builderFee.ts`,
`hyperliquid.order-inference.ts`, `bybit.income.ts`, `okx.income.ts`,
`bybit.closed-pnl.ts`, `hyperliquid.income.ts`, …). This is the normal shape —
not a special case — and for response mappers it is close to mandatory:

**Exchange methods are instance fields** (`fetchIncomeHistory = async () => {}`),
so a parser written inside one is unreachable from a test. That is not a
theoretical cost. The WOO hedge-side bug silently dropped 42% of returned rows
and lived behind an instance-field parser where no unit test could reach it; the
fix was to extract `mapWooHedgeTransactionSide` and only then assert on it.

So: any function that turns a venue response into a domain type — fills,
ledger rows, closed positions, orders — goes in its own file, is `export`ed, and
is pure (no `this`, no I/O). Pass what it needs, including store lookups, as
arguments (`resolveSymbol: (id) => string | undefined` is the usual shape).

This matters most where the call site erases types. `ClosedPositions.tsx` casts
`as unknown[] as ClosedPosition[]`, so TypeScript catches **no** shape error in
`mapBybitClosedPnl` — a side inversion or a passed-through decimal string
compiles cleanly and is wrong at runtime. The extracted function plus a
deterministic regression is the only thing standing there.

## BaseExchange Contract

`base.ts` defines the `Exchange` interface and `BaseExchange` class. Every
exchange class extends `BaseExchange` and overrides the methods it needs.
Most unsupported action/fetch methods throw `'Not implemented'` and fail at
runtime if called. The base implementations of `fetchOHLCV`,
`fetchHistoricalFills`, `fetchIncomeHistory`,
`fetchIncomeHistoryRange`, `fetchHistoricalOrders` and `fetchClosedPnl` deliberately
resolve to empty arrays **and are branded**, so a caller can tell that empty apart from a real one — callers must
check `exchange.capabilities` and convert, rather than storing the `[]`
(`requestExecutions` returns `undefined`, `collectAccountAxes` records
`unsupported`);
`updateSymbolMetadata` is a no-op; and `listenOHLCV`, `listenOHLCVs`, and
`listenOrderBook` return without subscribing unless an external public-data
provider is configured. Missing overrides for those methods therefore fail
silently and must be checked explicitly when adding support.

Key methods exchanges typically override:

- `start()` — bootstraps the exchange (markets → tickers → WS → private
  state). Default throws.
- `fetchMarkets()`, `fetchTickers()`
- `fetchPositions()`, `fetchBalance()`, `fetchOrders()`
- `placeOrder()`, `updateOrder()`, `cancelOrder()`, `cancelOrders()`
- `setLeverage()`, `changePositionMode()`, `changeMarginType()`
- `fetchOHLCV()`, `fetchHistoricalFills()`

Check `Exchange` interface in `base.ts` for the full surface.

`Ticker` keeps derivatives fields (`mark`, `index`, `fundingRate`,
`fundingTime`, and `openInterest`) required for compatibility. Spot venues and
venues that do not publish one of those values use `0`; consumers must treat
that value as "zero or unavailable," not proof that the upstream value is
genuinely zero.

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

`BaseExchange.rejectPreflight(message)` is the shared helper for this shape —
emit first, then throw. Prefer it over a hand-rolled pair in new checks. Caller
`clientOrderId` validation is built on it (`resolveCallerClientOrderId` /
`assertCallerClientOrderIds`); every `placeOrder`/`placeOrders` override must
call those first — see `packages/safe-cex/CLAUDE.md` → "Client order ID
contract".

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
`packages/script-api-core/src/order/downSizeReduceOrder.ts` util inside the web order
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

**Carve-out — bitget's plan-type selection.** `selectPlanTypeAndAmount` in
`bitget/bitget.exchange.ts` clamps a sized stop to the protected position. That
is **not** a violation of the rule above and must not be "cleaned up" into the
flag path. Bitget's wire format forces a choice the flag cannot express: a stop
is either `loss_plan` **with** a size or `pos_loss` **with the size field
omitted**, so declining to clamp does not mean "send the requested size" — it
means silently converting a deliberately-sized stop into a whole-position one.
The two things `downSizeReduceOrder` adds over a bare `Math.min` are already
handled or moot there (side matching is exact; orders-ahead is meaningless for
trigger orders). See `bitget/CLAUDE.md` → "Plan Type Determination".

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
  RE-open (`BaseWebSocket.trackConnected`) with `tries=10`. On a disposed
  account (browser gone / remote signer dead) it would retry `fetchOrders()`
  10×, each signing a REST request that throws "Remote signer unavailable" —
  flooding logs/CPU indefinitely. It now checks `isDisposed` at entry, each
  loop iteration, and after the `await`.

This was a real exchange-server incident: disposed accounts looped
"Remote signer unavailable" ~10×/s for 15+ min after disconnect (9k+ errors),
contributing to 9-wide e2e "stuck" load failures via signer/CPU contention.
**If you add another reconnect-triggered REST/auth path, gate it on
`isDisposed` the same way** — a disposed account must never sign again.

`BaseExchange.dispose()` also unsubscribes the store-to-emitter bridge before
resetting the store and removes the remaining exchange-level lifecycle,
market-change, signer-status, log, and Grafana listeners. Consumers may retain
an emitter reference, but a disposed exchange must not keep its store callback
or notify those listeners after teardown.

### Reconnect caps are streak-scoped, not lifetime

The `> 100` guards on `BaseWebSocket.onCloseCount` / `reconnectCount` (mirrored
in the Lighter ws classes and `external-public-data.ts`) bound a single
**unhealthy streak**: they reset once the connection is confirmed healthy again
(base: the first-message branch of `trackLastReceived`; each mirror: its own
healthy-traffic point). Don't turn them back into lifetime counters — long-lived
sessions (flaky networks, 24h WS session limits) legitimately accumulate 100+
closes, and a lifetime cap silently kills the socket forever.

**`lifetimeCloseCount` exists precisely because those counters reset.** Any
observer that samples _after_ recovery — an error reporter, a support bundle —
reads `onCloseCount`/`reconnectCount` as 0 and concludes the socket was fine.
`BaseWebSocket.lifetimeCloseCount` is monotonic and never reset, alongside
`lastCloseCode` / `lastCloseAt`, which survive reconnect. Don't "tidy" these
into the streak counters; they answer a different question.

These surface to consumers through `store.socketHealth` (aggregated by
`summarizeSocketDiagnostics` in `base.ts`), which already crosses the worker and
exchange-server boundaries. **That is deliberately the only channel** — safe-cex
ships in the `tealstreet-sdk` OSS mirror, so it must not gain a dependency on
any app's telemetry. Add fields to the existing struct; do not import a
reporting client here.

Two shape notes: `*ReconnectPaused` is a boolean, not a remaining-ms countdown,
because a countdown would mutate `socketHealth` on every watchdog tick and
manufacture store deltas for the whole pause — and because hosted mode stamps
these on the server's clock. `ExternalPublicDataProvider` is not a
`BaseWebSocket` and is absent from `getPublicSockets()`, so gateway-mode
accounts report no close code even though the gateway drives `publicHealthy`.

**`recoverAfterOffline` is the reconnect for "the browser came back online".**
An offline spell leaves a socket in states a plain `reconnect()` cannot clear: a
pending 120s pause timer (`reconnect()` with no `ws` falls into `onClose`, which
skips while `reconnecting`), a FRESH pause (closing a socket mid-connect counts
another failure), or `reconnecting` stuck after the 100-close cap. It resets the
streak, backoff and caps and then acts on exactly one state: a pending timer runs
now; a socket that still looks open (usually half-open) is closed and its OWN
close event drives the close path; a socket stuck past the cap reconnects.
**Do not detach a socket here to skip waiting for its close event**: Kraken,
Bitunix, MEXC and AsterDEX public register their own close/error closures, which
stay on a detached socket and tear down the replacement when its late close
arrives. Anything else is a connect in flight and is left alone, because a second
connect leaks the first socket. A `ReconnectingWebSocket` is reconnected only while
it reports OPEN: during the library's backoff wait, `reconnect()` runs two chains. A socket
with its own close loop sets `recoversAfterOffline = false` (Lighter, Polymarket,
HyroTrader's private socket); the exchange then returns `false` and the web
reloads that account instead. A connect that fails before any socket exists
(listen-key or token fetch) goes through `retryFailedConnect`: one toast per
outage (its own flag, cleared with the failure streak — not the streak itself, which
a dropped socket has already set), then the close path's backoff — BingX keeps its own listen-key recovery.

Native-WebSocket reconnects use close-code-scoped exponential backoff. A
different close code or a failure-free window longer than twice the maximum
delay resets growth to the base delay. Eight consecutive non-normal close
codes activate the two-minute reconnect pause; healthy inbound traffic or a
normal close resets that failure streak.

External public-data gateway connections have their own liveness loop because
they are not `BaseWebSocket` instances. The provider sends a gateway `ping`
after 6 seconds without inbound traffic and closes the socket after 20 seconds
so its reconnect path recovers half-open connections. Any inbound frame,
including `pong`, satisfies the heartbeat; disposal clears its timer. The
provider exposes the same initialized/healthy/last-message surface consumed by
`BaseExchange.syncSocketHealthToStoreInternal`, so gateway-only public data is
reflected in `store.socketHealth` and the reconnect banner instead of being
reported healthy by the empty direct-socket fallback. Candle subscription keys
split at the final delimiter during reconnect because normalized symbols may
themselves contain `:`.

Routine socket churn is not an exchange error: ordinary disconnect/reconnect
messages use warning severity, and heartbeat frames dropped while a socket is
closed use debug severity. Error severity remains reserved for terminal give-up
paths such as reconnect-count exhaustion.

The exchange watchdog measures first-message grace from each socket's latest
successful `open` event. A previously initialized socket resets
`lastReceived` while reconnecting, so `connected + initialized +
lastReceived=0` is expected during that grace window and must not be bounced
immediately based on the exchange object's older construction time.

When a socket is not open, `BaseWebSocket.send()` drops only protocol heartbeat
frames: literal `ping`/`pong`, JSON heartbeat operations (`op`, `type`,
`event`, or `method`, including `server.ping`), or JSON frames with a top-level
`ping`/`pong` key. Do not broaden this to a substring match; subscriptions and
orders may legitimately contain tokens such as `SPONGE` or client identifiers
containing `ping`, and those frames must trigger the reconnect path.

Every subclass `onOpen` must preserve the base lifecycle contract: public
sockets call `emitWsReconnected()` so consumers resubscribe. Private state is
resynced by the base: the open listener every socket gets in `createWebsocket`
(`trackConnected`) calls `parent.fetchAndSetPrivateSnapshot(10)` on every
RE-open of a socket whose `resyncsPrivateStateOnReopen` is true — the
exchange's `privateWebsocket` by default; OKX's business socket (algo orders)
opts in, HyroTrader's opts out (it recovers after acknowledged auth). It is
coalesced per exchange over 2s.

**This replaced `setupHouseKeepEventListener`, which healed almost nothing.**
It ran only from the native `processClose` path, so every Tealstreet-hosted,
relay or header socket (the `ReconnectingWebSocket` path) never resynced, and
on venues that connect after an `await` (listenKey/token: Binance, Binance PM,
BingX, Bitrue, KuCoin, AsterDEX) it attached to `undefined` and never fired.
Orders on OKX via proxy, Bybit via relay, GRVT, Bitget's fallback host and
Bitrue stayed stale until an unrelated event. Several venues still gate their
own `onOpen` snapshot behind `ENABLE_RECONNECTING_WEBSOCKET`; those gates are
now redundant, not load-bearing. MEXC and WEEX resync in `onOpen` too, so a
re-open there fetches twice — harmless.

### virtualClock

Don't use `Date.now()` for request timestamps. Use
`virtualClock.getCurrentTimeMs()` (imported from
`packages/safe-cex/src/utils/virtualClock.ts`) — it accounts for clock drift
between the client and the exchange server, which is critical for HMAC
signatures with strict `recvWindow` enforcement.

### Remote-signer WS auth — a new `outputFormat` MUST ship a signer

Private-WS auth under a remote signer has two halves that live in different
files:

1. the **request**, in `<name>.ws-private.ts`, which names an `outputFormat`
   from the union in `types.ts` (`RemoteSignerRequest.wsAuth`);
2. the **signer**, `BaseExchange.signWsAuth`, which actually produces the
   signature.

`signWsAuth` used to narrow anything that wasn't `'hex'` or `'base64'` down to
hex HMAC-SHA256, so adding a union member and using it compiled cleanly, passed
review, and produced a wrong signature at runtime — no error, the venue just
rejects the subscription and the private feed (fills, orders, positions) stays
dead while REST polling keeps the UI looking healthy. Kraken shipped that way;
so did blofin (`hex-base64`) and apexomni (`apexomni-base64`).

It is now an exhaustive `switch` ending in `const unhandled: never =
outputFormat`, so **adding a member to the union without a signer is a compile
error.** Implement the new branch there, and make it match the venue's local
(non-`remoteSigner`) expression byte for byte, then add a case to
`base.ws-auth-signing.test.ts`. Its expectations are golden vectors generated
with Node's built-in `crypto` rather than re-derived with the implementation's
own expression, so a shared misreading of a venue's convention cannot pass.

Override `signWsAuth` on the exchange class only when the scheme is not
HMAC-SHA256 over the secret at all. Kraken is the worked example —
`kraken.exchange.ts` implements
`base64(HMAC_SHA512(SHA256(challenge), base64Decode(secret)))`, mirroring both
`performAuth()`'s local path and the REST scheme in `kraken.api.ts`. Reaching
the base signer with `kraken-challenge` now returns an explicit error rather
than a plausible-looking wrong signature.

Two related traps in the same area:

- **Don't gate the auth handshake on `options.secret`.** Hosted mode runs with
  `secret: ''` by design (the secret never leaves the client), so a
  `if (!options.secret) return` guard makes the remote-signer path unreachable
  and kills the private feed with only a warning. Gate on
  `secret || remoteSigner`, and only where the secret is genuinely required.
- **This class of bug is hosted-mode-only**, so it does not reproduce in local
  web dev, where a real Worker holds the actual secret. Verify private-WS
  changes against exchange-server, not just `yarn dev`.
- **`await` the auth call if `subscribe()` follows it in the same function.**
  `auth` is `async`, and on the LOCAL path that is harmless: an async body runs
  synchronously to its first suspension point, and the local path computes an
  HMAC with no `await` before its send, so the auth frame goes out first. Under a
  remote signer the FIRST statement is an await, so an un-awaited `this.auth()`
  yields and `subscribe()` reaches the wire on an unauthenticated socket. The
  venue refuses the private topics and the order/position feeds never start —
  orders stop updating while REST polling keeps the UI looking alive.
  `b7b6822f95` fixed bybit linear and missed `bybitinverse`/`bybitspot`, which
  is why bybit USDT worked in hosted mode and BTCUSD did not. Measured on
  bybit's own wire, 2026-10-01: a `subscribe` sent before any `auth` is answered
  `{"success":false,"ret_msg":"Request not authorized","op":"subscribe"}`, and
  the same subscribe after a successful auth is answered `success:true`.
- **The refusal is LOGGED, not silent** — `onMessage`'s `json.success === false`
  branch emits it — but nothing retries, because the auth-retry arm is gated on
  `json.op === 'auth'`. So the symptom is a stuck orders table plus a
  `[WS]subscribe:Request not authorized` line, which is the string to grep for.
- **On a FAILED auth, send nothing at all and `reconnect()` — do not subscribe.**
  This is the half that is easy to get wrong in both directions, and both were.
  `RemoteSigner.sign` **throws** — signer unavailable, the 15s `SIGN_TIMEOUT_MS`,
  a failed send, `markUnavailable` rejecting everything pending — rather than
  resolving `{ error }`, which is the only case `auth` itself handles. So the
  await can reject, and whatever happens next is the whole design:
  - Letting it escape is an unhandled rejection: the `onOpen` caller does not
    await. Catch it, and pass the error ALONE — for an `Error` input a second
    `emitAndLogError` argument REPLACES the message (`base.ts`), so naming it
    there destroys the only description of the cause and collapses four distinct
    failures into one line.
  - **Never put the words "auth failed" in that message.**
    `health/errorPatterns.ts` lists that literal under `auth_invalid`, so the
    user is told their API key is rejected, and
    `noteTerminalExchangeFailure` suppresses automatic account reloads for five
    minutes — the very reload that would re-pair the browser signer. A backgrounded
    tab must not look like a bad key. (`auth`'s own
    `Remote WS signing unavailable:` string exists for this reason; every
    remote-signer private-WS site now emits that wording, so a new venue must
    not reintroduce `Remote WS auth failed:`.)
  - **And do not "recover" by subscribing anyway.** That was tried, on the theory
    that the venue's refusal frame would arm the staleness watchdog. It does the
    opposite. `base.ws.ts trackLastReceived` sets `isInitialized` on the FIRST
    inbound frame, and bybit answers a ping on an unauthenticated `/v5/private`
    socket — measured 2026-10-01, 7 pings, 7 pongs — so `lastReceived` keeps
    refreshing, `checkLastReceived`'s 20s threshold never trips, `reconnect()`
    never fires, and a socket holding ZERO topics reports healthy through
    `store.socketHealth` indefinitely. That is the original bug's failure mode,
    not a recovery from it.
  - Sending nothing and calling `reconnect()` is what works, and the reason is
    BOUNDED AUTOMATIC RETRY — not honest health. Health is NOT honest here: a
    private socket that never initialized is filtered out of `base.ts`'s
    `initializedPrivateSockets`, and `privateHealthy` short-circuits true on an
    empty list, so this still reads connected; `exchangeAccountController`'s
    `privateBroken` also requires `privateInitialized`, so the automatic reload is
    suppressed too. It is still strictly better than subscribing anyway, which had
    no retry at all.
  - **Do not describe the retry bound as the socket's `> 100` reconnect cap.**
    `reconnect()` goes through `ws.close()` -> `processClose`, so
    `registerCloseResult`/`getReconnectDelay` pace it 2 -> 4 -> 8 -> 16 -> 30s
    (`WS_BACKOFF_MAX_DELAY`) with a 120s pause after
    `WS_FAILURE_STREAK_PAUSE_THRESHOLD` (8) consecutive failures. And in hosted
    mode the bound that actually fires first is the signer's own
    `MAX_CONSECUTIVE_FAILURES = 10` (`services/exchange-server/src/remote-signer.ts`),
    which force-disposes the account; the 100-cap is unreachable. A sleeping laptop
    also closes the browser socket, which disposes every account server-side, and
    the next init on wake is a fresh session — so recovery is automatic and needs
    no reload.
  - Tell the host chain the silence was OURS (`WsDirectFallback.disregardSilence()`).
    A connect that exchanged no frames otherwise reads as a broken egress path, and
    two of those advance onto the relay and then the athena passthrough for the
    socket's LIFETIME — putting every signer-less user on one shared IP against the
    venue's per-IP cap, which that class exists to avoid. A genuinely silent connect
    still escalates, and a real inbound frame still resets the streak, so a wrong
    API key still cannot trip it.
    Gated by `__tests__/audit-regressions/bybit.private-ws-auth-ordering.test.ts`,
    which covers the ordering, the exact subscribe args, the local-HMAC path, both
    signer failure modes, and that a disposed exchange never asks the signer.
    `__tests__/audit-regressions/private-ws-auth-failure-consequences.test.ts` gates
    the two consequences the socket file cannot show: the host chain does not
    escalate when the silence was ours, and none of the strings these sockets emit
    classifies into a terminal auth family (with the old wording as a live control).
    **This only applies where `subscribe()` is called straight after auth.**
    bitget, blofin, okx (private and business) and phemex all subscribe from their
    login/auth RESPONSE handler, so they are unaffected by the ORDERING bug — but
    not by the rest of this entry: bitget, blofin and phemex still `ping()` from the
    open handler beside an un-awaited `auth()`, so a pong sets `isInitialized` and a
    zero-topic socket reports healthy there too. **okx is FIXED**: its `onOpen`
    awaits `auth()` (now returning a boolean), and on a thrown or `{error}` signature
    sends nothing, `disregardSilence()`s and reconnects — gated by
    `__tests__/audit-regressions/okx.private-ws-auth-failure.test.ts`. The `Remote WS auth failed:` wording that
    `auth_invalid` matches has been swept out of every remote-signer private-WS
    site in this tree; a new venue must not reintroduce it. **apexomni had the same shape and is FIXED**
    (`598a9553f4`): its `onOpen` is now `async` and awaits the login. Read
    `apexomni/CLAUDE.md` before touching it — `d1184f3d8e` reverted three bundled
    things after code-1005 churn and awaiting the signature was none of them, but
    Apex's own tolerance for a pre-login subscribe is still unmeasured because there
    is no funded Apex test key.

### Signature rejections get one re-signed resend

A venue rejecting a request's SIGNATURE (Bybit 10004, Binance -1022, …) refused
it, so nothing executed and a write is as safe to resend as a read. These come in
bursts after a reconnect on keys that work, so `utils/signatureRejection.ts`
resends once before anything counts it as a bad key. Wire BOTH paths with the
venue's `IsSignatureRejection`: `signatureRejectedResponse` in the axios-retry
condition (a rejected non-2xx), and `resendOnSignatureRejection` in the LAST
response interceptor (a 200 envelope, or any status over the HOW tunnel, which
resolves 4xx). Before wiring, make the signer resend-safe: `config.data` is a
string by then, and the first pass's signature/timestamp are still on the config.
Skip venues whose rejection cannot be told apart from a bad key, and wallet/Stark
signers, where a rejection is not signature drift.

| Venue | Rejection | Notes |
| --- | --- | --- |
| Bybit family, HyroTrader | `10004` / `/error sign/i` | 200 envelope |
| Binance USD-M / PM / spot, Bitrue | `-1022` (`isBinanceStyleSignatureRejection`) | 4xx; Bitrue also over the tunnel |
| AsterDEX | `-1022`, V1 (HMAC) paths only | V3 is a wallet signature; rebuilt from `__asterUnsignedParams` |
| MEXC | `602` "Signature verification failed!" | 200 envelope |
| BingX | `100001` | 200 envelope, tunnel by default |
| OKX | `50113` "Invalid Sign" | 401 or 200 |
| Bitget | `40009` "sign signature error" | 4xx |
| KuCoin futures / spot | `400005` "Invalid KC-API-SIGN" | 401; tunnel when authenticated |
| Bitunix | `10007` "Sign signature error" | 200 envelope |
| Kraken spot | `EAPI:Invalid signature` in `error[]` | 200 envelope; replay re-signs its form params under a new nonce |
| Crypto.com | `40101` | Shared with a bad key, so a bad key costs one extra request |

Not wired: Coinbase (JWT; a 401 does not say which half failed), Kraken futures
(`authenticationError` covers a bad key too), BloFin and WEEX (no documented REST
signature code — their signers are still resend-safe), and the wallet, Stark and
Ed25519 signers (Hyperliquid, Lighter, Paradex, GRVT, Polymarket, Orderly,
AsterDEX V3, Backpack).

A venue whose response interceptor is registered after axios-retry sees a retried
response twice: once in the resend's chain, then in the original's. Keep it
idempotent — Crypto.com's result unwrap and Binance's error emit both had to be.

### A resend must be re-signed

Retries replay a config: axios-retry calls `instance(error.config)`, and venue
interceptors resend `response.config` (Bybit's clock-sync and sign retries). In
hosted mode that config must still name `createRemoteSigningAdapter` as its
adapter, or the resend skips the device and goes out under the previous
attempt's signature while the request interceptors change the request around it
— Bybit adds a fresh `requestId` per pass, so it answers 10004 "Error sign".
Without an inner (HOW) adapter the signer executes through axios' default
adapter list; `executeThroughDefaultAdapters` hands back the incoming config on
both the response and the error for exactly this reason. Do not "simplify" it
back to mutating `finalConfig.adapter`.

### Public endpoints MUST be tagged so hosted mode skips the signing round trip

The hosted exchange-server holds no secrets, so `createRemoteSigningAdapter`
sends every request over a WebSocket to the user's device, waits for it to sign,
and only then hits the wire. That adapter runs **after** every request
interceptor, so it cannot see the `isPublic` branch your interceptor already
took, and round-trips regardless. Measured on a hosted mobile session: 19 sign
requests on connect spanning ~3.6s, four of the first six of them public,
gating the markets and tickers lanes behind a signature nobody wanted.

Two things can be true of such a request, and they differ in cost:

- **Your interceptor already skips signing it.** The device replays it through
  that same interceptor, reaches that same branch, and hands back the unsigned
  config that went out. Pure latency for a guaranteed no-op — tagging is free.
- **It is public but your interceptor signs it anyway**, because the public test
  is narrower than the public surface (OKX and Bybit `/market/*`). Tagging means
  the hosted request now goes out unsigned where it used to be signed. Both
  those venues meter public market data per-IP regardless, so nothing is given
  up; on a venue that credited it to the account, this is the case to weigh.

Tag those requests with `markSkipRemoteSigning(config)` from
`../remoteSigningAdapter`. **Tag by falling through — never return on the tag's
behalf.** The tag says only "a keyless server need not ask the device"; it says
nothing about local signing, and a client that holds the key must keep signing
exactly what it signs today, since venues that meter signed requests per-account
and unsigned ones per-IP give you real headroom for free.

```ts
if (isPublicEndpoint(config.url)) markSkipRemoteSigning(config);
// existing branches unchanged below
```

Three rules, each of which has already bitten:

- **Tag on the url or body, never on a credential-shaped condition.** Hosted
  mode is handed the key but never the secret, so `!options.secret` is true for
  _every_ request it makes. Tagging on the common
  `if (!options.key || !options.secret) return` shape would mark real orders as
  needing no signature and ship them unauthenticated. `publicDataOnly` is the
  same trap on a public-data instance. Where an exchange ORs its URL test
  together with one of those — `blofin`, `coinbase`, `krakenspot`, `kucoinspot`,
  `weex`, and kraken futures, which is the worked example — split the URL term
  out first.
- **Tag the object every `return` derives from.** Bybit builds
  `nextConfig = {...config}` and returns spreads of it; a tag on `config` is
  silently dropped and the round trip quietly comes back.
- **Check the exchange's public test actually covers its public data.** Most are
  narrower than they look — OKX matched only `includes('public')`, missing
  tickers and both candle endpoints under `/api/v5/market/`; Bybit matched kline
  by exact equality, missing tickers and instruments-info. Widen the tag
  predicate without touching the signing condition.

Cover it with an interceptor-level test — see
`__tests__/kraken.public-endpoint-tagging.test.ts` and its OKX/Bybit and
Hyperliquid siblings. Drive the real interceptor through a capture adapter and
assert both directions: public tagged, private never tagged **in the hosted
shape** (`{ key: 'k', secret: '', remoteSigner }` — the signer is what makes it
hosted, and the blank secret is what makes the hazard live), and where the
endpoint is public but locally signed, that it is tagged _and_ still carries its
signature header.

Tagged today: kraken futures, okx, bybit (+spot/inverse, shared `createAPI`),
hyperliquid perps (both instances), hyperliquidspot, and backpack. Untagged
exchanges keep today's behaviour — absence means "sign it" — so this is
incremental. The free wins remaining are the ones whose interceptor already
decides not to sign a public URL, and so pay the hop for a guaranteed no-op:
binance (+pm/spot), apexomni, asterdex, bitunix, blofin, coinbase, cryptocom,
krakenspot, kucoinspot, orderly, paradex, polymarket, weex, woo. Some
early-return; cryptocom and polymarket instead compute a URL-derived
`requiresAuth`/`isPublic` and fall past the signing block. Either shape is fine
to tag.

An exchange with no per-URL **market-data** test (bitget, phemex, bingx,
bitrue, kucoin futures, grvt, breakoutprop) needs that distinction established
first, which is a signing change, not a tagging one. Some of those do have a
per-URL early return for something else — bingx's `LISTEN_KEY` — which is not the same thing and does not cover their market data.

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

### Sequenced delta books — verify the venue contract before adoption

`packages/safe-cex/src/utils/sequencedDeltaOrderBook.ts` centralizes the safe
state machine for snapshot-initialized feeds with explicit
sequence/predecessor fields. It returns a transport-agnostic `recover` result
on a gap; the WebSocket adapter must translate that into the venue's
refcount-safe unsubscribe/resubscribe flow and wait for a fresh snapshot.

Adoption is intentionally gated. Verify current official documentation for
snapshot vs delta actions, exact predecessor semantics, duplicate/replay and
sequence-reset behavior, and sequence numeric width. Confirm resubscription
actually yields a snapshot, reset state on close and final unsubscribe, parse
all levels into finite positive prices/nonnegative amounts, and add a fresh
venue regression before replacing working code. Never infer compatibility
from `seqId`/`prevSeqId` field names alone. OKX and BloFin keep their audited
venue-specific implementations until this review is done.

### `messageHandlerStats` checks SUBSCRIPTIONS, not the connection

**Read this before removing anything from the topic watchdog.**

Connection liveness is already owned by safe-cex — ping/pong, `checkLastReceived`,
the reconnect backoff in `base.ws.ts`. Per-topic stats are not a second copy of
that and must never be reasoned about as one.

They exist to catch **a subscription that has silently stopped delivering while
the socket is perfectly healthy**: a subscribe dropped on reconnect, a venue that
ignores subscribes sent too fast, a frame lost in flight, an ack that never came.
The socket is up, the pong is fresh, every other feed is flowing — and one topic
is dead. That is the exact scenario this exists for, and it is invisible to every
connection-level check.

So these arguments are always wrong:

- "the pong is fresh, so the socket is fine, so dropping this topic costs
  nothing" — a fresh pong is the _precondition_ for the failure, not a refutation
  of it
- "other topics on the same socket are updating, so this one doesn't matter" —
  same error, per-socket instead of per-connection
- "this topic is event-driven so silence is normal" — true for a small number of
  topics, and **nearly all topics poll**. Establish it per topic, from observed
  behaviour or venue docs, for that exact topic. Do not infer it from the
  topic's name, from a neighbouring constant that groups it with others, or from
  a category you assembled yourself. A topic that has genuinely never sent
  anything has no stats entry at all and is already harmless; the failure mode is
  a topic that delivered and then stopped.

If a topic legitimately goes quiet, the fix is to make the _expectation_
conditional on whatever state governs its traffic — or to clear its entry via
`cleanupMessageHandlerStats` when that state goes away, which returns it to the
harmless never-recorded case. The fix is not to stop watching it, because then a
failed subscribe for that topic can never be detected again.

**The stats key MUST be the `messageHandlers` key.** This is the invariant the
whole watchdog rests on. `reconcileMessageHandlerStats` can only mark a topic
idle by looking its key up in `messageHandlers`; a key that lives nowhere in
that map gets `hadHandlers: false`, is never reconciled, and reports a dead
subscription as a live feed forever. Bybit states the positive form of this rule
in `bybit/CLAUDE.md` — record the handler key that matched.

Two shapes break it, and both shipped:

- **A bare shared channel name.** Hyperliquid recorded `json.channel`, so every
  symbol shared one `bbo` entry while handlers lived at `bbo.{coin}`. Record at
  the dispatch site, under the key you just looked up.
- **Recording before the no-listeners guard.** Kraken recorded in `onMessage`,
  ahead of `handleBookDelta`'s `handlers.length === 0` return — so the frame
  right after a teardown put the entry straight back, and with `hadHandlers`
  unset it was then permanently beyond reconcile. Kraken Futures is never sent
  an unsubscribe, so those frames never stop. Record inside the handler, after
  the guard.

Where a venue genuinely cannot use a `messageHandlers` key — kraken keeps
subscriptions in private `orderBookHandlers` / `ohlcvHandlers` maps — clear the
entry explicitly when the last symbol stops listening.

**Knowingly non-compliant, deliberately:** cryptocom (bare `result.channel`),
binance (bare `eventType` in `dispatchBinanceMessage`), paradex
(`order_book.{symbol}` against a bare `order_book` handler key), and kraken's
`ticker`. The first two are blocked on a threshold bug, not on effort — see
`apps/web/CLAUDE.md` → "Per-symbol stats keys can produce an INFINITE staleness
threshold" before "finishing" the migration. `asterdex`'s
`cleanupMessageHandlerStats(stream)` calls target keys that are never recorded;
dead, but harmless.

**`blofin`, `breakoutprop` and `orderly` never call `recordMessageHandlerStats`
at all** — only the cleanup side. Their datafeed list in the connection popup is
therefore empty and this watchdog is blind on them: a subscription that silently
stops delivering there produces no signal of any kind. That is a bigger gap than
a mis-keyed entry, and it is invisible precisely because nothing ever warns.

### Reconnect-banner stats: one topic on the frame path, map walk on a throttle

`base.ws.ts → recordMessageHandlerStats(topic)` is called on every
WS frame, so it refreshes only the topic that just arrived. The
map-wide refresh of `lastCallbacksCount` lives in
`reconcileMessageHandlerStats`, throttled to 5 s (250 ms floor on
the forced path that `cleanupMessageHandlerStats` triggers).

**Rule:** don't iterate the stats map on the frame path. Iterating
it on the throttle is the design, not a regression.

That distinction is the whole entry. The walk was once deleted
outright as redundant, on the reasoning that
`cleanupMessageHandlerStats(topic)` already removes the entry when
handlers go to zero and that the only consumer treats
`lastCallbacksCount` as truthy/falsy. Both premises were wrong:
those cleanup calls are hand-written per venue and were missing,
mis-keyed or unreachable on close to a dozen of them — see the
non-compliant list below — and the truthiness IS
the mechanism — a stuck non-zero count is what kept a torn-down
subscription showing as a live feed in the connection-status popup
until it aged into a warning nobody could act on. Optimise the
frame path; leave the reconcile alone.

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
  divides by 1000. Bybit now builds `createLiveCandleStart(opts.interval)`
  (`utils/calendarCandles.ts`) once at subscription time and calls it per
  trade: a bare epoch floor for intraday intervals, and for `1w` / `1M` the
  Monday-week / calendar-month start with the current bucket cached, so the
  hot path does no `Date` work until the bar rolls. **Do not inline an epoch
  floor for those two** — it starts a Thursday week and a 30-day "month" that
  match no venue kline. Other venues' trade-built bars (apexomni,
  blofin, lighter, woo) still epoch-floor `1w`.
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
   one with comparable architecture: Binance for HMAC + listen-key,
   OKX for OAuth-style passphrase, Hyperliquid for EIP-712, etc.).
3. Add metadata to `exchangeMetadata.ts` — see `packages/safe-cex/CLAUDE.md`
   for the proxy/tunnel flag matrix.
4. Register in both `registry.ts` and `registry.native.ts`. Their
   `Record<ExchangeName, ExchangeLoader>` types enforce name parity, while
   `exchange-registry-native-parity.audit-regression.test.ts` parses both
   registries and every target module to enforce native module/export parity.
5. Write the per-exchange `CLAUDE.md` against the template above. Link back
   to this parent doc instead of restating shared mechanics.
6. Freshly author deterministic regressions in
   `src/__tests__/audit-regressions/` from verified upstream behavior and the
   runtime contract. Never copy deleted exchange-tree test fixtures.
7. Run `yarn test-ci` from `packages/safe-cex` — `exchangeMetadata.test.ts`
   has invariant checks that catch impossible flag combinations.

## Reference

- Upstream CCXT: https://github.com/ccxt/ccxt — read the relevant exchange
  file when implementing or debugging signing/endpoints.
- Proxy/tunnel architecture: `packages/safe-cex/CLAUDE.md`.
- Tunnel infrastructure: `packages/safe-cex/src/httpOverWebsocket/`.


## Fill events report ONE execution

`OrderFillEvent.amount` is what that execution filled — never the order's
running total. Consumers add fills up (the chaser's `addFillAmount`, the web's
provisional positions), so a running total counts every partial again. Bybit
(linear, inverse, spot), Bitrue and BloFin report running totals and convert
through `utils/incrementalFills`. Bybit prices each execution from its traded
value (`qty / value` on inverse, where value is coin); BloFin derives cumulative
value from `filledSize × averagePrice`. A replayed frame emits nothing, and a
worker restart reports an already-part-filled order's total once.

**`fillEventUnits.ts` lists the venues whose event is VERIFIED** — one
execution, in `native` (placement) or `base` units. A venue absent there emits a
terminal total (BingX, ApeX, Coinbase), two events per execution (Lighter),
string sizes or algo totals (WOO, Orderly), a cumulative fallback (Bitget,
KuCoin Spot) or an unchecked unit, and the web builds no provisional position
from it. Verify a venue before listing it; do not list one to turn the feature on.

## Rust leverage and position-mode commands

The shared Rust command boundary preserves optional `setLeverage` position
`side` and optional `changePositionMode` market `symbol`. Only side-aware
venues interpret the side; coupled venues retain their existing behavior.
Phemex requires a symbol for its market-scoped mode setter; WEEX accepts an
optional symbol. Native JSON uses `setPositionMode` with that same scope.
Successful side writes update only the selected rail and matching positions;
failed writes preserve both. Venue metadata cannot make leverage clamping panic.
For the five side-aware families, reading already known leverage flushes the
engine and returns its confirmed leg settings instead of replacing them with
the market maximum. Fresh uncached leverage discovery is a separate contract.

## Rust cancellation requires evidence

For every registered Rust venue, canceling a nonempty ID absent from the local
store must send a native cancellation request or reject the command explicitly.
Store absence cannot prove an exchange order was canceled. Venues whose native
cancel needs a symbol, asset index, or ordinary/conditional provenance reject the
whole mixed known/unknown batch before writes. Native ID-only cancel APIs may
send the supplied identity directly. Empty requested lists remain valid no-ops.
The family inventory and regression evidence live in
`docs/plans/RUST_UNKNOWN_CANCEL.md`.
