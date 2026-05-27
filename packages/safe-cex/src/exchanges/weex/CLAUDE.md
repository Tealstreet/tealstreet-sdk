# CLAUDE.md - WEEX Exchange Integration

WEEX uses Contract V3 for this adapter. The Contract V3 changelog states
that Futures Contract V3 launched on 2026-03-09 and Contract WebSocket V3
launched on 2026-03-18; V2 is no longer maintained.

## Local Smoke Test Keys

Use `.apikey.json` in this directory for local manual smoke tests. It is
gitignored and must not be committed.

Expected shape:

```json
{
  "key": "ACCESS-KEY",
  "secret": "secret",
  "passphrase": "passphrase"
}
```

## Domains

- REST: `https://api-contract.weex.com`
- Public WS: `wss://ws-contract.weex.com/v3/ws/public`
- Private WS: `wss://ws-contract.weex.com/v3/ws/private`

## Auth

REST private endpoints use headers:

- `ACCESS-KEY`
- `ACCESS-PASSPHRASE`
- `ACCESS-TIMESTAMP`
- `ACCESS-SIGN`
- `Content-Type: application/json`

The REST signature is:

```text
base64(hmac_sha256(secret, timestamp + METHOD + path + ("?" + queryString if present) + bodyString))
```

Use `virtualClock.getCurrentTimeMs()` for timestamps. Sign `DELETE` requests
as uppercase `DELETE`.

Private WebSocket auth is header-based, not a login message. The private WS
signature signs:

```text
timestamp + "/v3/ws/private"
```

## Implementation Notes

- `POST /capi/v3/batchOrders` is explicitly unavailable in Contract V3, so
  `placeOrders` should call `placeOrder` serially.
- Normal order amend is not documented in Contract V3. Use cancel-and-replace
  for `updateOrder`, preserving TP/SL fields when present.
- Reduce-only request parameters are not documented for normal order
  placement. Prefer `POST /capi/v3/closePositions` for full close flows and
  use `reduceOnly` only where WEEX documents or echoes it.
- Public symbols are already Tealstreet/Binance-style (`BTCUSDT`).
- WEEX sizes in V3 REST public examples are base-asset quantities; still keep
  `contractVal` from `exchangeInfo` in `market.info` for smoke-test auditing.

## Primary Endpoints

- Markets: `GET /capi/v3/market/exchangeInfo`
- Tickers: `GET /capi/v3/market/ticker/24hr`,
  `GET /capi/v3/market/ticker/bookTicker`
- Depth: `GET /capi/v3/market/depth`
- Klines: `GET /capi/v3/market/klines`,
  `GET /capi/v3/market/historyKlines`
- Public trades: `GET /capi/v3/market/trades`
- Balance: `GET /capi/v3/account/balance`
- Account config: `GET /capi/v3/account/accountConfig`
- Symbol config: `GET /capi/v3/account/symbolConfig`
- Positions: `GET /capi/v3/account/position/allPosition`
- Margin type: `POST /capi/v3/account/marginType`
- Leverage: `POST /capi/v3/account/leverage`
- Open orders: `GET /capi/v3/openOrders`
- Open algo orders: `GET /capi/v3/openAlgoOrders`
- Place order: `POST /capi/v3/order`
- Place conditional: `POST /capi/v3/algoOrder`
- Place TP/SL: `POST /capi/v3/placeTpSlOrder`
- Cancel orders: `DELETE /capi/v3/batchOrders`
- Cancel all normal: `DELETE /capi/v3/allOpenOrders`
- Cancel algo: `DELETE /capi/v3/algoOrder`
- Cancel all algo: `DELETE /capi/v3/algoOpenOrders`
- User trades: `GET /capi/v3/userTrades`
- Income: `POST /capi/v3/account/income`

## WebSocket Channels

Subscribe with:

```json
{ "method": "SUBSCRIBE", "params": ["BTCUSDT@ticker"], "id": 1 }
```

Public channels:

- `<symbol>@ticker`
- `<symbol>@depth15` or `<symbol>@depth200`
- `<symbol>@trade`
- `<symbol>@kline_<interval>_LAST_PRICE`

Private channels:

- `account`
- `positions`
- `orders`
- `fill`
