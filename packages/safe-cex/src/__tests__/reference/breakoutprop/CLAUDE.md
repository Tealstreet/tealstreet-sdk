# Breakout Prop legacy DXtrade adapter

This directory contains a disabled, polling-only adapter for Breakout Prop's
legacy DXtrade accounts.

## Product status

Breakout's proprietary terminal is the platform for all accounts purchased
after July 9, 2026. Existing DXtrade evaluation and funded accounts remain
usable, but that legacy pool is frozen. This adapter only speaks DXtrade and is
disabled in production and preview (`enabled: false`, `enableInPreview: false`,
`enabledInDev: true`). Do not enable it as a path for new Breakout accounts.

First-party references:

- DX REST specification: `https://demo-xt.dx.trade/specs/DX_REST_API.html`
- Breakout DXtrade retirement: `https://www.breakoutprop.com/article/dxtrade-retirement/`
- Breakout terminal launch: `https://www.breakoutprop.com/article/breakout-terminal-launch/`

The configured legacy API origin is
`https://breakout.prosp.devexperts.com/dxsca-web`. There is no testnet URL.
The old `demo.dx.trade/developers` link is unavailable and must not be used as
an implementation oracle.

## Files and architecture

| File | Responsibility |
| --- | --- |
| `breakoutprop.api.ts` | Axios client, DXAPI session header, bounded auth recovery |
| `breakoutprop.exchange.ts` | Markets, portfolio, orders, candles, and polling lifecycle |
| `breakoutprop.types.ts` | Active endpoints and wire response contracts |
| `breakoutprop.ws-public.ts` | In-process polling listeners for bars and order books |

There is no exchange WebSocket. `BreakoutpropPublicWebsocket` is retained only
as a subscription/lifecycle container for the common safe-cex interface. The
polling loop fetches REST tickers and dispatches them to `poll_book.<symbol>`
and `poll_kline.<symbol>` listeners. Do not add HMAC WebSocket authentication:
DXtrade REST uses a login session, and the removed localhost/Bitrue socket was
dead copy-paste code.

## Authentication

`BreakoutpropExchange.getSessionToken` POSTs `/login` with the DXtrade basic
authentication body:

```json
{ "domain": "default", "username": "<key>", "password": "<secret>" }
```

The response supplies `sessionToken` and an `HH:MM:SS` inactivity timeout.
The adapter parses that timeout, caches the deadline, and deduplicates
concurrent login calls. Private requests attach `DXAPI <sessionToken>` using
the existing tunnel/direct header convention.

The first 401 on a non-login request invalidates the local session, logs in,
and retries that request once. Login failures and a second 401 propagate. This
matches the first-party DX REST expired-token procedure and prevents an
unbounded auth loop. Generic retry is limited to network errors; DXtrade has
no Bitrue-style `error sign!` response contract.

`getAccountCode` fetches `/users/{username}`, retains `FULL_TRADING` accounts,
and requires `options.subaccount` when more than one is available. Account
selection matches the exact account code or suffix, preventing a request for
`2` from selecting `12`. Account codes use the `default:<account>` form and are inserted through
`replaceUrlParams`.

## Active REST surface

| Constant | Path | Method | Purpose |
| --- | --- | --- | --- |
| `LOGIN` | `/login` | POST | Session login |
| `USER` | `/users/{username}` | GET | Account discovery |
| `INSTRUMENTS` | `instruments/query` | GET | Base instrument data |
| `INSTRUMENT_DETAILS` | `accounts/{accountCode}/instruments/query` | GET | Account sizing/margin data |
| `MARKET_DATA` | `marketdata` | POST | Quotes and historical candles |
| `PORTFOLIO` | `accounts/{accountCode}/portfolio` | GET | Balance, positions, and orders |
| `PLACE_ORDER` | `accounts/{accountCode}/orders` | POST | Place order |
| `CANCEL_ORDER` | `accounts/{accountCode}/orders/{orderCode}` | DELETE | Cancel order |

Only `PORTFOLIO` is periodically monitored. `PLACE_ORDER` and `CANCEL_ORDER`
are monitored on every request. There are no separate active positions/orders
read endpoints in this adapter.

## Market and candle contracts

Safe-cex precision values are steps, not decimal counts:

- `precision.price = instrument.priceIncrement`
- `precision.amount = instrumentDetails.minOrderSizeIncrement`
- `limits.amount.min = instrumentDetails.minOrderSize`

The `XBTUSD` rows remain filtered because the legacy host returned HTTP 500
when they were included in ticker requests. Max leverage is the reciprocal of
the first margin-rate tier.

Historical candle timestamps are normalized from RFC 3339 milliseconds to
safe-cex seconds. The polling listener creates synthetic zero-volume candles
from ticker `last`, bucketed to the subscribed interval start. The order book
is likewise synthetic and contains the REST quote's bid/ask with placeholder
size, with one level on each side. Public-only sessions continue quote polling
for their listeners while skipping portfolio reads. These are legacy limitations, not real WebSocket market depth/bars.

## Portfolio reconciliation

Startup resolves the account, fetches markets and tickers, then loads the
portfolio before entering the polling loop. Balance, positions, and orders all
come from the same `PORTFOLIO` response. `fetchOrders` delegates to
`formatOrders(await fetchPortfolioRaw())`, so a failed portfolio read preserves
the current store instead of wiping orders.

Native LIMIT/STOP prices come from documented top-level `limitPrice`/`stopPrice`,
with legacy `legs[0].price` retained as a fallback. MARKET rows may omit price.
Order `lastUpdate` comes from RFC 3339 `transactionTime`, and a first-leg
`positionEffect: "CLOSE"` maps to `reduceOnly: true`.

The consumed portfolio does not include exchange-published account/position
metrics. The adapter derives:

- position `marginSize = abs(quantityNotional) * marginRate`
- position unrealized PnL from the latest stored mark/last price, falling back
  to the open price until a ticker exists
- balance `used` and `upnl` as position sums
- balance `total = cash balance + upnl`
- balance `free = total - used`

These are explicit client-side estimates for the disabled legacy adapter.
Native `positionCode` remains the position ID so distinct lots on one symbol
survive Store reconciliation. Position mode is always one-way and margin type is
always cross. Missing
symbols receive zero-sized placeholder positions.

## Order contract

The first-party DX REST single-order contract supports `LIMIT`, `MARKET`, and
`STOP`. The adapter maps:

| Internal type | DX type | Price field |
| --- | --- | --- |
| `OrderType.Limit` | `LIMIT` | `limitPrice` |
| `OrderType.Market` | `MARKET` | neither |
| `OrderType.StopLoss` / `TakeProfit` | `STOP` | `stopPrice` |

Only the documented last-price stop trigger is advertised in metadata.
Unsupported internal types, missing limit prices, and missing stop trigger
prices fail before any request is sent.

DX REST documents `positionEffect` values `OPEN` and `CLOSE`. Reduce-only
orders therefore send `CLOSE`; all others send `OPEN`. The first-party single
order schema does not require `positionCode`, so the adapter does not guess a
legacy-host extension.

Orders use GTC and a `tealstreet-<uuid>` client code. Placement returns the
SDK string-array order-ID shape, normalizing the native numeric ACK. Amount and price are
adjusted to market steps. Cancellation includes the standard `If-Match: *` precondition required by DX
REST; errors, including 412, propagate without a replacement. Bulk cancellation
runs batches of ten, awaits every
cancel in the current batch, and propagates rejection. `updateOrder` awaits the
cancel before placing its replacement, preventing double exposure.

## Lifecycle and validation

`dispose` stops the polling listener through the common base lifecycle. Do not
introduce a live socket unless a current first-party Breakout API is published
and the product decision changes.

Fresh deterministic regressions belong under
`src/__tests__/audit-regressions/`; never restore tests under
`src/exchanges/**`. The Breakout contract suite covers official order payloads,
session timeout/401 recovery, cancel-and-replace ordering, candle timestamps,
portfolio order mapping, and derived margin/PnL.

Validate from `packages/safe-cex` with `yarn lint`, `yarn typecheck`, the
targeted Breakout audit suite, and the complete deterministic test suite.

## Client order ID

This venue does not honor `PlaceOrderOpts.clientOrderId`: `placeOrder` / `placeOrders` reject any caller ID before a write (`packages/safe-cex/CLAUDE.md` → "Client order ID contract"). Keep that guard as the first statement of any new placement override.


## Rust engine

The `sdk-rs/venues/breakoutprop` port uses the same DXtrade API and account
model. Native session authentication stays in `Authorization: DXAPI …`;
Tealstreet proxy JWT authorization uses the separate `Tealstreet-Authorization`
policy, including the native serialized routing plan. There is no signing or
exchange socket. Initial session/market reads may use the five-request burst
budget; subsequent attempts stay within five per rolling second.

Native position codes distinguish multiple lots on one symbol. Account snapshots
validate nested identities and commit atomically. Confirmed cancellation protects
against stale snapshots for 60 seconds; reads launched before successful writes
are revision-fenced. An amendment verifies the canceled order's final, zero-fill
history before creating its replacement. Failed or ambiguous proof sends no
replacement. Unrepresentable attached protections, trigger sources, close flags
and slippage controls fail before any batch write.

Explicit zero-quantity CLOSE orders retain their native quantity through
reconciliation and price amendments; filled and remaining must also be exactly
zero. Empty optional client IDs are treated as absent so native order codes
remain distinct during cancellation, locking and history confirmation.

Polling intervals honor initial and live slowdown multipliers. An explicit
infinite interval pauses polling; native queries still work. Repeated unchanged
configuration updates preserve the existing poll deadline. History axes without
a TS implementation keep their branded, empty, no-I/O fallbacks.
