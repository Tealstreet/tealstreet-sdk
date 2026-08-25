# MEXC Exchange Integration

## Exchange Information

- Exchange name: `mexc`
- Product scope: USDT- and USDC-margined perpetual futures.
- Venue symbols use underscores (`BTC_USDT`); Tealstreet symbols remain compact (`BTCUSDT`).
- MEXC reports order and position sizes in native contract volume. Convert to base-asset size with `market.contractSize` before publishing to the store.
- MEXC ticker `holdVol` is open interest in native contract volume. Convert it to base with `market.contractSize`, then publish `Ticker.openInterest` as quote notional using fair/mark price so ticker tables align with the venue's `Open Interest (USDT/USDC)` display.
- `PlaceOrderOpts.amount` arrives from the app order flow as native contract volume. Send it to MEXC as `vol` after snapping to `volUnit`; do not convert it with `contractSize` again.
- Web close-size helpers must use native volume for MEXC reduce-only closes. A displayed `0.006 BTC` BTC_USDT position is `60` native contracts when `contractSize` is `0.0001`; sending `amount: 0.006` rounds to `vol: 1` and closes only `0.0001 BTC`.
- The shared reduce-only clamp (`apps/web/src/utils/order/downSizeReduceOrder.ts`) runs before `placeOrder` and historically compared the incoming native amount with `position.contracts` (base) and stored `order.amount` (base) directly. For MEXC-style fractional-lot linear futures (`contractSize < 1`) it now converts the incoming native amount to base for the clamp and converts the result back to native before returning, so full and partial closes preserve the intended `vol` instead of collapsing to `vol: 1`.

## Domains

- REST: `https://api.mexc.com`
- Public/private futures WS: `wss://contract.mexc.com/edge`

`https://contract.mexc.com` currently mirrors the public futures REST paths, but the adapter uses `api.mexc.com` because the current official docs route futures REST there.

## REST Authentication

Private REST requests use headers:

- `ApiKey`
- `Request-Time`
- `Signature`
- `Recv-Window`
- `Content-Type: application/json`

The signature is:

```text
hmac_sha256(secret, apiKey + requestTime + requestParam)
```

For GET requests, `requestParam` is the sorted query string. For POST requests, it is the exact raw JSON body string sent on the wire. Do not reserialize a POST payload after signing.

Public endpoints must call `markSkipRemoteSigning` so hosted public reads do not round-trip to the client signer.

## WebSocket Authentication

Private WebSocket login sends:

```json
{
  "method": "login",
  "param": {
    "apiKey": "...",
    "reqTime": "...",
    "signature": "hmac_sha256(secret, apiKey + reqTime)"
  }
}
```

Hosted mode must use the remote signer for this login signature. The secret is intentionally empty on hosted exchange-server instances.

## WebSocket Diagnostics

MEXC public/private WebSockets emit `grafana-pws-sub` for outbound frames and `grafana-pws` for inbound frames so the web Requests drawer and Grafana diagnostics can show WS traffic even while the global `ENABLE_PWS_LOG` flag remains off. Keep private login frames redacted before emission. Public `push.ticker`, `push.depth`, and `push.depth.full` frames are intentionally skipped because ticker/depth fanout is high-volume; subscriptions, pongs, klines, trades, and private account frames should still be visible.

MEXC `rs.*` channels are one-shot control responses (for example `rs.sub.ticker` / `rs.sub.kline`), not liveness feeds. They may be logged for diagnostics, but do not record them in `messageHandlersStats`; otherwise the generic stale-topic health check treats the naturally quiet ack row as a dead feed and reloads the account around the timeout window.

## Order Semantics

- Side enum: `1` open long, `2` close short, `3` open short, `4` close long.
- Order type enum: `1` limit, `2` post-only, `3` IOC, `4` FOK, `5` market.
- Position mode enum: `1` hedge, `2` one-way.
- Margin/open type enum: `1` isolated, `2` cross.
- Regular order updates use cancel/replace. Native amend is not wired until verified.
- Regular order placement is FIFO-paced with a short gap between submits and an added cooldown after venue `code:510` / `Requests are too frequent` responses. Do not bypass this queue for UI-driven single orders or scaled order loops unless MEXC batch placement has been verified.
- Regular scaled `placeOrders` first tries `POST /api/v1/private/order/submit_batch` with a raw array body and `stpMode: 0` on each leg. Official docs say this endpoint is only supported for market maker accounts; live non-market-maker accounts return `code:511` / `You do not have access to the interface`. Treat this as an API key / account-UID entitlement issue, not evidence that the REST source IP is not market-maker allowlisted. Cache that denial for the session and fall back to paced sequential placement instead of retrying native batch on every scaled order.
- Regular order cancellation uses `POST /api/v1/private/order/cancel` with the raw JSON body as an order-id array (for example `["739113577038255616"]`), not an `{ orderIds: [...] }` object. The docs list `orderIds`, but live MEXC rejects the object body with `code:600 Parameter error`; CCXT carries the same raw-array workaround. Inspect returned per-order `errorCode` rows before marking local orders canceled.
- Private WS order/fill reconciliation and the periodic `tick` use a regular-open-orders-only refresh and preserve existing plan/position-stop rows in store. Use full `fetchOrders()` only when TP/SL/stop visibility is needed, because it fans out to regular, plan, and stop-order list endpoints.
- The paced sequential fallback preserves partial-placement metadata. If one leg fails after earlier legs returned order ids, `placeOrders` annotates the thrown error with `partialOrderIds`, `partialOrderCount`, `totalOrderCount`, and `failedOrderIndex` so the web order layer can tell the user that the batch halted partially.
- Standalone sized TP/SL uses `POST /api/v1/private/planorder/place/v2` as a reduce-only market plan order, seven-day execution cycle (`executeCycle: 2`), and explicit quantity. Trigger direction is inferred from order type plus closing side.
- After a successful standalone TP/SL or position TP/SL placement, add a local open stop row immediately and schedule a delayed full order refresh. MEXC private WS reports regular orders/fills but does not reliably emit the plan-order row, so relying only on WS makes accepted chart-drag TP/SL lines vanish when the optimistic row expires.
- Full-position / infinity TP/SL uses the separate position TP/SL API: `POST /api/v1/private/stoporder/place` with the native `positionId`, current native position `vol`, and `volType: 2`. These rows are listed with `GET /api/v1/private/stoporder/list/orders?is_finished=0` and canceled through `POST /api/v1/private/stoporder/cancel`; do not send them to the standalone `planorder` cancel route.
- Open TP/SL orders do not appear in `/api/v1/private/order/list/open_orders`. Fetch untriggered plan orders with `GET /api/v1/private/planorder/list/orders?states=1` and normalize them into regular Tealstreet order state so REST reconciliation does not erase optimistic chart lines after placement.
- Cancel standalone TP/SL through `POST /api/v1/private/planorder/cancel`; regular order cancellation does not apply to these plan-order ids.
- Margin mode changes use `POST /api/v1/private/position/change_leverage` with the current leverage plus `openType`; MEXC has no `change_margin_type` endpoint.
- Position mode is account-level. After a successful mode change, update cached per-symbol metadata so flat fake positions do not revert the UI to the previous mode.
- Active-symbol metadata refresh (`updateSymbolMetadata`) must call `GET /api/v1/private/position/leverage` even when the symbol is flat. Open-position rows are absent while flat, so deriving from positions alone falls back to 20x and makes first-load order validation use the wrong leverage.
- Contract detail can advertise a high top-level `maxLeverage` (for example BTC_USDT 500x) while the authenticated account/symbol cap is lower. Prefer `GET /api/v1/private/position/leverage` `maxLeverageView` for slider/display caps when credentials are available.
- In hedge mode, long and short leverage can differ on the same ticker. Preserve separate `buyLeverage` and `sellLeverage` rails when fetching or setting leverage.
- Trailing stops and opening trigger orders are intentionally not advertised or implemented in the MVP.

## Current Validation Status

Public REST contract detail, ticker, depth, and kline endpoints were probed successfully without credentials. Authenticated REST write and private WebSocket probes still require `MEXC_API_KEY` and `MEXC_API_SECRET`.
