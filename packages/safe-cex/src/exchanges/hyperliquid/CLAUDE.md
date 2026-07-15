# Hyperliquid Exchange Integration

This directory contains both the Hyperliquid perpetual futures and spot exchange integrations for Tealstreet.

## Exchange Information

- **Type**: Decentralized exchange (DEX) on Hyperliquid L1
- **Authentication**: Ethereum wallet signing (EIP-712), no traditional API key/secret
- **Settlement**: USDC
- **Position Mode**: OneWay only (no hedged mode)
- **Special Features**: HIP-3 builder-deployed perpetuals, vault trading

## API Documentation

- **Official API docs**: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api
- **Builder codes**: https://hyperliquid.gitbook.io/hyperliquid-docs/trading/builder-codes
- **WebSocket subscriptions**: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
- **CCXT REST reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/hyperliquid.ts
- **CCXT WS reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/hyperliquid.ts
- **HIP-3 data flow report**: `docs/HIP3_API_CAPABILITIES_MATRIX.md` (local, detailed WS/REST coverage)

## File Structure

### Core Perpetual Futures

| File                         | Description                                                                                                                                                                                                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hyperliquid.exchange.ts`    | Main exchange class extending `BaseExchange`. Orchestrates startup, order/position management, HIP-3 lifecycle, builder fee, cancel suppression, and tick loop.                                                                                                                                               |
| `hyperliquid.api.ts`         | Axios client factory. Creates two clients: `createExchangeAPI` (for `/exchange` endpoint) and `createInfoAPI` (for `/info` endpoint).                                                                                                                                                                         |
| `hyperliquid.types.ts`       | All TypeScript interfaces: REST responses, WS payloads, order wires, HIP-3 types (`PerpDex`, `PerpDexs`), builder fee types, candle types, etc. Also defines `BASE_URLS`, `WSS_URLS`, `ENDPOINTS`, `INFO_TYPES`, `EXCHANGE_TYPES`, `WEBSOCKET` constants.                                                     |
| `hyperliquid.constants.ts`   | WS subscription type lists (`singularSubscriptionTypes`, `systemSubscriptionTypes`), Tealstreet builder wallet addresses (mainnet/testnet), builder fee amount (10 tenths bps = 1bp = 0.01%).                                                                                                                 |
| `hyperliquid.signing.ts`     | EIP-712 phantom agent signing, L1 action signing, user-signed action signing (agent approval, builder fee, USD transfer, withdraw). Contains `floatToWire`, `orderRequestToOrderWire`, order action builders (`orderWiresToOrderAction`, `orderWiresToNormalTPSLAction`, `orderWiresToPositionalTPSLAction`). |
| `hyperliquid.ws-public.ts`   | Public WebSocket: `allMids` ticker updates, `l2Book` orderbook, `candle` OHLCV. Connects to `wss://api-ui.hyperliquid.xyz/ws`.                                                                                                                                                                                |
| `hyperliquid.ws-private.ts`  | Private WebSocket handling `clearinghouseState`, mode-gated `spotState` (unified/portfolio), HIP-3 `allMids`, `activeAssetData`, `orderUpdates`, `userFills`, `userEvents`, `userFundings`, and `notification`.                                                                                              |
| `hyperliquid.utils.ts`       | `symbolToCoin()` (display symbol to API coin format), `calculatePricePrecision()` (CCXT-compatible price precision algorithm).                                                                                                                                                                                |
| `hyperliquid.errors.ts`      | `HyperliquidAPIError` class and `handleApiError()` utility.                                                                                                                                                                                                                                                   |
| `hyperliquid.rateLimiter.ts` | Token bucket rate limiter (1200 tokens/minute capacity).                                                                                                                                                                                                                                                      |
| `hyperliquid.agent.ts`       | Agent wallet (API wallet) creation and approval via browser wallet (MetaMask). Handles EIP-712 `ApproveAgent` signing, network switching (Arbitrum One / Sepolia).                                                                                                                                            |
| `hyperliquid.builderFee.ts`  | Builder fee approval and checking. EIP-712 `ApproveBuilderFee` signing, `checkBuilderFeeApproved()` REST check, `approveBuilderFeeWithBrowserWallet()` full browser flow.                                                                                                                                     |
| `hyperliquid.cross-cache.ts` | Cross-context singleton cache for HIP-3 order type tracking and fill deduplication. Uses `sessionStorage` + `BroadcastChannel` for cross-worker/iframe synchronization. Manages pre-placement cache, pending orders cache, and seen fills set.                                                                |

### Spot Trading

| File                             | Description                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------- |
| `hyperliquidspot.exchange.ts`    | Spot exchange class extending `BaseExchange`. Uses `spotMetaAndAssetCtxs` for markets. |
| `hyperliquidspot.api.ts`         | Spot REST API client (same base pattern as perps).                                     |
| `hyperliquidspot.types.ts`       | Spot-specific type definitions and constants.                                          |
| `hyperliquidspot.constants.ts`   | Spot constants.                                                                        |
| `hyperliquidspot.signing.ts`     | Spot request signing (same EIP-712 pattern).                                           |
| `hyperliquidspot.errors.ts`      | Spot error handling.                                                                   |
| `hyperliquidspot.rateLimiter.ts` | Spot rate limiter.                                                                     |
| `hyperliquidspot.ws-public.ts`   | Spot public WebSocket.                                                                 |
| `hyperliquidspot.ws-private.ts`  | Spot private WebSocket.                                                                |

### Other

| File                                   | Description                                                                                           |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `docs/HIP3_API_CAPABILITIES_MATRIX.md` | Detailed report on HIP-3 WS/REST coverage, data flow architecture, known bugs, and improvement areas. |
| `.gitignore`                           | Ignores `.apikey.json`.                                                                               |

## Authentication

Hyperliquid is a DEX and uses Ethereum wallet signing instead of API key/secret pairs.

### Credential Fields

| Field                   | Maps To             | Description                                                                                                                            |
| ----------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `options.key`           | Wallet address      | Main wallet address (used as `user` for info queries)                                                                                  |
| `options.secret`        | Private key         | Private key for the wallet (agent or main)                                                                                             |
| `options.applicationId` | Main wallet address | The main wallet address (for wallet-based connections). Used for builder fee checks.                                                   |
| `options.vault`         | Vault address       | Optional vault address (for vault trading). When set, sent as `vaultAddress` in exchange requests and used as `user` for info queries. |

**`applicationId` is load-bearing.** All private info queries (`clearinghouseState`, `frontendOpenOrders`, etc.) key off `vault || applicationId`. Without one of them, `hasPrivateSessionIdentity` is false → `isPublicDataOnly` flips true → balance/orders/positions never load. Both registration paths now require it: the auto flow (`HyperliquidAutoSetup.tsx`) writes `applicationId = mainWallet` from the connected MetaMask address; the manual form (`ApiKeyFormFragment.tsx`) renders a required "Main Wallet Address" input gated on `requiresApplicationId`. Legacy accounts predating this enforcement may ship without it — see "applicationId auto-resolution" below.

### applicationId auto-resolution (legacy accounts)

`start()` calls `resolveApplicationIdFromKey()` before reading `hasPrivateSessionIdentity` if the account has `key` but no `applicationId`/`vault`. POSTs `/info { type: "userRole", user: key }` and patches `this.options.applicationId` (or `vault`) in-place based on the returned role:

| `role`       | Action                                                       |
| ------------ | ------------------------------------------------------------ |
| `user`       | `applicationId = key` (key is itself a master wallet)        |
| `agent`      | `applicationId = data.user` (master that approved the agent) |
| `subAccount` | `applicationId = data.master`                                |
| `vault`      | `vault = key` (key is a vault wallet)                        |
| `missing`    | Surface a user-visible error via `emitAndLogError` (re-add)  |

Wrapped in `withStartupTimeout(USER_ROLE_RESOLVE_TIMEOUT_MS = 6_000)` so a slow `userRole` call can't block startup; on timeout/error, falls back to public-data-only.

**Why the in-place patch is correct.** `ExchangeOptions` is `Readonly<>`, so the resolver casts `this.options as { applicationId?: string; vault?: string; key?: string }` and writes through that. This is intentional — the resolved value has to be visible to every downstream read (`hasPrivateSessionIdentity`, `fetchCorePerpsState`, builder-fee check, WS subscribe). Don't refactor this to "return the resolved id and pass it explicitly" without updating every consumer that reads `options.applicationId` directly.

**The resolved value is NOT persisted back to the saved account record**, so the `userRole` round-trip is paid on every startup for legacy accounts. Persisting upstream is a follow-up that touches the account-config layer outside `safe-cex`.

**`role: 'missing'` surfaces a user-visible toast** with the wallet address embedded, telling the user to re-add the account. This is the only signal the user gets that their saved account is unusable on Hyperliquid (no deposits, no agent approvals, no subaccount linkage on this network) — don't downgrade it back to `this.log(..., LogSeverity.Warning)`. Same `emitAndLogError` treatment for the (rare) edge cases where `userRole` returns `agent`/`subAccount` but omits the master address.

### Account abstraction mode detection (unified / portfolio)

`start()` resolves account abstraction mode for private sessions via:

`POST /info { type: "userAbstraction", user: vault || applicationId }`

Returned modes currently include:

- `unifiedAccount`
- `portfolioMargin`
- `disabled`
- `default`
- `dexAbstraction`

Runtime gating is centralized in `shouldUseSpotCollateralBalance()`:

- `true` only for `unifiedAccount` and `portfolioMargin`
- `false` for `disabled`, `default`, and `dexAbstraction` (standard fallback path)

If mode is unified/portfolio:

- Perp private WS subscribes to `spotState` (mode-gated, not always-on), adding `isPortfolioMargin: true` for portfolio-margin accounts; its payload is nested as `{ user, spotState: { balances, tokenToAvailableAfterMaintenance } }`.
- The current `spotState` subscription and nested response shape were re-verified against both mainnet and testnet on 2026-07-15. Keep dispatch support for the legacy `spotClearinghouseState` channel name during the upstream migration, but never send that old name as a WS subscription; it remains only a REST `/info` request type.
- Perp startup triggers a REST `spotClearinghouseState` refresh as bootstrap safety net.
- REST and WS retain the complete spot state, including each balance's token id and optional `tokenToAvailableAfterMaintenance` pairs.
- Token id `0` (USDC) in `tokenToAvailableAfterMaintenance` is the authoritative post-maintenance aggregate availability. It is used directly for `balance.free` and synthetic `currencies.USDC.free`, rather than subtracting core margin or PnL a second time. Once received from the live stream, that authoritative aggregate snapshot cannot be downgraded by a later field-less or stale in-flight REST response.
- Spot balances/holds supply collateral equity for displayed balance. Unified spot totals already include live perp PnL, so the synthetic wallet total removes PnL once while core `clearinghouseState` supplies the separately displayed `upnl` field.
- `balance.currencies.USDC` is a synthetic aggregate margin slice for order sizing (`total/free/used/upnl`), not the raw spot USDC balance.
- Raw spot asset slices are preserved under `balance.info.spotCollateralCurrencies`; notional holdings are also exposed via `balance.coins`.
- `userNonFundingLedgerUpdates` triggers a cooldown-aware spot collateral refresh (10s cooldown + in-flight collapse guard).

Resilience contract:

- `userAbstraction` resolution is bounded by startup timeout and starts before public market hydration, but public markets/public WS do not wait on it.
- Private WS startup waits for `userAbstraction` so the mode-gated `spotState` subscription is correct.
- Timeout/error/unexpected mode falls back to `standard` behavior.
- This keeps standard-mode users stable while supporting early per-account migrations that may happen before network-wide deprecation dates announced in Telegram.

### Signing Methods

1. **L1 Action Signing (Phantom Agent)** — Used for all trading operations (`/exchange`):
   - Msgpack-encode the action, append nonce (8 bytes big-endian) and vault flag (1 byte + 20 bytes if vault)
   - Keccak256 hash the combined data
   - Construct phantom agent: `{ source: "a" (mainnet) or "b" (testnet), connectionId: hash }`
   - EIP-712 sign with domain `{ name: "Exchange", version: "1", chainId: 1337, verifyingContract: 0x0 }`
   - Result: `{ r, s, v }` signature
   - Reference: `hyperliquid.signing.ts:signL1Action`

2. **User-Signed Action (wallet-key)** — Used by exchange class for agent approval, builder fee approval, USD transfer, withdraw:
   - EIP-712 sign with domain `{ name: "HyperliquidSignTransaction", version: "1", chainId: 421614, verifyingContract: 0x0 }`
   - `chainId` is ALWAYS `421614` and `signatureChainId` is ALWAYS `"0x66eee"` regardless of mainnet/testnet
   - Only `hyperliquidChain` differs: `"Mainnet"` or `"Testnet"`
   - Reference: `hyperliquid.signing.ts:signUserSignedAction`

3. **User-Signed Action (browser-wallet)** — Used for browser-based agent/builder-fee approval via MetaMask:
   - EIP-712 sign with domain `chainId: 42161` (Arbitrum One) for mainnet, `chainId: 421614` (Arbitrum Sepolia) for testnet
   - Submission payload uses `signatureChainId: "0xa4b1"` for mainnet, `"0x66eee"` for testnet
   - Reference: `hyperliquid.builderFee.ts:signBuilderFeeApproval`, `hyperliquid.agent.ts:signAgentApproval`

### Agent Wallets

Agent wallets are limited-permission wallets that can trade on behalf of the main wallet but cannot withdraw or transfer funds. Created via `hyperliquid.agent.ts`:

1. Generate random `ethers.Wallet` locally
2. Switch browser wallet to correct Arbitrum network (One for mainnet, Sepolia for testnet)
3. Sign `ApproveAgent` message with main wallet via `signTypedData`
4. Submit to Hyperliquid `/exchange` with action type `approveAgent`
5. Return agent address + private key

Agent name format: `"Tealstreet XXXX"` (15 chars, max 16 chars, random alphanumeric suffix). Reference: `hyperliquid.agent.ts:generateAgentWalletName`.

### Builder Fees

Builder fees (builder codes) allow Tealstreet to receive a portion of trading fees. Fee = 10 tenths of bps = 1bp = 0.01%. Builder info is attached to order actions as `{ b: builderAddress, f: 10 }`.

**Flow**: On startup, check if user has approved builder fee via `maxBuilderFee` info type (`fetchBuilderFeeApproval`). If approved, include `builder` field in all order actions. Approval requires EIP-712 signing from the main wallet via `approveBuilderFeeWithBrowserWallet()`.

**Builder addresses** (from `hyperliquid.constants.ts`):

- Mainnet: `0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d`
- Testnet: `0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba`

**Approval status** is cached in `_builderFeeApproved` (null = unknown, true = approved, false = not approved). If unknown when placing an order, `getBuilderInfo()` re-fetches the status. On successful browser-wallet approval, a fire-and-forget POST to `/api/logger/builder-fee` logs the event.

`HyperliquidExchange.getBuilderInfo()` is the only order-path builder-info gate. Keep it tied to the live `maxBuilderFee` approval state; do not reintroduce environment-based signing helpers.

### EIP-712 Domain Details

| Domain                         | Name                         | Version | ChainId                     | Usage                                                                |
| ------------------------------ | ---------------------------- | ------- | --------------------------- | -------------------------------------------------------------------- |
| Phantom Agent                  | `Exchange`                   | `1`     | `1337`                      | Trading operations (place/cancel/modify orders, leverage)            |
| User-Signed (wallet-key)       | `HyperliquidSignTransaction` | `1`     | `421614` (always)           | Agent approval, builder fee, USD send, withdraw (via exchange class) |
| User-Signed (browser, mainnet) | `HyperliquidSignTransaction` | `1`     | `42161` (Arbitrum One)      | Agent/builder-fee approval via MetaMask (mainnet)                    |
| User-Signed (browser, testnet) | `HyperliquidSignTransaction` | `1`     | `421614` (Arbitrum Sepolia) | Agent/builder-fee approval via MetaMask (testnet)                    |

## Startup Sequence

The `start()` method orchestrates initialization:

```
Phase 0: Identity resolution
└── resolveApplicationIdFromKey()     — legacy accounts missing applicationId/vault

Phase 1: Parallel initial data fetch
├── resolveUserAbstractionMode() → POST /info { type: "userAbstraction" } — private sessions, non-blocking for public hydration
├── fetchMarketsAndTickers()
│   ├── POST /info { type: "metaAndAssetCtxs" }         — core perp markets + tickers
│   ├── fetchFee() → POST /info { type: "userFees" }    — user fee rates
│   └── fetchHip3Dexs() → POST /info { type: "perpDexs" }  — HIP-3 DEX list
│       └── For each DEX (strictly sequential, 8s total budget): POST /info { type: "metaAndAssetCtxs", dex: name }
├── fetchBuilderFeeApproval() → POST /info { type: "maxBuilderFee" }
└── fetchCorePerpsState() → POST /info { type: "clearinghouseState" }; authoritative core-position + balance bootstrap

Phase 2: Build data structures (synchronous)
├── buildMarketsLowerCaseMap()   — O(1) case-insensitive lookup map
├── generateFakePositionsFromMarkets()  — zero-size positions for UI
└── store.update({ markets, tickers, positions, orders:[], balance, loaded })

Phase 3: Start WebSocket connections (non-blocking)
├── publicWebsocket.connectAndSubscribe()   — allMids, l2Book, candle
└── await userAbstractionModePromise, then privateWebsocket.connectAndSubscribe()  — system topics
    └── if unified/portfolio: requestUnifiedSpotCollateralRefresh(true)

Phase 4: Async initial data enrichment (fire-and-forget)
├── fetchOrders()              — core perps + all HIP-3 dex frontendOpenOrders
└── fetchAllHip3Positions()    — clearinghouseState per HIP-3 dex (sequential)

Phase 5: Start tick loop
└── tick() → loop(tick, tickInterval)
```

**Key dependency ordering**:

- Markets must be loaded before WebSocket subscriptions (subscriptions reference market IDs)
- `_lastOrderRefreshTime` and `_lastPositionRefreshTime` are set to `Date.now()` before async fetches to prevent the tick loop from triggering duplicate fetches immediately
- `isDisposed` is checked after every `await` to abort gracefully if the exchange is disposed mid-startup

## REST API Endpoints

### Base URLs

| Environment    | REST                                  | WebSocket                              |
| -------------- | ------------------------------------- | -------------------------------------- |
| **Production** | `https://api.hyperliquid.xyz`         | `wss://api-ui.hyperliquid.xyz/ws`      |
| **Testnet**    | `https://api.hyperliquid-testnet.xyz` | `wss://api.hyperliquid-testnet.xyz/ws` |

### REST Endpoints

Hyperliquid uses only two REST endpoints. All operations are differentiated by the `type` field in the POST body.

| Endpoint         | Purpose                                                                               | Rate Limit                     |
| ---------------- | ------------------------------------------------------------------------------------- | ------------------------------ |
| `POST /info`     | Read operations (markets, positions, orders, balances, candles, fills)                | 3 RPS (via `axios-rate-limit`) |
| `POST /exchange` | Write operations (place/cancel/modify orders, update leverage, approve agent/builder) | 3 RPS (via `axios-rate-limit`) |

### Info Types (POST /info, `type` field)

| Type                            | Purpose                                                          | When Called                             |
| ------------------------------- | ---------------------------------------------------------------- | --------------------------------------- |
| `metaAndAssetCtxs`              | Market metadata + ticker data (core perps)                       | Startup                                 |
| `metaAndAssetCtxs` with `dex`   | Market metadata per HIP-3 DEX                                    | Startup (per DEX)                       |
| `perpDexs`                      | List of HIP-3 DEXes                                              | Startup                                 |
| `userFees`                      | User fee rates (taker/maker)                                     | Startup                                 |
| `maxBuilderFee`                 | Check builder fee approval status                                | Startup                                 |
| `userRole`                      | Resolve missing applicationId from key                           | Startup (legacy accounts only)          |
| `userAbstraction`               | Resolve abstraction mode (`unified`/`portfolio`/`standard`-like) | Startup (private accounts)              |
| `clearinghouseState`            | Account positions + margin (core perps)                          | Startup (initial balance)               |
| `clearinghouseState` with `dex` | HIP-3 positions per DEX                                          | Startup + every 120s safety net         |
| `frontendOpenOrders`            | Open orders with type/trigger info (core perps)                  | Startup + on-demand refresh             |
| `frontendOpenOrders` with `dex` | HIP-3 open orders per DEX                                        | Startup + on-demand + retry chain       |
| `candleSnapshot`                | OHLCV candle data                                                | On chart load                           |
| `userFills`                     | User fill history                                                | Executions tab                          |
| `userFillsByTime`               | User fills filtered by time                                      | Executions tab                          |
| `spotMetaAndAssetCtxs`          | Spot market metadata                                             | Spot startup                            |
| `spotClearinghouseState`        | Spot balances / holds (collateral source for unified/portfolio)  | Spot exchange + perps unified/portfolio |

### Chaser fill reconciliation

`fetchFillsForOrderIds()` is used after a chaser cancel attempt, before a
replacement can be placed. It must throw on a failed or malformed `userFills`
response; returning an empty map is reserved for a valid response with no
matching fills. The chaser manager treats a thrown reconciliation as a
fail-closed stop because cancellation may already have succeeded and a missed
fill would make the replacement overfill the target.

### Exchange Types (POST /exchange, `action.type` field)

| Type                   | Purpose                                                            |
| ---------------------- | ------------------------------------------------------------------ |
| `order`                | Place orders (with `grouping`: `na`, `normalTpsl`, `positionTpsl`) |
| `cancel`               | Cancel orders by asset index + order ID                            |
| `cancelByCloid`        | Cancel by client order ID                                          |
| `modify`               | Modify existing order                                              |
| `batchModify`          | Batch modify orders                                                |
| `updateLeverage`       | Change leverage/margin type for a market                           |
| `updateIsolatedMargin` | Adjust isolated margin                                             |
| `usdSend`              | Transfer USDC                                                      |
| `withdraw3`            | Withdraw funds                                                     |
| `vaultTransfer`        | Transfer to/from vault                                             |
| `approveAgent`         | Approve agent wallet                                               |
| `approveBuilderFee`    | Approve builder fee                                                |

## WebSocket Implementation

### Private WebSocket Design

The private WebSocket connects to the official Hyperliquid API.

```
CORE PERPS:
  Official WS ---+--> Store
  REST (startup) -+

UNIFIED / PORTFOLIO (perp route only):
  spotState WS (mode-gated) ------------------+
  spotClearinghouseState REST (startup/ledger)+--> balance merge

HIP-3 (per dex):
  Official WS (real-time) ---+--> Store (primary)
  REST (120s safety net) ----+    (fallback only)
```

**Timestamp deduplication**: Each channel tracks the last seen timestamp. A message is skipped if its timestamp is older than the most recent seen. HIP-3 dex channels are keyed separately (e.g., `clearinghouseState:vntls`) so updates from one dex do not suppress another dex's updates.

**Timestamp extraction** (`getMessageTimestamp`): The code checks `data.serverTime`, then `data.time`, then `data.clearinghouseState.time` — Hyperliquid payloads are inconsistent in which field contains the timestamp.

**Exceptions to timestamp check**: `userFills`, `userFundings`, `userNonFundingLedgerUpdates`, `notification`, `orderUpdates`, `userEvents` skip the stale check because they are event-driven (not snapshots) and every message matters.

**activeAssetData deduplication**: Since `activeAssetData` has no `serverTime`, a hash of `markPx` + `leverage.value` is used to detect duplicates. Key format: `activeAssetData:{coin}`.

### Public WebSocket Channels

| Channel   | Subscription                                      | Data                                              |
| --------- | ------------------------------------------------- | ------------------------------------------------- |
| `allMids` | `{ type: "allMids" }`                             | Mid prices for all coins (ticker updates)         |
| `fastAssetCtxs` | `{ type: "fastAssetCtxs" }`                   | Raw-DEFLATE base64 mark/mid snapshot and diffs    |
| `l2Book`  | `{ type: "l2Book", coin: "BTC" }`                 | Standard 20-level orderbook depth, on demand      |
| `bbo`     | `{ type: "bbo", coin: "BTC" }`                    | Fresh best bid/ask overlay for each active ladder |
| `candle`  | `{ type: "candle", coin: "BTC", interval: "1m" }` | OHLCV candles (per coin+interval, on-demand)      |

**Ping**: Every 15 seconds via `{ method: "ping" }` (`HyperliquidPublicWebSocket.PING_INTERVAL_MS`). Ping is not sent immediately on connect — first scheduled ping fires after 15s delay to avoid issues with premature pings before subscription responses.

**Subscription deduplication**: `activeSubscriptions` Set tracks sent subscriptions by JSON key to prevent duplicate l2Book/candle subscriptions. Cleared on reconnect.

**Reconnection**: Handled by `BaseWebSocket` parent class. On reconnect, `activeSubscriptions` is cleared and `subscribePublicTopics()` re-subscribes `allMids` plus any user topics (candles, orderbooks).

**Public WS hot-path conventions** (see `../CLAUDE.md` → "Public WebSocket Hot-Path Patterns"):

- `onMessage` parses JSON once up front, then dispatches via `switch (json?.channel)`. No substring scanning per registered handler — already O(1) by design.
- `case 'l2Book'`: direct `messageHandlers[\`l2Book.\${coin}\`]`lookup. Don't reintroduce the`Object.keys.filter(k === ...)` walk that used to allocate the keys array and run a linear scan to find the single match.
- `case 'candle'`: sample the first entry's `s` and `i`, look up `messageHandlers[\`candle.\${s}.\${i}\`]` directly. The prior all-handlers iteration fired every candle parser on every frame and relied on each parser's internal symbol/interval filter — wasteful when N-1 of N parsers were guaranteed no-ops.
- `handleAllMids`: `for (const id in data)` over the mids object instead of `Object.entries`; avoids the per-frame ~600-tuple array allocation. Reads tickers via the `tickersById` hashmap.
- `fastAssetCtxs` supplements rather than replaces `allMids`. Its first frame is a full coin-keyed snapshot and later frames are partial coin/field diffs. Decode synchronously with raw DEFLATE after base64; a 2026-07-15 production probe measured 2.97 ms for the 11.7 KB compressed initial snapshot and 0.19–0.33 ms for 0.6–1.13 KB steady-state diffs at about one-second cadence. Update only finite positive `markPx` / `midPx` values and never clear a field omitted by a diff. `midPx` updates `last` and `midPrice`; `markPx` updates `mark`, which prices market and TP/SL slippage caps. A malformed frame must leave ticker state intact so `allMids` continues as fallback.
- Perp orderbook intentionally pairs exact/full-precision standard `l2Book` with `bbo`: the standard tier preserves the 20-level ladder the UI needs, while BBO refreshes the top level without shrinking the ladder to the `fast:true` tier's five levels. When BBO moves through stale depth, discard levels that claim to be better than the new best. Do not infer a grouping bucket from adjacent price gaps; HIP-3 books can be thin or irregular enough that adjacent gaps round to zero against market precision and produce `Infinity`/`-Infinity` grouping errors.
- `l2Book` parser: indexed `for`-loop with direct `lvl.px` / `lvl.sz` access, never `forEach({ px, sz })`. **No trailing `sortOrderBook`** — Hyperliquid's `levels[0]` (bids) is DESC and `levels[1]` (asks) is ASC per the WS contract, and we push them in order, so the resulting arrays are already sorted.
- Spot `l2Book` parser meta-book aggregation: build via direct `metaBook.bids.push(b)` inside indexed for-loops. **Don't reintroduce** the `groupingLevels.reduce((acc, g) => ({ bids: [...acc.bids, ...filtered] }))` shape — that spread the growing accumulator per iteration (quadratic) and allocated a fresh filter array per side per grouping.
- `handleAllMids` / `handleHip3AllMids`: reuse a single `_midsUpdateBuf` instead of allocating `{ last, midPrice }` per ticker. The store's `updateTicker` iterates the buffer fields onto the live ticker without retaining the reference, so reuse is safe. Also skip the `updatedSymbols` `Set<string>` allocation entirely when `positions.length === 0` (consumer bails fast in that case anyway).
- `onMessage`: **deliberately** calls `recordMessageHandlerStats('pong')` unconditionally on every inbound frame, in addition to the channel-specific stats record. This is a load-bearing hack against Hyperliquid's flaky pong cadence: the staleness watchdog (in `apps/web/src/utils/misc/getGroupedMessageTopicStats.ts`) gates on `now - pong.lastMessageTimestamp`, and without this bump the public WS gets falsely flagged as dead and reconnects even when tickers/orderbooks are flowing normally. **Do not "fix" this** — same hack lives in `hyperliquidspot.ws-private.ts → onMessage` for the same reason. If the perp private WS ever starts spuriously reconnecting, that's the next place to apply the hack (it currently only records pong inside the dedicated pong branch).

### Private WS hot-path conventions

- `processMessage 'activeAssetData'`: indexed scan with `continue` over `store.positions`, NOT `positions.filter(p => p.symbol === market.symbol)`. And drop the `{ ...position, ...overrides }` spread in `updatePosition` — the store writes only the fields in `changes` onto the live position; the spread copied every position field into a fresh object just to write it back unchanged. Pass only the 3 fields you actually want to update (`positionMode`, `marginType`, `leverage`).
- `handleOrderUpdates` order processing: pre-build `existingOrdersById: Record<string, Order>` once at the top of the call and use it for HIP-3 type-preservation + tif fallback lookups. The prior shape ran `store.orders.find(o => o.id === orderId)` per update — potentially twice — for ~50 orders × ~50 stored = ~5K linear comparisons per frame.
- `new Set(arr.map(o => o.id))` should be a direct loop into a pre-allocated Set — drops the intermediate `.map(o => o.id)` array that's only consumed by Set's iterator.
- `handleClearinghouseState` and spot equivalents: collapse `arr.filter(...).map(...).filter(o => o !== null)` chains into a single indexed for-loop with `continue` for the skip cases and `push` for the build step.
- `handleHip3ClearinghouseState` builds via `Map<id, Position>` with fakes first, preserved core/other-DEX positions second, and incoming positions last. A dex-scoped empty snapshot is authoritative and must remove closed positions for that DEX.
- `_getSymbolConfigurationFromAssetData`: defaults branch returns the shared frozen `DEFAULT_SYMBOL_CONFIG` singleton instead of allocating a fresh `{ marginType, positionMode, leverage }` per call. Callers always spread the result into a new position object — never mutate — so sharing is safe. `Object.freeze` makes the contract explicit. Same pattern in the spot exchange (`SPOT_SYMBOL_CONFIG`).
- `handleFills`: indexed for-loop over `forEach`. Snapshot loads can carry hundreds of fills.
- `handleClearinghouseState` upnl recompute: indexed for-loop over `positions.reduce((acc, p) => acc + p.upnl, 0)`. Drops the closure allocation per balance update.
- `SKIP_TIMESTAMP_CHECK_CHANNELS`: hoisted to a class-level `Set<string>` instead of allocating a fresh 6-element array + running `.includes()` (linear scan) per WS frame.

### Private WebSocket Channels

Subscribed for the user (or vault address):

| Channel                       | Core Perps                                | HIP-3 (per dex)                 | Data                                                                                                              |
| ----------------------------- | ----------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `allMids`                     | No                                        | Yes (with `dex` param)          | HIP-3 published mid-price updates                                                                                 |
| `clearinghouseState`          | Yes                                       | Yes (with `dex` param)          | Positions + margin (real-time, primary for positions)                                                             |
| `spotState`                   | Conditional (unified/portfolio mode only) | No                              | Nested spot state; token 0 `tokenToAvailableAfterMaintenance` is authoritative unified free margin               |
| `activeAssetData`             | Yes (per coin)                            | No (subscription limit concern) | Leverage, margin info per market                                                                                  |
| `orderUpdates`                | Yes                                       | Yes (with `dex` param)          | Order status: open, filled, canceled, triggered, rejected, marginCanceled, reduceOnlyCanceled, reduceOnlyRejected |
| `userFills`                   | Yes                                       | Yes (with `dex` param)          | Fill notifications with deduplication                                                                             |
| `userEvents`                  | Yes                                       | Yes (with `dex` param)          | Order lifecycle (backup to orderUpdates)                                                                          |
| `userFundings`                | Yes                                       | -                               | Funding payments                                                                                                  |
| `userNonFundingLedgerUpdates` | Yes                                       | -                               | Deposits, withdrawals, transfers, liquidations                                                                    |
| `notification`                | Yes                                       | -                               | User notifications                                                                                                |

**HIP-3 subscription channels per dex**: `allMids`, `clearinghouseState`, `userFills`, `orderUpdates`, `userEvents` (5 channels per dex). `activeAssetData` is NOT subscribed for HIP-3 markets to avoid exceeding the 1000 channel limit.

**Deliberate aggregate-feed boundaries**: Mainnet and testnet probes on 2026-07-15 returned only `userState` and `perpDexStates` from `webData3`, so it does not replace position, order, or ticker reconciliation. The same probes accepted `allDexsAssetCtxs` and `allDexsClearinghouseState`, but live frames use arrays of `[dex, value]` tuples while the published TypeScript contract describes records. Asset contexts omit metadata needed to map array ordinals safely, and the clearinghouse feed would replace already-correct dex-scoped reconciliation with a much larger aggregate snapshot (240 DEX entries on testnet). Keep the existing per-DEX paths until the wire contract stabilizes or a concrete correctness issue justifies migration. `webData2` is rejected upstream and its dead handlers have been removed.

**Ping**: Every 15 seconds.

## Order Types and Mappings

### Supported Order Types

| Internal Type      | Wire Format                                                 | Grouping                                                  |
| ------------------ | ----------------------------------------------------------- | --------------------------------------------------------- |
| `Limit`            | `{ limit: { tif: "Gtc" } }`                                 | `na`                                                      |
| `Limit` (PostOnly) | `{ limit: { tif: "Alo" } }`                                 | `na`                                                      |
| `Market`           | `{ limit: { tif: "Gtc" } }` with 5% slippage price          | `na`                                                      |
| `TakeProfit`       | `{ trigger: { isMarket: true, tpsl: "tp", triggerPx: X } }` | `positionTpsl` (full close, sz=0) or `na` (partial, sz>0) |
| `StopLoss`         | `{ trigger: { isMarket: true, tpsl: "sl", triggerPx: X } }` | `positionTpsl` (full close, sz=0) or `na` (partial, sz>0) |
| Limit + TP + SL    | Combined wires                                              | `normalTpsl`                                              |
| `TrailingStopLoss` | **Not supported** (throws before submission)                | -                                                         |

### TIF Mappings

| Internal            | Hyperliquid Wire |
| ------------------- | ---------------- |
| `GoodTillCancel`    | `Gtc`            |
| `PostOnly`          | `Alo`            |
| `ImmediateOrCancel` | `Ioc`            |

### Order Grouping

- `na`: Standalone order (limit, market, partial TP/SL with `sz > 0`)
- `normalTpsl`: Limit order with attached TP and/or SL
- `positionTpsl`: Full position TP/SL (size = 0, closes entire position)

### Market Order Implementation

Market orders are sent as `Gtc` limit orders at 5% above/below mark price (or last price if mark unavailable). Mark price is preferred over last price to avoid "price too far from oracle" errors, especially for HIP-3 markets with thin liquidity. Reference: `HyperliquidExchange.placeOrder`.

### Order Wire Format

```typescript
interface OrderWire {
  a: number; // asset index (hyperliquidId)
  b: boolean; // is_buy
  p: string; // price (floatToWire format)
  s: string; // size (floatToWire format)
  r: boolean; // reduce_only
  t: OrderType; // { limit: { tif } } or { trigger: { isMarket, triggerPx, tpsl } }
  c?: string; // optional client order ID (cloid)
}
```

## Order Lifecycle

### Placement Flow

1. Resolve market via `store.marketsBySymbol[symbol]` (O(1) lookup)
2. Build `OrderWire` array (main order + optional TP/SL wires)
3. Get builder info if approved (`getBuilderInfo()`)
4. Select action builder based on grouping: `orderWiresToOrderAction` (na), `orderWiresToNormalTPSLAction` (normalTpsl), `orderWiresToPositionalTPSLAction` (positionTpsl)
5. Generate nonce, sign L1 action
6. **HIP-3 pre-placement cache**: Before the REST call, cache `{coin, side, type, price, amount}` so WS handler can identify the order before REST returns
7. POST to `/exchange`
8. Validate response via `validateResponseAndExtractOrderIds()` — extracts `oid` from `resting` or `filled` status
9. **HIP-3 post-placement**: Add to pending cache (`addPendingHip3Order`), add optimistic order to store with `OrderStatus.Creating`, start retry chain

### Update Flow (Cancel + Replace)

Hyperliquid does not support in-place order modification for TP/SL. The `HyperliquidExchange.updateOrder` method:

1. Build `newOrder` from existing order + updates
2. **TP/SL type re-evaluation**: If the new price crosses the current market price, the order type changes (e.g., TP becomes SL). This prevents accidental position closure when dragging orders past current price.
3. **HIP-3 optimistic update**: Add an order with `OrderStatus.Updating` at the new price for immediate UI feedback
4. Cancel original order via `cancelOrders()`
5. Mark original as "dead" in `_deadOrders` Set (prevents WS from re-adding it)
6. Place new order with updated parameters via `placeOrder()`

### Cancellation Flow

1. Build cancel action: `{ type: "cancel", cancels: [{ a: assetIndex, o: orderId }] }`
2. Sign and POST to `/exchange`
3. **Cancel suppression**: Add order IDs to `_cancelledOrders` map with timestamp
4. **Pre-placement cache cleanup**: Clear all entries for the cancelled order's `coin|side` combinations to prevent stale entries from poisoning future matches
5. Remove from store immediately

### Retry Chain (HIP-3 Order Refresh)

After placing HIP-3 orders, a coalesced retry chain fetches `frontendOpenOrders` with progressive delays: **300ms, 700ms, 1500ms, 3000ms, 5000ms** (max 5 attempts). Multiple rapid placements merge order IDs into the same chain rather than spawning parallel chains (`_pendingRetryOrderIds` Set, `_retryChainActive` flag).

Reference: `HyperliquidExchange.refreshHip3OrdersWithRetry`.

### Fetch Orders Deduplication

Overlapping `fetchOrders()` calls are collapsed into a single in-flight request via `_fetchOrdersInFlight` promise. The second caller receives the same promise result as the first.

Hyperliquid is the narrow legacy exception to the package's pure-fetch
convention: `fetchOrdersInternal()` performs surgical, DEX-aware store
reconciliation while producing its result. Fire-and-forget startup/debounce/tick
heals therefore call the explicitly named `refreshOrders()` completion wrapper;
directly discarding `fetchOrders()` remains forbidden by the package lint rule.

### Order Refresh Debounce

`scheduleOrderRefresh()` debounces order refreshes with a 500ms window and a minimum 1s interval (`HIP3_ORDER_REFRESH_INTERVAL_MS`). Triggered by position closures (auto-cancel TP/SL) and unrecognized HIP-3 WS orders.

## Symbol Format and Market IDs

### Core Perps

| Field                       | Format                  | Example    |
| --------------------------- | ----------------------- | ---------- |
| `market.id`                 | Coin name (lowercase)   | `BTC`      |
| `market.symbol`             | `{BASE}-PERP`           | `BTC-PERP` |
| `market.info.hyperliquidId` | Integer index (0-based) | `0`        |

### HIP-3 Markets (Builder-Deployed Perps)

| Field                       | Format                                                          | Example               |
| --------------------------- | --------------------------------------------------------------- | --------------------- |
| `market.id`                 | `{dex}:{asset}` (API format)                                    | `vntls:vKRAKEN`       |
| `market.symbol`             | `{DEX}\|{ASSET}-PERP` (pipe separator)                          | `VNTLS\|VKRAKEN-PERP` |
| `market.info.hyperliquidId` | Offset index: `110000 + (dexPosition - 1) * 10000 + localIndex` | `110002`              |
| `market.info.hip3Dex`       | DEX name string                                                 | `vntls`               |

The pipe `|` separator is used instead of colon `:` to avoid TradingView conflicts.

**Conversion**: `symbolToCoin("VNTLS|VKRAKEN-PERP")` returns `"vntls:VKRAKEN"` (lowercase dex, uppercase asset; the asset is uppercased during market creation).

### Case-Insensitive Lookup

A `marketsLowerCaseMap` (`Map<string, Market>`) provides O(1) case-insensitive lookups for HIP-3 markets, since the API may return coin names with inconsistent casing. Built during startup and used as fallback in all market resolution paths (order updates, fills, positions).

## Data Precision & Conversion

### Price Precision Algorithm

Uses a CCXT-compatible algorithm (`calculatePricePrecision` in `hyperliquid.utils.ts`):

1. Price = 0: `pricePrecision = min(maxDecimals - szDecimals, 5)`
2. Price between 0 and 1: Count leading zeros after decimal, add 5 significant digits, cap at `maxDecimals - szDecimals`
3. Price >= 1: `significantDigits = max(5, integerDigits)`, then `min(maxDecimals - szDecimals, significantDigits - integerDigits)`

`maxDecimals` is `6` for Hyperliquid perps (core + HIP-3) and `8` for Hyperliquid spot.

### `priceToPrecision`

Applies two rounds of precision:

1. First round: `decimalToPrecision(price, ROUND, significantDigits, SIGNIFICANT_DIGITS)` where `significantDigits = max(5, integerPartLength)`
2. Second round: `decimalToPrecision(result, ROUND, 6 - amountPrecisionDigits, DECIMAL_PLACES)` caps decimal places

### `floatToWire`

Converts float to string with up to 8 decimal places:

- `toFixed(8)` then strip trailing zeros
- Throws if rounding exceeds `1e-9` tolerance (prevents silent precision loss)
- `-0` is normalized to `"0"`

### `floatToUsdInt` / `floatToIntForHashing`

- `floatToUsdInt(x)`: Multiplies by `10^6` (USDC has 6 decimals), rounds, throws if error > `1e-3`
- `floatToIntForHashing(x)`: Multiplies by `10^8`, rounds, throws if error > `1e-3`

### Size Precision

Order sizes are adjusted to `market.precision.amount` using `adjust()` from `safe-math` before building the wire format.

### Nonce Generation

Nonces are timestamp-based (milliseconds) and guaranteed unique via `generateNonce()`:

```typescript
this.lastNonce = Math.max(now, this.lastNonce + 1);
```

This ensures monotonically increasing nonces even if multiple operations happen within the same millisecond.

## Rate Limiting

| Client                          | Limit              | Notes                                                              |
| ------------------------------- | ------------------ | ------------------------------------------------------------------ |
| `xhr` (exchange + info)         | 3 RPS              | via `axios-rate-limit`                                             |
| `infoXhr`                       | Unlimited          | Separate instance for info endpoint without rate limit             |
| `unlimitedXHR`                  | Unlimited          | For operations that need to bypass rate limit                      |
| Token bucket (`rateLimiter.ts`) | 1200 tokens/minute | Application-level limiter (available but not wired into main flow) |

**Retry on network errors**: Both `xhr` and `infoXhr` use `axios-retry` with `retries: 3` and `retryCondition: isNetworkError`. Only network errors trigger retries, not HTTP error responses.

**HIP-3 position fetch**: Sequential per-DEX to avoid rate limiting (`fetchAllHip3Positions` uses a `for` loop, not `Promise.all`).

**Order fetch per-DEX**: Sequential (`fetchOrdersInternal` iterates HIP-3 dexes in a `for` loop).

**HIP-3 market metadata**: Sequential per DEX with an 8-second total startup budget. Official `/info` weights are 20 for `metaAndAssetCtxs`, 60 for `userRole`, and 1200 total per IP per minute. Live discovery on 2026-07-15 returned one core plus nine HIP-3 entries on mainnet; testnet's much larger catalog remains restricted to the four-DEX whitelist. Do not turn this loop back into `Promise.all`: multiple browser accounts can share one proxy egress budget.

## State Reconciliation

### Position Reconciliation

**Core perps**: Primary source is `clearinghouseState` WS channel. Hyperliquid's live subscription frame is wrapped as `{ user, dex: '', clearinghouseState }`, so the private WS must unwrap `data.clearinghouseState ?? data` before giving it to `applyCoreClearinghouseState`; passing the outer wrapper silently drops positions and balance updates. Core WS frames merge incrementally because they can be scoped; the startup and reconnect REST `clearinghouseState` snapshot is authoritative and replaces core rows in full while preserving HIP-3 rows and fake position defaults, so closures during a disconnect are removed. A source-time guard prevents an older REST response from overwriting newer WS state.

**HIP-3 positions**: Primary source is `clearinghouseState` WS with `dex` parameter. When updating:

- Positions from other DEXes and core perps are always preserved
- Empty `assetPositions` correctly indicates the position is closed (uses `data.dex` flag, not coin content detection, because an empty array has no coins to detect)
- REST `clearinghouseState` per-dex runs every 120s (`HIP3_POSITION_REFRESH_INTERVAL_MS`) as safety net

**Leverage preservation**: `activeAssetData` can lag behind `clearinghouseState` with stale leverage values. For **open** positions, leverage/marginType from position streams are preserved. For **empty/fake** positions, `activeAssetData` values are applied so pre-position leverage defaults stay current.

### Balance Reconciliation

`applyCorePerpsBalanceSnapshot` is now the single entrypoint for core-perp balance snapshots from REST/WS.

- Core `clearinghouseState` always updates latest perps `accountValue`, `totalMarginUsed`, and `upnl`.
- **Standard-like modes** (`disabled`, `default`, `dexAbstraction`, and timeout/error fallback) keep legacy math:
  - `total = accountValue - upnl`
  - `used = totalMarginUsed`
  - `free = accountValue - used`
- **Unified/portfolio modes** merge the complete spot state from REST `spotClearinghouseState` and WS `spotState`:
  - spot collateral totals already include live perp PnL; when authoritative availability is present, `total = spotCollateralNotionalTotal - upnl` preserves the `Balance.total` wallet-total convention
  - when token 0's `tokenToAvailableAfterMaintenance` is present, `free` is that authoritative post-maintenance amount and `used = spotCollateralNotionalTotal - free`
  - authoritative `total`/`used`/`free` stay snapshot-consistent until the next authoritative spot state; ticker-derived PnL may update the separate `upnl` field between spot frames but must not recompute the other three from stale spot equity
  - when the field is absent (older responses), `used = corePerpsMarginUsed + spotHoldNotional` and `free = total + upnl - used`
  - `currencies.USDC` is a synthetic aggregate margin slice used by `getBalanceSlice()` / order sizing
  - raw spot per-coin collateral slices are exposed in `balance.info.spotCollateralCurrencies`
  - notional spot holdings are exposed in `balance.coins`
  - an empty `spotClearinghouseState.balances` snapshot is authoritative zero spot collateral, not a signal to fall back to standard balance math

Spot collateral freshness:

- Source of truth: mode-gated private WS `spotState`; REST `spotClearinghouseState` remains the bootstrap and ledger-refresh fallback.
- Fallbacks: startup forced REST refresh + cooldown-aware refresh on `userNonFundingLedgerUpdates`.
- Throttling: 10s cooldown and in-flight promise collapse to avoid REST spam.

### Order Reconciliation

**REST as source of truth** (`fetchOrdersInternal`):

1. Fetch core perp orders + per-DEX HIP-3 orders
2. For each fetched order, reconcile with cancel suppression:
   - If cancel was recent (< 30s trust window): suppress the order
   - If cancel was old (> 30s) but REST still shows it open: un-suppress (cancel likely failed)
3. Surgical update: only add/update changed orders to avoid unnecessary re-renders
4. Remove orders not in REST response, but preserve:
   - Optimistic orders (Creating/Updating status)
   - HIP-3 orders from DEXes whose REST fetch failed

**WebSocket order updates**:

- `orderUpdates` is the only live order feed for core and HIP-3.
- HIP-3 updates omit order type fields, so pending/pre-placement caches preserve known metadata and unknown orders trigger the bounded REST retry chain.
- Order removal comes from terminal `orderUpdates` states and authoritative `fetchOrders()` reconciliation.

**Cancellation Suppression** (`_cancelledOrders` map):

- On cancel: add orderId with timestamp
- Trust window: 30s (`CANCEL_SUPPRESSION_TRUST_WINDOW_MS`) — suppress stale WS messages
- Auto-expire: 2 minutes (`CANCEL_SUPPRESSION_MAX_AGE_MS`)
- REST override: After trust window, if REST still shows order as open, un-suppress

### Fill Deduplication

Uses the singleton `HyperliquidCrossCache`:

- In-memory `Set<string>` of seen fill IDs (max 500, FIFO eviction)
- `BroadcastChannel` syncs across web workers and iframes
- Does NOT use `sessionStorage` for fills (would persist across page refresh, skipping fills)
- `sessionStorage` IS used for pre-placement and pending order caches
- Fill snapshots (`isSnapshot: true`) are processed for realized PnL tracking but do not emit fill events

### Realized PnL Tracking

The private WS tracks per-position realized PnL in `handleFills`:

- On "Open" fill with `startPosition === 0`: Reset accumulator to 0
- On "Close" fill: Increment by `closedPnl - fee` (net realized PnL)
- Applied to current position via `store.updatePosition`
- Fill ID tracking bounded to 20,000 entries (`MAX_TRACKED_REALIZED_PNL_FILL_IDS`) with FIFO eviction

## Exchange Metadata

### HIP-3 Order Type Inference

HIP-3 `orderUpdates` WS channel omits `orderType`, `triggerPx`, `tpsl`, and sometimes `tif`. This requires a multi-layer inference system:

```
Order type resolution chain (in priority order):
1. Pending cache    → orderId → type/price  (set after REST returns order ID)
2. API fields       → orderType, triggerPx   (available for core perps, not HIP-3)
3. Store preserved  → existing order in store (from previous REST or optimistic add)
4. Pre-placement    → coin|side → type/price (set before REST call, matched by price/amount/FIFO)
5. Placeholder      → Limit at limitPx       (fallback, triggers debounced REST refresh)
```

### Pre-Placement Cache

Before placing a HIP-3 order, the type/price/amount are cached keyed by `coin|side`. When WS delivers the order (before REST returns), it matches using:

1. Price match: WS `limitPx` === cached price
2. Amount match: WS `origSz` === cached amount (unique match required)
3. FIFO: oldest entry

Entries expire after 30 seconds. Cleared on cancel to prevent stale matches. Consuming a pending cache entry also consumes the corresponding pre-placement entry.

### Pending Orders Cache

After REST returns order IDs, `orderId -> { type, price }` is cached. The WS handler consumes this on first match and also consumes the corresponding pre-placement entry.

Entries expire after 60 seconds.

## Testnet / Demo

### URL Differences

| Component     | Mainnet                           | Testnet                                |
| ------------- | --------------------------------- | -------------------------------------- |
| REST base URL | `https://api.hyperliquid.xyz`     | `https://api.hyperliquid-testnet.xyz`  |
| WebSocket     | `wss://api-ui.hyperliquid.xyz/ws` | `wss://api.hyperliquid-testnet.xyz/ws` |

### Feature Limitations on Testnet

- **HIP-3 DEX whitelist**: Only these DEXes are fetched on testnet: `['vntls', 'xyz', 'VOLMEX', 'flx']` (`TESTNET_HIP3_WHITELIST`). This constrains the large testnet catalog to four metadata calls.
- **Signing differences**: Phantom agent `source: "b"` (vs `"a"` for mainnet), `hyperliquidChain: "Testnet"` (vs `"Mainnet"`). The wallet-key signing path (`signUserSignedAction`) uses the same `chainId: 421614` and `signatureChainId: "0x66eee"` on both mainnet and testnet. The browser-wallet path uses `chainId: 421614` (Arbitrum Sepolia) for testnet vs `42161` (Arbitrum One) for mainnet.
- **Leverage defaults**: Defaults to 20x cross margin until WebSocket delivers leverage info.

## Known Quirks / Implementation Notes

1. **HIP-3 `orderUpdates` missing fields**: `orderType`, `triggerPx`, `tpsl`, `tif` are omitted (API limitation). Requires the multi-layer inference system described above.
2. **Aggregate feeds are not drop-in reconciliation replacements**: `webData2` is rejected and current `webData3` contains frontend user/DEX status but no positions, orders, or ticker contexts. Current `allDexs*` live frames also differ from their published record-shaped contract and do not remove the metadata dependency. Do not adopt them without a concrete consumer, stable mapping, and refreshed upstream evidence.
3. **Balance scoping depends on account abstraction mode**: `clearinghouseState` with `dex` returns dex-specific margin only. In `unifiedAccount`/`portfolioMargin`, all collateral and holds are in spot state: use the current `spotState` WS channel, not the rejected `spotClearinghouseState` WS channel. Token 0 `tokenToAvailableAfterMaintenance` is authoritative free USDC; only responses without it fall back to core margin plus spot-hold reconstruction. Keep `currencies.USDC` as the aggregate margin slice for sizing; raw spot slices belong under `balance.info.spotCollateralCurrencies`.
4. **`activeAssetData` not available for HIP-3**: Only subscribes for core perp markets (channel limit concern). HIP-3 leverage defaults until WS delivers.
5. **Case-insensitive coin names**: HIP-3 API may return coin names with inconsistent casing. All lookups fall back to `marketsLowerCaseMap`.
6. **Market orders have no IOC**: Sent as GTC limit at 5% slippage from mark price.
7. **TP/SL update is cancel+replace**: No in-place modification supported. Dead order tracking (`_deadOrders` Set) prevents the cancelled order from reappearing via stale WS messages.
8. **120s periodic position REST**: Safety net for HIP-3 positions; likely redundant with WS but kept for reliability.
9. **TP/SL type re-evaluation on update**: When dragging a TP/SL order past the current market price, the order type switches (TP to SL or vice versa) to prevent accidental position closure.
10. **Fake positions cache**: `_fakePositionsCache` is invalidated only when leverage or margin type changes. Markets loading does not invalidate it — positions are generated directly from fetched markets during startup.
11. **Builder fee lazy check**: If `_builderFeeApproved` is null when placing an order, `getBuilderInfo()` fetches status on-the-fly before returning.
12. **Order status for unconfirmed HIP-3**: Placeholder orders use `OrderStatus.Creating` to signal the UI that the order is still syncing and should not allow full interaction.
13. **Spot WebSocket URL difference**: Spot uses `wss://api.hyperliquid.xyz/ws` (not `api-ui`), as per `hyperliquidspot.types.ts`.
14. **placeOrders (batch)**: Uses `mapSeries` (sequential), not parallel. Each order is placed one at a time.
15. **Legacy accounts may be missing `applicationId`**: Auto-resolved at startup via `userRole` info type. See "applicationId auto-resolution" under Authentication for the resolver contract; don't refactor it without updating the consumers that read `options.applicationId` directly. The resolved value is not persisted upstream — every startup pays one `userRole` round-trip for affected accounts.
16. **`midPx` populates `ticker.midPrice`**: Hyperliquid's `bid`/`ask` come from `impactPxs`, NOT L1 top-of-book — they're impact-weighted prices and notably wider than the true mid. The canonical mid (`allMids`, or `sourceTicker.midPx` from REST) is therefore populated on **both** `ticker.last` AND `ticker.midPrice` so consumers that want the published mid (e.g. the cli's `$mid` constant) read `ticker.midPrice` directly rather than computing `(bid+ask)/2`. Don't drop `midPrice` from any path that already sets `last = midPx`.
17. **`userAbstraction: "default"` is currently treated as standard fallback**: unified collateral merge only enables for explicit `unifiedAccount` / `portfolioMargin`. If Hyperliquid remaps `"default"` semantics in the future, update `shouldUseSpotCollateralBalance`.
18. **DEX abstraction deprecation can be per-account before global date**: users can be migrated early via `agentSetAbstraction` / UI flows before network-wide milestones. Never gate balance mode on calendar date; always read runtime `userAbstraction`.

## Error Handling

### API Response Error Detection

The response interceptor in `hyperliquid.api.ts` checks multiple error patterns in the response:

| Pattern                               | Location in Response | Handling                                       |
| ------------------------------------- | -------------------- | ---------------------------------------------- |
| `data.status === "err"`               | Top-level            | Emit error, throw with `data.response` message |
| `data.response.data.status === "err"` | Nested response      | Emit error, throw with nested message          |
| `data.statuses[].error`               | Array of statuses    | Join error messages, emit and throw            |
| `data.response.data.statuses[].error` | Nested status array  | Join error messages, emit and throw            |

### `HyperliquidAPIError` Class

Custom error class in `hyperliquid.errors.ts` with `code` and `message` properties. `handleApiError()` classifies errors:

| Condition                           | Error Code                            | Description                           |
| ----------------------------------- | ------------------------------------- | ------------------------------------- |
| `error.response` exists             | From response data or `UNKNOWN_ERROR` | Server returned non-2xx status        |
| `error.request` exists, no response | `NETWORK_ERROR`                       | Request sent but no response received |
| Neither                             | `REQUEST_SETUP_ERROR`                 | Error during request construction     |

### Order Validation Errors

`validateResponseAndExtractOrderIds()` throws:

- `"Invalid response from Hyperliquid API"` — if `data.response.data.statuses` is missing
- `"Hyperliquid order error: ..."` — joins all status errors from the response

### Retry Strategies

| Component                   | Strategy                         | Details                                                |
| --------------------------- | -------------------------------- | ------------------------------------------------------ |
| REST client (`axios-retry`) | 3 retries on network error       | Only `isNetworkError` triggers retry (not HTTP errors) |
| HIP-3 order refresh         | 5 attempts, progressive delay    | 300ms, 700ms, 1500ms, 3000ms, 5000ms                   |
| Order REST refresh          | Debounced 500ms, min 1s interval | Coalesced via `scheduleOrderRefresh()`                 |
| HIP-3 position REST         | Every 120s safety net            | Sequential per-DEX, fire-and-forget                    |

### Startup Error Resilience

- `fetchBuilderFeeApproval()`: Logs warning on failure, sets `_builderFeeApproved = null` (lazy re-check on first order)
- `fetchCorePerpsState()`: Returns null on failure; store falls back to `{ total: 0, free: 0, used: 0, upnl: 0 }` until WebSocket populates
- `resolveUserAbstractionMode()`: Bounded startup timeout; on timeout/error/unexpected value, falls back to standard balance mode. Public market hydration does not wait on this call, but private WS startup does.
- Unified spot collateral refresh: on failure logs warning and continues with core-perp snapshot view until next refresh succeeds. After an empty spot snapshot is received, unified balance math stays active with zero spot collateral.
- `fetchOrders()`: Logged as warning on failure, does not block startup
- `fetchAllHip3Positions()`: Logged as warning on failure, does not block startup
- Fee defaults: If `userFees` fetch fails, defaults to `takerFee: 0.00045`, `makerFee: 0.00015`

### Error-as-Success Patterns

- Cancel response with `status: "err"` is treated as a real error (interceptor throws)
- Order placement response with any `status.error` in the statuses array causes full rejection of the batch

## Testing / Validation

### Testing Checklist

- [ ] Account validation with wallet address + private key
- [ ] Balance fetching from REST on startup and WS updates
- [ ] `userAbstraction` resolution + fallback path (`timeout/error -> standard`)
- [ ] Standard-mode balance path unchanged (`clearinghouseState` only)
- [ ] Unified/portfolio balance merge path (REST `spotClearinghouseState` + WS `spotState`)
- [ ] Spot collateral refresh on `userNonFundingLedgerUpdates` (throttled)
- [ ] Core perp markets loading from `metaAndAssetCtxs`
- [ ] HIP-3 markets loading from `perpDexs` + per-dex `metaAndAssetCtxs`
- [ ] Order placement: Limit, Market, TakeProfit, StopLoss (core + HIP-3)
- [ ] Order placement with TP+SL attached (normalTpsl grouping)
- [ ] Positional TP/SL (positionTpsl grouping, sz=0)
- [ ] Partial TP/SL (na grouping, sz>0)
- [ ] PostOnly (Alo) time-in-force
- [ ] Order update (cancel + replace) with type re-evaluation
- [ ] Order cancellation with suppression
- [ ] HIP-3 order type inference (pre-placement cache, pending cache, retry chain)
- [ ] Position tracking: core perps (`clearinghouseState` WS + REST reconciliation)
- [ ] Position tracking: HIP-3 (clearinghouseState with dex + REST 120s safety net)
- [ ] Leverage change and margin type switch
- [ ] Fill notifications with cross-instance deduplication
- [ ] Builder fee approval and inclusion in orders
- [ ] Agent wallet creation via browser wallet
- [ ] Vault trading (vaultAddress in requests)
- [ ] Orderbook subscription and depth display
- [ ] OHLCV candle data (REST fetch + WS streaming)
- [ ] Ticker updates via public core `allMids` and dex-scoped HIP-3 `allMids`
- [ ] Testnet mode (whitelisted HIP-3 DEXes)
- [ ] WebSocket reconnection handling
- [ ] Fake positions generation for all markets
- [ ] Realized PnL tracking across fill lifecycle

## Monitoring and Debug Logs

| Log Pattern                                                       | Meaning                                            | Action                                                      |
| ----------------------------------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------- |
| `HIP-3 order type inference: source=hip3-unconfirmed-placeholder` | Cache miss, type unknown until REST confirms       | Normal during init; investigate if frequent in steady state |
| `Cancel suppression expired for order`                            | REST shows cancelled order as still open after 30s | May indicate cancel failure                                 |
| `Failed to fetch orders from HIP-3 dex`                           | REST fetch failed for a dex                        | Orders for that dex are preserved (not removed)             |
| `Found market via case-insensitive lookup`                        | HIP-3 coin name case mismatch                      | Normal for HIP-3; ensure map is kept up to date             |

## CCXT Reference Implementation

- **REST**: https://github.com/ccxt/ccxt/blob/master/ts/src/hyperliquid.ts
- **WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/hyperliquid.ts
