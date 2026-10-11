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

| Venue                        | Caller rule              | Minted              | Attribution          | Where the ID lands on a mapped `Order`                    |
| ---------------------------- | ------------------------ | ------------------- | -------------------- | --------------------------------------------------------- |
| Binance                      | `[.A-Z:/a-z0-9_-]{1,36}` | `x-s3vFFRQp…`       | in the ID            | `info.clientOrderId` (REST), `info.c` (WS)                |
| Binance Spot                 | `[A-Za-z0-9_-]{1,36}`    | `x-s3vFFRQp…`       | in the ID            | **`Order.id`**                                            |
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
| Bitunix                      | `\S+`                    | `ts…`               | none                 | `info.clientId`                                           |
| Paradex                      | `.{1,64}`, not `close`   | `ts_…`              | header               | `info.client_id`                                          |
| Orderly family               | `(?!-).{1,36}`           | `ts…`               | body `broker_id`     | `info.client_order_id` / `info.clientOrderId` †           |
| WOO                          | positive safe integer    | 9-digit number      | body `broker_id`     | `info.client_order_id` (v1) / `info.clientOrderId` (v3) † |
| Crypto.com                   | `.{1,36}`                | `ts…`               | none                 | **dropped** — mapper keeps no `info`                      |
| Backpack                     | uint32                   | 9-digit number      | header               | **dropped** — mapper keeps no `info`                      |
| GRVT                         | decimal, < 2^63          | 9-digit number      | builder fee          | **`Order.id`**                                            |

**Refuse caller IDs** (`mintClientOrderId` → `undefined`): Hyperliquid (+spot),
Lighter, Kraken (+spot), MEXC, AsterDEX, BitMEX, BreakoutProp, HyroTrader,
Polymarket.

## Gaps this leaves

- **No venue has a lookup by client ID in safe-cex.** The only order read is the
  `fetchOrders` list. Several venues expose one (OKX
  `GET /api/v5/trade/order?clOrdId`, KuCoin `byClientOid`, Binance `ORDER` with
  `origClientOrderId`, Crypto.com `ORDER_DETAIL`, Backpack `ORDER_QUERY`); none is
  called.
- **The ID's location differs by venue** (`Order.id`, `parentId`, several
  `info.*` names, sometimes different between REST and WS on one venue), so a
  matcher needs a per-venue accessor rather than one field.
- **Crypto.com and Backpack drop it entirely**, so their orders can only be
  matched by shape (symbol, side, size, price, time).
