# HIP-3 Data Flow Report

Last updated: 2026-07-15

## Current Status

HIP-3 positions, fills, and order lifecycle use dedicated WebSocket feeds. Mid and mark prices use `fastAssetCtxs` with `allMids` retained as a full-snapshot fallback. REST remains required for startup metadata, full order metadata, and bounded reconciliation.

`webData2` is rejected by the upstream server and has no subscription or handler. `webData3` was probed on mainnet and testnet on 2026-07-15; it returned only `userState` and `perpDexStates`, so it does not replace any trading-state feed and is deliberately not subscribed.

## Strategic Feed Evaluation (2026-07-15)

Credential-free mainnet and testnet probes established these current contracts:

| Feed | Evidence and decision |
| --- | --- |
| `fastAssetCtxs` | Accepted on both networks. It sends base64 raw-DEFLATE JSON: an initial coin-keyed snapshot followed by partial `markPx` / `midPx` diffs. Adopted on the perp public socket because `ticker.mark` directly prices market and TP/SL slippage caps; `allMids` remains the fallback. |
| `allDexsAssetCtxs` | Accepted, but live `ctxs` is an array of `[dex, AssetCtx[]]` tuples (10 mainnet / 240 testnet during the probe), not the record shape in the published TypeScript contract. It contains no universe metadata, so it cannot replace per-DEX `metaAndAssetCtxs` and is not adopted. |
| `allDexsClearinghouseState` | Accepted with the same tuple-array contract mismatch and full all-DEX snapshots. Existing dex-scoped streams already provide authoritative closure semantics, while the aggregate testnet frame includes hundreds of unloaded DEXes; not adopted. |
| `webData3` | Contains `userState` plus `perpDexStates`, with no positions, orders, or asset contexts. There is no trading-state consumer; not adopted. |

On the production `api-ui.hyperliquid.xyz` socket, a six-frame probe measured an 11.7 KB compressed initial snapshot (1,267 coins, 55.6 KB inflated) decoded by pako in 2.97 ms. Five steady-state diffs arrived about once per second after the immediate post-snapshot diff; they were 0.6–1.13 KB compressed and decoded in 0.19–0.33 ms. This keeps synchronous decode below the existing JSON/store-update work and avoids asynchronous frame reordering.

The standard-depth `l2Book` plus `bbo` overlay and unified/portfolio `spotState` path are independent and unchanged by this evaluation.

## WebSocket Subscriptions per HIP-3 DEX

All use `dex: "<dexName>"` on the official API WebSocket.

| Channel              | Delivers                    | Role                                      |
| -------------------- | --------------------------- | ----------------------------------------- |
| `allMids`            | Published mid prices        | Live `last` / `midPrice` ticker updates   |
| `clearinghouseState` | Positions + dex margin      | Primary position source                   |
| `userFills`          | Fill notifications          | Instant fills + cross-context dedup       |
| `orderUpdates`       | Incremental order lifecycle | Open/filled/cancelled/terminal state      |
| `userEvents`         | User lifecycle events       | Backup lifecycle signal                   |

`activeAssetData` is not subscribed for HIP-3 because per-market fan-out would consume too much of the 1000-subscription limit.

### Known upstream order limitation

HIP-3 `orderUpdates` can omit:

- `orderType`
- `triggerPx` / `tpsl`
- `tif`

The integration therefore cannot derive every trigger order from the live update alone.

## REST Inventory

### Startup

| Endpoint                              | Purpose                     | Behavior                                      |
| ------------------------------------- | --------------------------- | --------------------------------------------- |
| `metaAndAssetCtxs`                    | Core markets + tickers      | Required before public hydration              |
| `perpDexs`                            | HIP-3 DEX discovery         | Bounded by a 6-second startup timeout          |
| `metaAndAssetCtxs` per DEX            | HIP-3 markets + ticker seed | Strictly sequential, 8-second total budget    |
| `userFees`                            | Fee rates                   | Defaults safely if unavailable                 |
| `maxBuilderFee`                       | Builder approval            | Approval-gated order attachment                |
| `frontendOpenOrders` core + per DEX   | Orders with type metadata   | Authoritative order reconciliation             |
| `clearinghouseState` core + per DEX   | Initial positions           | Authoritative startup/reconnect reconciliation |

Official IP limits are 1200 weight/minute; `metaAndAssetCtxs` costs 20 and `userRole` costs 60. Live discovery on 2026-07-15 returned one core plus nine HIP-3 entries on mainnet. Testnet returned a much larger catalog and remains restricted to `vntls`, `xyz`, `VOLMEX`, and `flx`.

### Periodic and event-driven

| Endpoint                         | Trigger                 | Purpose                                      |
| -------------------------------- | ----------------------- | -------------------------------------------- |
| `clearinghouseState` per DEX     | Every 120 seconds       | Position safety net                          |
| `frontendOpenOrders` retry chain | HIP-3 order placement   | Confirm order type/trigger/tif               |
| `frontendOpenOrders` debounced   | Unknown live order      | Repair cache miss                            |
| `frontendOpenOrders` debounced   | HIP-3 position closure  | Confirm automatic TP/SL cancellation         |

## Reconciliation Architecture

```text
Core positions: clearinghouseState WS + authoritative REST bootstrap/reconnect

HIP-3 per DEX:
  fastAssetCtxs ----------> ticker mark + last/mid (coin-keyed snapshot/diffs)
  allMids ----------------> ticker last/mid fallback
  clearinghouseState ----> dex-scoped positions
  userFills -------------> fill events + realized PnL
  orderUpdates ----------> live order state
  userEvents ------------> backup lifecycle events
  REST ------------------> order metadata + position safety nets

HIP-3 order metadata:
  pre-placement cache ---> type/price before POST
  pending cache ---------> order id -> type/price after POST
  optimistic store ------> immediate correct UI representation
  REST retry chain ------> authoritative frontendOpenOrders confirmation
```

### Position rules

- A dex-scoped empty `assetPositions` array is authoritative for closures on that DEX.
- Core positions and positions from other HIP-3 DEXes are preserved during a dex update.
- Dex-scoped `marginSummary` must never overwrite the global core/unified balance.

### Order rules

- `orderUpdates` is the only live order feed.
- Pending/pre-placement caches preserve known HIP-3 order metadata.
- An unknown HIP-3 update gets a temporary creating-state placeholder and triggers bounded REST refresh.
- Terminal updates and authoritative REST reconciliation remove orders.
- Cancellation suppression prevents delayed updates from resurrecting recently cancelled orders.

## Remaining Gaps

| Area                    | Gap                                                                      | Mitigation                                    |
| ----------------------- | ------------------------------------------------------------------------ | --------------------------------------------- |
| HIP-3 order metadata    | Live updates omit type/trigger fields                                    | Caches + optimistic order + bounded REST      |
| HIP-3 ticker contexts   | Fast feed supplies mark/mid; index/funding seed from startup REST         | Keep ordinal aggregate feeds gated on stable contract |
| HIP-3 leverage defaults | No per-market `activeAssetData` subscription due subscription headroom  | Position payload when open; defaults when flat |
| Position safety net     | Per-DEX REST reconciliation every 120 seconds                            | Retained for disconnect/missed-frame recovery |

## Monitoring

| Log                                                               | Meaning                                  | Action                                      |
| ----------------------------------------------------------------- | ---------------------------------------- | ------------------------------------------- |
| `HIP-3 order type inference: source=hip3-unconfirmed-placeholder` | Cache miss pending REST confirmation     | Investigate if frequent in steady state     |
| `Cancel suppression expired for order`                            | REST still reports an old cancelled order | Check whether the cancel failed upstream    |
| `Failed to fetch orders from HIP-3 dex`                           | One DEX reconciliation failed            | Existing orders for that DEX stay preserved |
