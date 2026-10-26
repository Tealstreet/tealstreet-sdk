# Client order IDs by venue

What each venue does with a client order ID, for anything that needs to find an
order whose placement response was lost. Surveyed 2026-10-11 from the adapters;
field names marked † come from payload types, not a live check. The contract
itself is in `CLAUDE.md` → "Client order ID contract".

**Minted** is what `exchange.mintClientOrderId(opts)` returns: the venue's own
generated format, so broker attribution carried by the ID survives. It returns
`undefined` for a venue that refuses caller IDs, a shape it refuses (anything
but plain Limit/Market; Paradex also refuses `close`), an order above the
market's max size, or an unknown market.

The Rust wrapper applies the same caller-ID preflight as each TS adapter.
Binance, Binance PM and Phemex preserve the rules and broker-attributed mint
formats below; caller IDs reach the engine unchanged. An invalid order anywhere
in a batch prevents the entire batch from reaching the engine. Lighter rejects
caller IDs before any engine command.

| Venue                        | Caller rule              | Minted              | Attribution          | Where the ID lands on a mapped `Order`                    |
| ---------------------------- | ------------------------ | ------------------- | -------------------- | --------------------------------------------------------- |
| Binance                      | `[.A-Z:/a-z0-9_-]{1,36}` | `x-s3vFFRQp…`       | in the ID            | `info.clientOrderId` (REST), `info.c` (WS)                |
| Binance Spot                 | `[A-Za-z0-9_-]{1,36}`    | `x-s3vFFRQp…`       | in the ID            | **`Order.id`**                                            |
| Binance Options              | `[A-Za-z0-9_-]{1,36}`    | generic, no prefix  | none                 | `Order.clientOrderId`                                     |
| Binance PM                   | `[.A-Z:/a-z0-9_-]{1,32}` | `x-s3vFFRQp…`       | in the ID            | **`Order.id`**                                            |
| OKX (+EU, demo)              | `[A-Za-z0-9]{1,32}`      | `e847386590ce4dBC…` | ID + `tag`           | `info.clOrdId`                                            |
| BloFin                       | `[A-Za-z0-9]{1,32}`      | `b3cdedc0a20880ba…` | ID + body `brokerId` | `info.clientOrderId`                                      |
| Phemex                       | `.{1,40}`                | `Tealstreet_…`      | in the ID            | `info.clOrdID`                                            |
| Bybit (+demo, inverse, spot) | `[A-Za-z0-9_-]{1,36}`    | `ts…`               | header               | `info.orderLinkId`                                        |
| Bitget (+inverse, USDC)      | `[.A-Z:/a-z0-9_-]{1,32}` | `ts…`               | header               | `info.clientOid`                                          |
| KuCoin, KuCoin Spot          | `[A-Za-z0-9_-]{1,40}`    | `ts…`               | header               | `info.clientOid` † (WS)                                   |
| BingX                        | 1–40, no uppercase       | `ts…` (lowercase)   | header               | `info.clientOrderId` (REST), `info.c` (WS)                |
| Bitrue                       | `.{1,31}`                | `ts…`               | header               | `info.clientOrderId` (REST), `info.c` (WS)                |
| WEEX                         | `[.A-Z:/a-z0-9_-]{1,36}` | `ts…`               | none                 | `info.clientOrderId` †                                    |
| Coinbase (+spot)             | `\S+`                    | `tealstreet-…`      | none                 | `parentId`, `info.client_order_id`                        |
| ApeX Omni                    | `\S+`                    | `ts…`               | body `brokerId`      | `info.clientOrderId` / `info.clientId`                    |
| Bitunix                      | `\S+`                    | `ts…`               | none                 | **`Order.clientOrderId`**, `info.clientId`                |
| Paradex                      | `.{1,64}`, not `close`   | `ts_…`              | header               | `info.client_id`                                          |
| Orderly family               | `(?!-).{1,36}`           | `ts…`               | body `broker_id`     | `info.client_order_id` / `info.clientOrderId` †           |
| WOO                          | positive safe integer    | 15-digit number     | body `broker_id`     | `info.client_order_id` (v1) / `info.clientOrderId` (v3) † |
| Crypto.com                   | `.{1,36}`                | `ts…`               | none                 | **dropped** — mapper keeps no `info`                      |
| Backpack                     | uint32                   | 9-digit number      | header               | **dropped** — mapper keeps no `info`                      |
| GRVT                         | decimal, < 2^63          | 9-digit number      | builder fee          | **`Order.id`**                                            |
| Hyperliquid (+spot)          | `0x` + 32 lowercase hex  | random `0x…`        | builder fee          | **`Order.clientOrderId`** (WS and REST)                   |
| TxFlow (testnet)             | 1–64 UTF-8 bytes, no `0x` prefix | `ts_…` | none configured | `Order.clientOrderId` (WS and REST) |

Hyperliquid is the one venue that honors (and mints for) every shape it places,
stops and brackets included; only a trailing stop, which it does not support,
mints nothing. It also puts a cloid on every order it sends, so the WS handler
can identify its own orders — see `src/exchanges/hyperliquid/CLAUDE.md`.

**Refuse caller IDs** (`mintClientOrderId` → `undefined`): Lighter, Kraken
(+spot), MEXC, AsterDEX, BitMEX, BreakoutProp, HyroTrader, Polymarket.

## Lookup

`exchange.findOrderByClientOrderId(symbol, id)` checks the store, then the
venue, and matches with `orderCarriesClientOrderId` — the ID's location on the
mapped `Order` in the table above. It never writes the store. The base version
scans `fetchOrders([symbol])`, which lists OPEN orders only and on several
venues returns the store on failure, so its miss is `definitive: false`: it
cannot tell an order never placed from one already filled or cancelled. Only a
venue override that asks about closed orders as well may return
`definitive: true`. A venue that refuses caller IDs always misses without a
request. A venue fetch that fails can throw (Backpack) — treat that as "could
not check". A match is returned with its status, which may already be filled
or cancelled, so read `order.status` rather than assuming it is open. Crypto.com and Backpack drop the ID from their mapped orders, so their
lookup can never find one.

**Definitive lookups** ask a single-order endpoint that also sees filled and
cancelled orders: **OKX** (+EU, demo; `GET /api/v5/trade/order` by `clOrdId`; `51603` is a
definitive miss) and **Binance USD-M** (`GET /fapi/v1/order` by
`origClientOrderId`, on the PRIMARY key; `-2013` is a definitive miss and is
silent, so a lookup never toasts "Order does not exist"). Every other venue
uses the open-orders scan. Bybit is deliberately not definitive: its order
history lags, so a just-filled order can be missing from both endpoints.
**Hyperliquid** asks `orderStatus` by cloid, which sees filled and cancelled
orders and never writes the store (its `fetchOrders` would), but a miss is
still not definitive: the venue accepts a nonce up to two days old, so nothing
bounds when a send still in flight can land.

**A definitive miss right after a lost send is still not proof.** A placement
the venue is still processing (Binance documents 503 "execution status
unknown") can answer "does not exist" for a few seconds. Callers decide "never
placed" only after misses some seconds apart, well after the send — the web
resolver's rule is in `apps/web/src/order/CLAUDE.md`.

## Gaps this leaves

- **Only OKX (+EU, demo) and Binance USD-M look up closed orders.** Every other
  venue scans open orders, so a filled or cancelled order is invisible to it.
  Several expose a single-order endpoint that could be wired the same way
  (KuCoin `byClientOid`, Crypto.com `ORDER_DETAIL`, Backpack `ORDER_QUERY`,
  Binance Spot/PM `ORDER` with `origClientOrderId`).
- **The ID's location differs by venue** (`Order.id`, `parentId`, several
  `info.*` names, sometimes different between REST and WS on one venue), so a
  matcher needs a per-venue accessor rather than one field.
- **Crypto.com and Backpack drop it entirely**, so their orders can only be
  matched by shape (symbol, side, size, price, time).
