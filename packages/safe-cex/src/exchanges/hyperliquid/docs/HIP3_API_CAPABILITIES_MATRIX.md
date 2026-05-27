# HIP-3 Data Flow Report

Last updated: 2026-02-06

## Current Status

WS coverage for HIP-3: ~90%. REST is only used for startup, order type inference, and a 120s position safety net.

---

## WS Subscriptions (per HIP-3 dex)

All use `dex: "<dexName>"` on the official API WS (`wss://api-ui.hyperliquid.xyz/ws`).

| Channel              | Delivers                         | Frequency           | Role                                             |
| -------------------- | -------------------------------- | ------------------- | ------------------------------------------------ |
| `clearinghouseState` | Positions + balance              | Real-time           | Primary position source                          |
| `webData2`           | Positions + orders + market data | ~0.4 msg/s snapshot | Additive-only for orders; position + market data |
| `userFills`          | Fill notifications               | Instant             | Fill events + dedup                              |
| `orderUpdates`       | Order status changes             | Instant             | Order open/filled/canceled                       |
| `userEvents`         | Order lifecycle                  | Event-driven        | Backup order channel                             |

### Key API limitations for HIP-3

HIP-3 `orderUpdates` omits these fields (core perps include them):

- `orderType` (Limit / Stop Market / Take Profit Market)
- `triggerPx` (trigger price for TP/SL)
- `tpsl` (tp or sl)
- `tif` (time in force -- sometimes present, sometimes missing)

These are only available from REST `frontendOpenOrders`.

### Dex-scoped marginSummary

`clearinghouseState` with `dex` returns margin for that specific dex only, NOT the full account balance. Global balance must come from core perps `clearinghouseState` (without dex). HIP-3 dex `marginSummary` is intentionally ignored in the balance update path.

---

## REST Calls Inventory

### Startup (one-time, required)

| Endpoint                              | Purpose               | Replaceable by WS?                          |
| ------------------------------------- | --------------------- | ------------------------------------------- |
| `metaAndAssetCtxs`                    | Markets + tickers     | No -- needed before WS subscribes           |
| `perpDexs`                            | HIP-3 dex list        | No -- needed to know which dexes exist      |
| `metaAndAssetCtxs` per dex            | HIP-3 market metadata | No -- needed before WS subscribes           |
| `userFees`                            | Fee rates             | No -- not in WS                             |
| Builder fee check                     | Approval status       | No -- not in WS                             |
| `frontendOpenOrders` (core + per dex) | Orders with type info | Partially -- WS has orders but missing type |
| `clearinghouseState` per dex          | Initial positions     | Partially -- WS delivers within seconds     |

### Periodic

| Endpoint                     | Interval | Purpose             | Removable?                                            |
| ---------------------------- | -------- | ------------------- | ----------------------------------------------------- |
| `clearinghouseState` per dex | 120s     | Position safety net | Yes -- monitor for a few weeks then consider removing |

### Event-driven

| Endpoint                         | Trigger                     | Purpose                               | Removable?                                         |
| -------------------------------- | --------------------------- | ------------------------------------- | -------------------------------------------------- |
| `frontendOpenOrders` retry chain | After HIP-3 order placement | Get `orderType` / `triggerPx` / `tif` | No -- API limitation                               |
| `frontendOpenOrders` debounced   | On `scheduleOrderRefresh()` | Unknown order from WS (cache miss)    | No -- API limitation                               |
| `frontendOpenOrders` debounced   | After HIP-3 position closes | Confirm auto-cancelled TP/SL          | Mostly -- WS sends cancel status but REST confirms |

### Write operations (always REST)

| Endpoint                | Trigger                                         |
| ----------------------- | ----------------------------------------------- |
| `/exchange`             | Order placement, cancellation, leverage changes |
| `/info` candleSnapshot  | Chart loads candles                             |
| `/info` userFillsByTime | Executions tab loads history                    |

---

## Architecture

```
CORE PERPS:
  Official WS -----> Store

HIP-3 (per dex):
  Official WS (real-time) ---+--> Store (primary)
  REST (120s safety net) ----+    (fallback only)

  WS channels per dex:
  +-- clearinghouseState --> Positions + Balance (real-time, primary)
  +-- webData2 -----------> Positions + Orders + Tickers (snapshot, additive-only for orders)
  +-- userFills ----------> Fills (instant)
  +-- orderUpdates -------> Order status (instant, missing type/tif for HIP-3)
  +-- userEvents ---------> Order lifecycle (backup)

  Order type gap workaround:
  +-- Pre-placement cache -> type cached BEFORE REST call (keyed by coin|side)
  +-- Pending cache -------> type cached AFTER REST returns order ID (keyed by orderId)
  +-- Optimistic order ----> correct type/price in store immediately
  +-- REST retry chain ----> frontendOpenOrders confirms type (300ms to 5s backoff)
```

---

## Bugs Found and Fixed During Development

### Data flow bugs

| Bug                                               | Root Cause                                                                                                        | Fix                                                                                                     |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Orders flicker/vanish for 1-2s                    | `webData2` full array replacement wiped orders from other sources                                                 | HIP-3 `webData2` openOrders is now additive-only; core perps path preserves HIP-3 orders                |
| Margin balance shows zero momentarily             | HIP-3 dex `marginSummary` (dex-scoped) overwrote global balance                                                   | Only core perps `marginSummary` updates global balance                                                  |
| Closed position lingers 1-2 minutes               | Empty `assetPositions` from dex `clearinghouseState` misdetected as core perps (`.some()` on empty array = false) | Use `data.dex` field as primary HIP-3 detection; handle empty/missing `assetPositions` for dex messages |
| Cancelled orders reappear                         | No suppression mechanism; stale WS snapshots re-added cancelled orders                                            | `_cancelledOrders` suppression map + optimistic removal + REST reconciliation                           |
| Stale `webData2` snapshot wipes optimistic orders | Optimistic orders (Creating/Updating) dropped during full array replacement                                       | Optimistic orders preserved through snapshot replacements                                               |
| `fetchOrders()` removes valid orders              | `currentOrders` captured before async REST calls; stale reference used in removal step                            | Re-capture from store after REST completes; skip removal for failed dex fetches                         |

### Order type inference bugs

| Bug                                        | Root Cause                                                                                                          | Fix                                                                             |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| TP shows as SL after cancel-all + re-place | Stale pre-placement cache entries from cancelled orders matched by FIFO                                             | Consume entries when pending cache is found; clear entries on cancel            |
| TP/SL appears at position price initially  | Pre-placement cache used type-priority matching                                                                     | Match by price, then amount, then FIFO; clear on cancel                         |
| PostOnly `[P]` tag disappears from chart   | Chart line `needsUpdate` check didn't track `timeInForce`; async line creation used stale order data                | Added `lastTimeInForce` tracking; use latest store order in async callback      |
| PostOnly lost after WS/REST sync           | `timeInForce` not included in `fetchOrders()` diff check; `webData2` openOrders didn't fall back to store for `tif` | Added `timeInForce` to surgical update diff; fall back to store for all markets |
| Optimistic order missing `timeInForce`     | `placeLimitOrder` didn't include `opts.timeInForce` on optimistic order                                             | Added `timeInForce` to optimistic order creation                                |

---

## What's Optimal

| Area               | Status  | Details                                                                             |
| ------------------ | ------- | ----------------------------------------------------------------------------------- |
| Position updates   | Optimal | `clearinghouseState` with `dex` delivers real-time; no REST after fills             |
| Fill notifications | Optimal | `userFills` with `dex` delivers instant; cross-cache deduplication prevents doubles |
| Order status       | Optimal | `orderUpdates` with `dex` delivers instant open/filled/canceled                     |
| Market data        | Optimal | `webData2` delivers mark/index/OI/funding/volume in real-time                       |
| Balance            | Optimal | Core perps `clearinghouseState` (full account); HIP-3 dex-scoped ignored            |
| Cancel UX          | Optimal | Immediate removal + suppression; REST reconciles after 30s                          |
| Core perps         | Optimal | Official WS delivers real-time data                                                 |

## What's Suboptimal

| Area                             | Issue                                                                                    | Severity                                       |
| -------------------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Order type for HIP-3             | WS omits `orderType`/`triggerPx`/`tpsl` -- requires REST + caching                       | Medium (API limitation)                        |
| `timeInForce` for HIP-3          | WS sometimes omits `tif` -- requires store fallback                                      | Low (mitigated by store preservation)          |
| `webData2` snapshot ordering     | Stale snapshots can temporarily show incomplete data during rapid operations             | Low (additive-only for HIP-3 orders mitigates) |
| 120s periodic position REST      | Likely redundant with `clearinghouseState` + `webData2` WS                               | Very low (negligible API load)                 |
| `activeAssetData` for HIP-3      | Not subscribed (channel limit concern) -- leverage defaults until WS delivers            | Low                                            |
| `addOrUpdateOrdersFull` in store | Full array replacement from `webData2` snapshots; causes unnecessary React notifications | Low (functional but wasteful)                  |

---

## Improvements to Request from Hyperliquid

| Request                                                                  | Impact                                                                                             | Effort for HL                |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- | ---------------------------- |
| Add `orderType` / `triggerPx` / `tpsl` to HIP-3 `orderUpdates`           | **Major** -- eliminates all order-type REST calls, pre-placement cache, pending cache, retry chain | Low (parity with core perps) |
| Add `tif` consistently to HIP-3 `orderUpdates` and `webData2` openOrders | Medium -- eliminates `timeInForce` store fallback logic                                            | Low                          |
| Add `dex` field to `orderUpdates` / `userFills` responses                | Minor -- easier message routing without coin-format detection                                      | Low                          |
| Support `activeAssetData` for HIP-3 markets                              | Minor -- leverage/margin info per HIP-3 market                                                     | Low                          |

## Internal Improvements to Consider

| Improvement                                                               | Impact                                                          | Risk                                     |
| ------------------------------------------------------------------------- | --------------------------------------------------------------- | ---------------------------------------- |
| Remove 120s periodic position REST                                        | Minor -- saves 0.5 REST calls/min/dex                           | Low -- monitor for a few weeks first     |
| Replace `addOrUpdateOrdersFull` with surgical updates in `handleWebData2` | Minor -- fewer React re-renders from `webData2` snapshots       | Medium -- changes shared store method    |
| Remove `webData2` dex openOrders processing entirely                      | Medium -- eliminates snapshot-vs-realtime ordering issues       | Medium -- lose snapshot-based order sync |
| Track `orderType` in chart line manager's `needsUpdate`                   | Minor -- chart lines update when type changes (TP/SL inference) | Low                                      |

---

## Monitoring

Watch for these debug log patterns:

| Log                                                               | Meaning                                            | Action                                                                |
| ----------------------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------- |
| `HIP-3 order type inference: source=hip3-unconfirmed-placeholder` | Cache miss, order type unknown until REST confirms | Normal during early init; investigate if frequent during steady state |
| `Cancel suppression expired for order`                            | REST shows cancelled order as still open after 30s | May indicate cancel failure; check exchange UI                        |
| `Failed to fetch orders from HIP-3 dex`                           | REST fetch failed for a dex                        | Orders for that dex are preserved (not removed)                       |
