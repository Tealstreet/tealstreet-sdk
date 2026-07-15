# Arkham Exchange Integration

## Availability and safety boundary

Arkham is disabled in every environment. `ArkhamMetadata` must keep
`enabled`, `enabledInDev`, and `enableInPreview` false.
The frozen adapter also keeps `supportsTradesHistory`, `supportsPartialStops`,
and all TP/SL trigger/default advertising disabled. Its legacy method bodies
are not evidence of a usable capability against a service that no longer
publishes or serves this CEX protocol.

This directory is a frozen legacy centralized-exchange adapter. Arkham's
official April 2026 announcement describes its current trading product as a
decentralized Solana platform:

- https://info.arkm.com/announcements/introducing-arkham-decentralized-trading

The old CEX REST and WebSocket routes used here are unavailable, and Arkham no
longer publishes primary documentation for this protocol. The current Arkham
Intel API at `api.arkm.com` is an analytics API, not a compatible trading API.

Do not:

- enable or deploy this integration;
- place, cancel, or modify orders while validating it;
- point it at the Intel API or the decentralized product without a new protocol
  review;
- infer legacy WebSocket fields from REST field names;
- recreate tests under `src/exchanges/**`.

Safe maintenance is limited to deterministic local correctness, lifecycle
cleanup, and trusted regressions in `src/__tests__/audit-regressions/`.

## Source layout

- `arkham.exchange.ts`: exchange lifecycle, REST mapping, and trading methods
- `arkham.api.ts`: legacy request signing and response handling
- `arkham.types.ts`: frozen legacy endpoints and response types
- `arkham.ws-public.ts`: proxy-authenticated public subscriptions
- `arkham.ws-private.ts`: private events plus throttled REST reconciliation
- `../../exchangeMetadata.ts`: feature flags and shared client capabilities
- `docs/safe_cex_audit_july/exchanges/arkham.md` (repository root): audit evidence and
  remediation checklist

Routing is owned by `routing/routingTable.ts` and resolved through
`resolveExchangeRouting`. Do not hardcode the current tunnel host in this
directory or treat a tunnel hostname documented elsewhere as source of truth.

## Frozen legacy protocol

The adapter currently retains the following historical protocol assumptions so
the disabled code remains internally coherent:

- REST base URL: `https://arkm.com/api`
- WebSocket upstream: `wss://arkm.com/ws`, reached through the configured route
- REST authentication: API key plus HMAC-SHA256 signature using the base64
  decoded secret
- WebSocket authentication: signed `GET /ws` handshake sent to the proxy before
  subscriptions
- timestamp unit at the wire boundary: microseconds
- position mode: one-way
- nominal margin mode: cross/dynamic

These assumptions are not evidence that the service still supports the
protocol. Revalidation requires current primary trading documentation and
captured responses from the replacement venue.

The endpoint constants are likewise historical. Public reads include pairs,
tickers, books, trades, and candles. Private reads include account, balances,
positions, leverage, active orders, trigger orders, and history. Writes include
normal/trigger order placement and cancellation plus leverage changes. Do not
exercise those writes during audit or reactivation discovery.

## Startup and disposal

`ArkhamExchange.start` performs these steps:

1. fetch markets and seed the store;
2. fetch tickers and seed stable ticker identities;
3. ask the public socket to connect;
4. for public-data-only instances, mark private stores loaded and stop;
5. otherwise connect the private socket, start the REST tick, and load orders.

The public proxy protocol implemented by this adapter requires a legacy API key
and secret even for public subscriptions. A keyless public-data-only instance
therefore fails closed before creating a socket. Do not send an invented
unauthenticated or "public mode" hello unless the proxy contract and upstream
protocol are both verified from primary sources.

`ArkhamExchange.dispose` delegates to `BaseExchange.dispose`. The base class
owns store unsubscription, watchdog cleanup, event cleanup, and disposal of all
registered sockets. Both Arkham sockets additionally clear their legacy
30-second ping timer on close and dispose. Starting a ping chain first clears
the previous timer, so reconnect/auth events cannot accumulate parallel chains.

## Tickers and public WebSocket state

REST ticker identity is stable: `Ticker.id` equals the Arkham symbol. A public
ticker event must resolve the existing object through
`store.tickersBySymbol[symbol]` and pass that object to
`store.updateTicker(ticker, changes)`. Never pass a raw symbol string and never
write a new id in a delta.

The current mapper retains the historical snake_case WebSocket fields such as
`bid_price`, `ask_price`, and `last_price`. No live legacy frame or current
primary schema was available in July 2026, so those field names remain
explicitly unverified. This safe object-identity fix must not be mistaken for
wire-schema validation.

Per-ticker updates are cheap store mutations and are applied as received. A
single global throttle would drop events for different symbols in the same
window.

## Private events and authoritative reconciliation

Order and position WebSocket events have two separate responsibilities:

1. apply the individual event to the store immediately;
2. schedule a throttled REST snapshot to repair missed or reordered events.

Do not wrap the per-event upsert in a global Lodash throttle. A global throttle
keeps only the first/last arguments and loses intermediate order ids or symbols.
Only the comparatively expensive REST reconciliation is throttled.

The order reconciliation writes the fetched array through
`addOrUpdateOrdersFull`. The position reconciliation writes through
`addOrUpdatePositionsFull`, which preserves known position records while
zeroing positions omitted by the authoritative snapshot. Both paths set their
store `loaded` flag after the snapshot is committed.

A position event with `base === 0` is not a no-op. It upserts a zero-contract
position immediately. When an existing position is present, its side and id are
preserved because a one-way zero-size frame cannot encode the prior sign. The
balance's used/free values are recomputed even when used margin falls to zero.

The periodic REST tick also uses `addOrUpdatePositionsFull`; it must not bypass
the full-reconciliation semantics with a wholesale raw state replacement.

## Orders and reduce-only behavior

Legacy order mappings retain market, GTC, IOC, and FOK limit types plus separate
trigger orders. Trigger order ids and regular order ids come from different
wire fields. Batch writes are sequential to preserve order and rate-limit
behavior.

The historical adapter omits `reduceOnly` from limit-order payloads because the
old service reportedly rejected it. That behavior cannot be retested while the
service is unavailable. `ArkhamMetadata.autoDownSizeReduceOrders` must therefore
remain true so the shared order builder caps reduce orders client-side instead
of assuming server enforcement. This is a client safety net, not proof that the
legacy API accepts or enforces the flag.

Do not change trigger semantics, reduce-only payload behavior, or write
endpoints without a new primary protocol and non-production validation plan.

## Mapping notes

- Symbols use the legacy underscore form, for example `BTC_USDT_PERP`.
- Market precision values are increments, not decimal-place counts.
- REST/WebSocket timestamps historically use microseconds and are converted to
  milliseconds at store boundaries.
- The balance mapper includes USD, USDT, and USDC only.
- Position size is the absolute value of signed base amount; the sign chooses
  long or short.
- Missing leverage falls back to fetched account leverage and then the exchange
  default.
- Order books maintain incremental price-level maps. The last-price lookup must
  use `tickersBySymbol`, not scan all tickers on each hot-path update.
- HTTP clients must continue using `createXhrWithSecondaryKey`; do not replace it
  with a directly constructed Axios client and lose secondary-key routing.

## Validation

Fresh trusted coverage is under:

- `src/__tests__/audit-regressions/arkham.venue-disabled.test.ts`
- `src/__tests__/audit-regressions/arkham.remaining-audit.test.ts`

The regression suite covers disabled metadata, stable ticker identity, stored
ticker updates, burst-safe private event upserts, zero-contract position
closure, authoritative full snapshots, keyless public-socket fail-closed
behavior, keepalive cleanup, and shared reduce-order downsizing metadata.

Use focused validation first, then package-wide checks:

```bash
yarn workspace @tealstreet/safe-cex vitest run \
  src/__tests__/audit-regressions/arkham.venue-disabled.test.ts \
  src/__tests__/audit-regressions/arkham.remaining-audit.test.ts
yarn workspace @tealstreet/safe-cex lint
yarn workspace @tealstreet/safe-cex typecheck
yarn workspace @tealstreet/safe-cex test-ci
```

## Reactivation gate

Reactivation is a new integration project, not a metadata flip. Before changing
any enablement flag, require all of the following:

- official trading REST and WebSocket documentation for the current product;
- an explicit custody, account, signing, and order model review;
- verified market, ticker, book, candle, balance, position, order, and private
  event schemas;
- captured public and private frames with credentials designed for testing;
- deterministic regressions for each captured schema outside the exchange tree;
- read-only validation through the real configured routing path;
- separate authorization before any test order, deployment, or production
  enablement.

Until then, live ticker snake_case fields, public proxy auth alternatives,
reduce-only limit behavior, and every legacy write endpoint remain unverified.
