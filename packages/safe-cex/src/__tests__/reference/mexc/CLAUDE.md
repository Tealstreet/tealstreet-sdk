# MEXC Exchange Integration

## Exchange Information

- Exchange name: `mexc`
- Product scope: USDT- and USDC-margined perpetual futures.
- Venue symbols use underscores (`BTC_USDT`); Tealstreet symbols remain compact (`BTCUSDT`).
- MEXC reports order and position sizes in native contract volume. Convert to base-asset size with `market.contractSize` before publishing to the store.
- MEXC ticker `holdVol` is open interest in native contract volume. Convert it to base with `market.contractSize`, then publish `Ticker.openInterest` as quote notional using fair/mark price so ticker tables align with the venue's `Open Interest (USDT/USDC)` display.
- `PlaceOrderOpts.amount` arrives from the app order flow as native contract volume. Send it to MEXC as `vol` after snapping to `volUnit`; do not convert it with `contractSize` again.
- Public `Order.filled` is base-sized (`dealVol * contractSize`), but chaser replacement arithmetic tracks fills against native `PlaceOrderOpts.amount`. Keep cumulative native fill volume in `Order.extra.filledInPlacementUnits`.
- Web close-size helpers must use native volume for MEXC reduce-only closes. A displayed `0.006 BTC` BTC_USDT position is `60` native contracts when `contractSize` is `0.0001`; sending `amount: 0.006` rounds to `vol: 1` and closes only `0.0001 BTC`.
- The shared reduce-only clamp (`packages/script-api-core/src/order/downSizeReduceOrder.ts`) runs before `placeOrder` and historically compared the incoming native amount with `position.contracts` (base) and stored `order.amount` (base) directly. For MEXC-style fractional-lot linear futures (`contractSize < 1`) it now converts the incoming native amount to base for the clamp and converts the result back to native before returning, so full and partial closes preserve the intended `vol` instead of collapsing to `vol: 1`.

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

Broker attribution is still pending the post-freeze implementation. MEXC has
assigned Tealstreet broker code `TEAL`; attach it to MEXC REST request headers
before finalizing/releasing the integration. The support thread did not include
the exact header key in the chat handoff, so confirm the canonical MEXC broker
header name with their docs/support before wiring it, then add regression
coverage that private writes/authenticated REST calls carry the broker header.

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

**Signer-failure branch does NOT say "auth failed".** The `remoteSigner.sign` error branch in `mexc.ws-private.ts` emits `Remote WS signing unavailable: …`, not `Remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`.

## WebSocket Diagnostics

MEXC public/private WebSocket diagnostics go through `BaseWebSocket`'s runtime-gated `emitWsDiagnostic*` helpers. They only populate the web Requests drawer when the Requests Log toggle enables WS diagnostics, and Grafana upload remains behind the separate WS upload opt-in. Keep private login frames redacted before emission. Public `push.ticker`, `push.depth`, and `push.depth.full` frames are intentionally skipped because ticker/depth fanout is high-volume; subscriptions, pongs, klines, trades, and private account frames should still be visible.

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
- Private WS order/fill/position handlers must not fire REST refetches. `push.personal.order` and `push.personal.position` carry the full row (`vol` / `remainVol` / `state` / `price` / `holdVol` / `frozenVol` / `holdAvgPrice` / `leverage` / `liquidatePrice` / `marginRatio` / `pnl` / ...), so `handleOrder` / `handleFill` / `handlePosition` only need to normalize and write to the store. Reconciliation is covered by the periodic 2s `tick` (regular-open-orders + positions/balance) and the web-layer `scheduleUserActivityPositionRefetch` bursts after user actions (positions only — there is no user-activity-driven order refetch). The previous 1s WS-triggered `throttledFetchOrders` / `throttledFetchPositions` doubled every order/position update into a REST roundtrip and raced the user-activity refetch (two `GET /open_positions` firing ~6 ms apart on a single order edit); do not reintroduce them.
- MEXC futures private WS `personal.filter` is a _narrowing_ filter, not a subscription — logging in with `subscribe: true` pushes every private topic by default, and any filter list submitted afterward drops topics not in the list. Per the futures WS docs, the filter keys are `order`, `order.deal`, `position`, `asset`, `plan.order`, `stop.order`, `stop.planorder`, `risk.limit`, `adl.level`; if TP/SL frames appear missing, first confirm the filter includes `plan.order` / `stop.order` / `stop.planorder`, then confirm the push handler is dispatching `push.personal.plan.order` / `push.personal.stop.order` / `push.personal.stop.planorder`. CCXT also emits `push.personal.trigger.order` for the same plan-order surface; keep both channel names dispatched to `handlePlanOrder` to survive naming inconsistencies between MEXC's spot and contract WS servers.
- `handlePlanOrder` / `handleStopOrder` normalize with `normalizePlanOrder` / `normalizeStopOrder`, then call `MexcExchange.markStopLikeOrderSeen(id)` to release the placement shield entry for that id. Do not fan out a REST refresh from these handlers — the frame is a full snapshot of the row, and the trailing-debounced `scheduleStopOrderSnapshotRefresh` still runs after placement as a safety net.
- Regular contract orders and trigger orders (plan orders + position TP/SL) use **different** `state` enums, and mixing them was the root cause of a full-scale "canceled TP/SL all reappear on the chart" regression during rapid cancel bursts. Regular order states: `1` Uninformed, `2` **Uncompleted (Open)**, `3` Completed, `4` Cancelled, `5` Invalid. Trigger order states (both `push.personal.plan.order` and `push.personal.stop.order`, and the REST plan/stop list endpoints): `1` Untriggered (Open), `2` **Cancelled**, `3` Executed, `4` Invalid, `5` Failed. The regular `ORDER_STATUS` map (`mexc.types.ts`) is only correct for `normalizeMexcOrder`; `normalizePlanOrder` and `normalizeStopOrder` MUST route status through `normalizeTriggerOrderStatus` in `mexc.utils.ts`. Historically `normalizePlanOrder` reached for `ORDER_STATUS` directly and silently miscoded every cancel notification (`state:2`) as Open, so the WS overwrite via `store.addOrUpdateOrders` resurrected every canceled plan order right after the cancel HTTP succeeded — a network trace showed dozens of cancel POSTs returning `success:true code:0`, dozens of `push.personal.plan.order state:2` frames landing, and yet the chart lit up with all the "canceled" rows still Open.
- The `applyStopLikeFrame` shield in `mexc.ws-private.ts` — which drops any Open plan/stop frame whose id sits in `recentlyCanceledStopLikeOrderIds` — is defense-in-depth for legitimate stale Open frames (WS reconnect replay, mid-flight snapshots that predate the cancel). It is **not** a substitute for correct state normalization: without the trigger-order state fix above, cancel frames arrive with `state:2` and would still be misread as Open, at which point the shield check only helps for ids whose local cancel-shield entry was set before the WS frame was processed — which is a losing race during rapid bursts where the WS frame beats the HTTP response. Canceled / Closed frames are always honored regardless of the shield.
- `handleFill` relies on MEXC emitting a companion `push.personal.order` with cumulative `dealVol` alongside every `push.personal.order.deal` frame; incrementing `filled` from the per-fill payload would double-count when the two frames race. If that invariant ever breaks, the 2s tick's REST refresh corrects it — do not add a "belt-and-suspenders" increment in the fill handler.
- The periodic `tick` uses a regular-open-orders-only refresh and preserves existing open plan/position-stop rows in store. Use full `fetchOrders()` only when TP/SL/stop visibility is needed, because it fans out to regular, plan, and stop-order list endpoints.
- `fetchAndSetRegularOrders` also preserves in-store open regular rows that MEXC's `open_orders` REST snapshot has not reflected yet (WS wrote them or `placeOrder` seeded them optimistically), bounded by `MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS`. Without this window the 2s tick's wholesale replace would drop a just-placed / just-moved order for one tick cycle, producing the "order disappears then reappears" symptom users see when moving lines. WS reconnect's `fetchAndSetPrivateSnapshot` remains the wholesale-replace authority; any regular row older than the window without a REST anchor is treated as a WS-missed cancellation and dropped.
- After `placeOrder` (regular) and `placeRegularOrdersBatch` succeed, seed an optimistic Open row via `addOptimisticRegularOrder` (info marker `__mexcOptimisticRegular`) so the store has a stable id-keyed anchor before the WS `push.personal.order` frame lands. The subsequent WS event overwrites the marker with the real payload. Market orders are skipped because they transition Open → Filled fast enough that the optimistic row would only flash.
- MEXC's placement HTTP `200` on `POST /order/submit` frequently arrives AFTER the WS lifecycle frames for the same order. Live traces have shown `push.personal.order state=2` (Open) at t+1013 ms and `push.personal.order state=4 errorCode=10` (Canceled by venue) at t+1014 ms, with the HTTP `200` landing at t+1042 ms. `addOptimisticRegularOrder` therefore checks the current store row and the rejection shield before writing so it does not resurrect a row the venue has already canceled.
- MEXC futures rejects reduce-only orders whose combined open close size would exceed the position (unlike Binance/Bybit which allow oversubscribed reduce-only stacks). The venue signals this by delivering `state=2` and `state=4 errorCode>0 dealVol=0` frames ~1 ms apart. `handleOrder` detects that pattern, calls `MexcExchange.noteRejectedRegularOrder(orderId, errorCode)`, and emits a user-visible error explaining the rejection. The id is shielded for `MEXC_RECENT_REJECTED_ORDER_SHIELD_MS` so the optimistic-add and preservation-window paths do not resurrect it. Known cancel `errorCode` values are translated by `MEXC_ORDER_CANCEL_REASONS`; unmapped codes fall back to a generic message that includes the numeric code.
- The known-cancel reason map is best-effort — MEXC does not publish a canonical `errorCode` enum for `state=4` frames. Add new codes to `MEXC_ORDER_CANCEL_REASONS` (in `mexc.exchange.ts`) when live traces confirm the mapping; do not guess without evidence.
- `push.personal.asset` is intentionally treated as a trigger only (not a data payload) — the 1s-debounced `throttledFetchBalance` call from the WS asset handler is the only exception to the "no REST from WS" rule, because we do not parse the asset frame. Plan/stop order WS handlers must remain REST-free; the trailing-debounced `scheduleStopOrderSnapshotRefresh` from the placement path is the reconciliation authority for TP/SL lag.
- Symbol-scoped full-order refreshes should merge in existing open regular rows not present in that snapshot. MEXC's regular and plan/stop endpoints can lag independently, and the delayed TP/SL refresh after placing a stop must not erase a reduce-only close limit that arrived through WS or a separate regular-order refresh.
- Standalone plan-order cancel and position TP/SL cancel return only a common success envelope, not regular-cancel per-order rows. After an accepted cancel, keep the stop/plan id shielded briefly from full-order snapshots so a lagging MEXC list response does not re-open the row locally.
- The paced sequential fallback preserves partial-placement metadata. If one leg fails after earlier legs returned order ids, `placeOrders` annotates the thrown error with `partialOrderIds`, `partialOrderCount`, `totalOrderCount`, and `failedOrderIndex` so the web order layer can tell the user that the batch halted partially.
- Standalone sized TP/SL uses `POST /api/v1/private/planorder/place/v2` as a reduce-only market plan order, seven-day execution cycle (`executeCycle: 2`), and explicit quantity. Trigger direction is inferred from order type plus closing side.
- After a successful standalone TP/SL or position TP/SL placement, add a local open stop row immediately via `addOptimisticStopOrder`, register the id in the placement shield (`recentlyPlacedStopLikeOrderIds`, TTL `MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS`), and schedule a coalesced full-order refresh via `scheduleStopOrderSnapshotRefresh`. MEXC contract private WS _does_ emit `push.personal.plan.order` and `push.personal.stop.order` frames (see filter guidance above), so once the WS frame lands the shield is released and the venue row takes over. The shield exists because MEXC's `planorder/list` and `stoporder/list` REST snapshots are visibly laggier than `open_orders`: during rapid TP/SL bursts the placement HTTP `200` returns an id that the next `fetchAndSetOrders([symbol])` REST snapshot does not yet contain, which without the shield would wholesale-drop the optimistic row from the chart until the snapshot catches up. This mirrors the Bybit `recentlyPlacedOrders` / `preserveRecentlyPlacedOrders` pattern.
- `scheduleStopOrderSnapshotRefresh` is trailing-debounced per symbol via `pendingStopSnapshotRefreshTimers`. Rapid drag-place bursts collapse to a single delayed `fetchAndSetOrders([symbol])` per symbol per burst, avoiding a cascade of `GET /planorder/list/orders` + `GET /stoporder/list/orders` calls that used to flood the network tab (visible as duplicated `planorder/list` / `stoporder/list` rows within a couple hundred ms of each other during TP/SL storms). Pending timers are cleared in `dispose()` so a delayed refresh never runs against a torn-down exchange.
- Full-position / infinity TP/SL uses the separate position TP/SL API: `POST /api/v1/private/stoporder/place` with the native `positionId`, current native position `vol`, and `volType: 2`. These rows are listed with `GET /api/v1/private/stoporder/list/orders?is_finished=0` and canceled through `POST /api/v1/private/stoporder/cancel`; do not send them to the standalone `planorder` cancel route.
- Open TP/SL orders do not appear in `/api/v1/private/order/list/open_orders`. Fetch untriggered plan orders with `GET /api/v1/private/planorder/list/orders?states=1` and normalize them into regular Tealstreet order state so REST reconciliation does not erase optimistic chart lines after placement. `fetchAndSetOrders` preserves shielded stop-like rows (still-Open, not recently canceled, id in `recentlyPlacedStopLikeOrderIds` and within `MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS`) across both symbol-scoped and full-scope wholesale replaces; without that preservation, a rapid REST refresh right after placement wipes the just-added chart line for one full snapshot cycle and users see flicker.
- Cancel standalone TP/SL through `POST /api/v1/private/planorder/cancel`; regular order cancellation does not apply to these plan-order ids.
- Margin mode changes use `POST /api/v1/private/position/change_leverage` with the current leverage plus `openType`; MEXC has no `change_margin_type` endpoint.
- For an open position, leverage updates must target the native `positionId` path. Use the `symbol` + `openType` + `positionType` shape only for flat rails; otherwise MEXC can acknowledge the HTTP request without changing the live position leverage, and later REST/WS reconciliation will show the old venue value again.
- Treat MEXC common mutation envelopes (`success: false`, non-zero `code`) as failures before mutating local leverage or margin-mode cache.
- Position mode is account-level. After a successful mode change, update cached per-symbol metadata so flat fake positions do not revert the UI to the previous mode.
- Active-symbol metadata refresh (`updateSymbolMetadata`) must call `GET /api/v1/private/position/leverage` even when the symbol is flat. Open-position rows are absent while flat, so deriving from positions alone falls back to 20x and makes first-load order validation use the wrong leverage.
- Regular and standalone plan-order placement must hydrate missing per-symbol metadata before formatting the payload. `PlaceOrderOpts` does not carry margin mode or leverage, so a cold `symbolSettings` cache would otherwise submit `openType: 2` / `leverage: 20` even when MEXC already has the symbol configured as isolated with custom side leverage.
- Contract detail can advertise a high top-level `maxLeverage` (for example BTC_USDT 500x) while the authenticated account/symbol cap is lower. Prefer `GET /api/v1/private/position/leverage` `maxLeverageView` for slider/display caps when credentials are available.
- In hedge mode, long and short leverage can differ on the same ticker. Preserve separate `buyLeverage` and `sellLeverage` rails when fetching, setting leverage, and publishing fake flat positions during periodic position refreshes.
- Trailing stops and opening trigger orders are intentionally not advertised or implemented in the MVP.

## Current Validation Status

Public REST contract detail, ticker, depth, and kline endpoints were probed successfully without credentials. Authenticated REST write and private WebSocket probes still require `MEXC_API_KEY` and `MEXC_API_SECRET`.

## Client order ID

This venue does not honor `PlaceOrderOpts.clientOrderId`: `placeOrder` / `placeOrders` reject any caller ID before a write (`packages/safe-cex/CLAUDE.md` → "Client order ID contract"). Keep that guard as the first statement of any new placement override.

## Rust engine

`sdk-rs/venues/mexc` uses native placement volume and base-sized store rows.
The frame applier reconstructs `remainingNative` and
`filledInPlacementUnits` from the contract-size column for MEXC on both
sides of the worker boundary. Optional buy/sell leverage columns in
`SETTINGS` preserve different hedge rails even when flat.

Regular cancel/replace reads `/api/v1/private/order/get/{orderId}` after
cancel acceptance before replacing. An unconfirmed cancellation or failed
read withholds the replacement; a definitive completed row updates local
fills and stops replacement. This avoids replacing fills whose WS frames
arrive after the cancellation response. Newer WS settings win over delayed
mutation acknowledgements, with revisions tracked separately per hedge rail.

Regular cumulative fills never regress across WS frames, independently of
status. Terminal rows accept a delayed Open frame's larger cumulative fill
while retaining their terminal status; Closed takes precedence. Older terminal
frames and amend reads cannot regress quantity; a stale final read withholds replacement even when the
caller supplied an explicit new amount. Stop replacements also abort if the
original executes before cancellation is acknowledged.

The Rust gate's `KNOWN_TS_FIXES.mexc` lists intentional corrections to the TS
reference, including full-depth sequencing, closing leverage for sized stops,
native attached brackets, and cancel-all across all three order surfaces.

History queries in the Rust adapter use streamed pages of at most 100 rows.
Executions use floor-seconds timestamps; order history, funding income and
public trades retain milliseconds to match the engine's query result shapes.
Only explicit `FUNDING_FEE` income is supported; an all-kind income request
fails because the funding endpoint cannot supply a complete general ledger.
This is additional Rust functionality: TS MEXC inherits the empty branded
`BaseExchange.fetchIncomeHistory` / `fetchIncomeHistoryRange` fallbacks and
does not implement an income ledger. Neither TS nor Rust implements opening
conditional orders or trailing stops. Before the Rust port, Conditional fell
through to a regular type-1 limit submit without trigger fields; rejecting it
prevents an unintended immediate order. Private replay covers whole-batch
preflight for both unsupported types alongside caller client IDs.
The public deals feed exposes no execution ID, so its result keys include the
response ordinal and preserve otherwise identical prints.

Private execution IDs are deduplicated for five minutes. Cumulative regular
fill volume is monotonic across WS updates, including updates arriving after
a cancel acknowledgement; terminal status is retained without dropping fills.

The TS reference applies the same cumulative-fill and terminal-status guards
when committing full or symbol-scoped REST snapshots, private WS frames, and
optimistic placement acknowledgements. Attached entry TP/SL prices and their independent
`lossTrend` / `profitTrend` references survive normalization and cancel/replace.
The definitive post-cancel read supplies updated bracket fields; omitted fields
retain known metadata, while an explicit zero bracket price clears that leg.
Standalone plan `trend` and position-stop `lossTrend` / `profitTrend` map to
the corresponding public trigger references and survive price amendments.
Full-position stop replacements retain the zero-size sentinel and native
position-stop route after a WS row supplies the actual native position volume.

Private regular and plan/stop frames share the snapshot metadata merge: omitted
bracket prices or trigger references preserve known values, including optimistic
placement sources, while an explicit zero bracket price clears its leg.

When native WS precedes the placement acknowledgement, its explicit bracket
prices and sources take precedence over the request; the optimistic row only
supplies metadata omitted from the native snapshot.

Carry native bracket field presence through sparse snapshots as well as public
values: a clear followed by an omitted field still overrides a late placement
acknowledgement, rather than restoring the requested bracket.

Rust `setLeverage(symbol, leverage, side)` changes only the selected Long or
Short rail, using its native `positionId` when open and `positionType` when
flat. Omitting side changes both rails. A confirmed write updates that leg's
position and fetched leverage; a failed write preserves its previous setting.

Rust also subscribes to the documented all-contract `sub.tickers` stream so
last/mark/index prices and 24-hour fields stay live beyond the first 100 detailed
subscriptions. Its sparse rows preserve omitted or null values; zero last/mark/index
prices mean unavailable and preserve the previous quote while other valid fields
still update. Unknown or inactive symbols outside the market catalog are ignored
before field validation.
`SetTickerSymbols` reconciles all requested known detailed subscriptions without
duplicates, preserves caller priority, and survives reconnects. With no selection,
the first 100 markets keep the detailed feed. See the [official futures WS contract](https://mexcdevelop.github.io/apidocs/contract_v1_en/#tickers).

Typed open-order rows whose market is unavailable are logged and skipped,
while any existing copy of their id survives reconciliation. Other valid rows
still load; a genuinely empty full snapshot remains authoritative. Malformed
wire rows and rows without a usable native order id still fail the snapshot
rather than silently clearing known orders.

Full-position TP/SL snaps the position's native volume to `volUnit` before
signing. A singleton Rust placement uses `/order/submit` with the regular
450 ms FIFO pacing; multi-order batches retain the batch entitlement fallback
and 1.5 s scaled pacing, including the existing throttle cooldown.

## Rust snapshot and plan reads

The Rust adapter supports independent `fetchOrderBook(symbol, limit)` public
snapshots without subscriptions. Standalone MEXC builds opt into the precise
order-book JSON decoder, so the advertised SDK method works without unrelated
venue features. Default 100, SDK range 1–1000; amounts and
cumulative totals use base units via captured `contractSize`. Versions remain
exact strings. A query cannot replace a live book. Public endpoint failures do
not provide evidence of invalid account credentials. Callers control retries.

Opening plan sides 1/3 are reduce-only only when the native flag says so;
closing sides 2/4 and dedicated position stops are reduce-only. Native fill-fee
sign is retained until debit/rebate conventions are independently verified.
