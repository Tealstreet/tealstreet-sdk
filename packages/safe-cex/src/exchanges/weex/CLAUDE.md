# CLAUDE.md - WEEX Exchange Integration

WEEX uses Contract V3 for this adapter. The Contract V3 changelog states
that Futures Contract V3 launched on 2026-03-09 and Contract WebSocket V3
launched on 2026-03-18; V2 is no longer maintained.

## Exchange Information

- Exchange name: `weex` (Contract V3 perpetual futures).
- Availability: development-only (`enabled: false`, `enabledInDev: true`,
  `enableInPreview: false`). Preview and production are both production lanes;
  do not enable this adapter there without a fresh release review.
- Testnet/demo: the adapter does not expose testnet support. WEEX documents a
  limited demo API, but it is not wired into this integration.

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

Node/Bun and CLI paths use the header-capable reconnecting `ws` transport. It
evaluates the required private-auth header provider for every connection
attempt, so the timestamp and signature are regenerated before each reconnect.
A renderer that exposes a Node marker but cannot load `ws` is also unsupported:
`BaseWebSocket` fails closed for those required auth headers instead of silently
dropping them. Browser WebSocket APIs cannot set upgrade headers, and WEEX has
no private login-message alternative; the private socket is deliberately
disabled (no credentials are put in the URL) and the exchange continues its
2-second REST safety loop instead. Public `User-Agent` and Tealstreet routing
headers are static best-effort headers, not fail-closed authentication.

Private server keepalive frames are `{ "type": "ping", ... }`; reply
immediately with `{ "method": "PONG", "id": n }`. Public pings instead use
`event: "ping"`. WEEX closes a connection after more than 10 unanswered pings.

## API Documentation

- [Contract V3 WebSocket overview](https://www.weex.com/api-doc/contract/Websocket/websocket-intro)
- [Contract V3 introduction](https://www.weex.com/api-doc/contract/intro)
- [Contract V3 changelog](https://www.weex.com/api-doc/contract/changelog)
- [Contract V3 signature](https://www.weex.com/api-doc/contract/QuickStart/Signature)
- [CCXT WEEX reference](https://github.com/ccxt/ccxt/wiki/weex)

## File Structure

Follows the standard layout in `../CLAUDE.md`; `weex.utils.ts` contains the
Contract V3 signing helpers shared by the REST and private WebSocket paths.

## Implementation Notes

- WEEX launched `POST /capi/v3/batchOrders` on 2026-05-28. The adapter still
  calls `placeOrder` serially until batch response and partial-failure behavior
  are implemented and covered with fresh deterministic regressions.
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
