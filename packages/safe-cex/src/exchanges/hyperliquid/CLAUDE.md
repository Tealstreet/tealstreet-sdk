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

## Rust public-feed ownership

The Rust perpetual engine uses one account socket for private and public topics.
`PublicFeedPolicy::Disabled` and `Shared` retain private subscriptions, account
reconciliation and pings, while skipping public mids/contexts, books and candles
on initial connection and reconnect. Public subscription commands are intercepted
by the engine for both policies; unsolicited public payloads on the private socket
are ignored. Shared ticker input updates native state without replacing local
market metadata or account settings. Shared quote changes invoke native account
revaluation so UPNL and available margin follow provider prices.

## Rust margin settings and standalone spot orders

HIP-3 assets get no `activeAssetData` subscription (channel limit), so the Rust
engine's startup settings seed is what leverage/margin writes start from. Assets
with `onlyIsolated` or `marginMode: strictIsolated | noCross` seed isolated, and
every seed caps leverage at the market maximum. Cross on such an asset is refused
before signing. The standalone `hyperliquidspot` engine ignores `orderUpdates`, so
it reads `frontendOpenOrders` at startup, on each order refresh and every 10 s.

## File Structure

### Core Perpetual Futures

| File                         | Description                                                                                                                                                                                                                                                                                                         |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hyperliquid.exchange.ts`    | Main exchange class extending `BaseExchange`. Orchestrates startup, order/position management, HIP-3 lifecycle, builder fee, cancel suppression, and tick loop.                                                                                                                                                     |
| `hyperliquid.api.ts`         | Axios client factory. Creates two clients: `createExchangeAPI` (for `/exchange` endpoint) and `createInfoAPI` (for `/info` endpoint).                                                                                                                                                                               |
| `hyperliquid.types.ts`       | All TypeScript interfaces: REST responses, WS payloads, order wires, HIP-3 types (`PerpDex`, `PerpDexs`), builder fee types, candle types, etc. Also defines `BASE_URLS`, `WSS_URLS`, `ENDPOINTS`, `INFO_TYPES`, `EXCHANGE_TYPES`, `WEBSOCKET` constants.                                                           |
| `hyperliquid.constants.ts`   | WS subscription type lists (`singularSubscriptionTypes`, `systemSubscriptionTypes`), Tealstreet builder wallet addresses (mainnet/testnet), the DEFAULT builder fee (10 tenths bps = 1bp), and `HYPERLIQUID_BUILDER_TIERS` — the whole 10/30/50 ladder, which the standalone approval page validates a URL against. |
| `hyperliquid.signing.ts`     | EIP-712 phantom agent signing, L1 action signing, user-signed action signing (agent approval, builder fee, USD transfer, withdraw). Contains `floatToWire`, `orderRequestToOrderWire`, order action builders (`orderWiresToOrderAction`, `orderWiresToNormalTPSLAction`, `orderWiresToPositionalTPSLAction`).       |
| `hyperliquid.ws-public.ts`   | Public WebSocket: `allMids` ticker updates, `l2Book` orderbook, `candle` OHLCV. Connects to `wss://api-ui.hyperliquid.xyz/ws`.                                                                                                                                                                                      |
| `hyperliquid.ws-private.ts`  | Private WebSocket handling `clearinghouseState`, mode-gated `spotState` (unified/portfolio), HIP-3 `allMids`, `activeAssetData`, `orderUpdates`, `userFills`, `userEvents`, `userFundings`, and `notification`.                                                                                                     |
| `hyperliquid.utils.ts`       | `symbolToCoin()` (display symbol to API coin format), `calculatePricePrecision()` (CCXT-compatible price precision algorithm).                                                                                                                                                                                      |
| `hyperliquid.errors.ts`      | `HyperliquidAPIError` class and `handleApiError()` utility.                                                                                                                                                                                                                                                         |
| `hyperliquid.rateLimiter.ts` | Shared weighted REST limiter (1200 weight/minute) plus request-weight classification for `/info` and `/exchange`.                                                                                                                                                                                                   |
| `hyperliquid.agent.ts`       | Agent wallet (API wallet) creation and approval via browser wallet (MetaMask). Handles EIP-712 `ApproveAgent` signing, network switching (Arbitrum One / Sepolia).                                                                                                                                                  |
| `hyperliquid.builderFee.ts`  | Builder fee approval and checking. EIP-712 `ApproveBuilderFee` signing, `checkBuilderFeeApproved()` REST check, `approveBuilderFeeWithBrowserWallet()` full browser flow.                                                                                                                                           |
| `hyperliquid.cross-cache.ts` | Cross-context singleton cache for order identification and fill deduplication. Uses `sessionStorage` + `BroadcastChannel` for cross-worker/iframe synchronization. Manages the cloid placement records, pre-placement cache, pending orders cache, and seen fills set.                                                                      |
| `hyperliquid.cloid.ts`       | Client order IDs (cloids): `mintHyperliquidCloid`, the caller-ID rule (`hyperliquidClientOrderIdRule`), and `placementRecordFromWire`, which turns a sent wire into the record the WS handler reads. See "Client order ID" below. |

### Spot Trading

| File                             | Description                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------- |
| `hyperliquidspot.exchange.ts`    | Spot exchange class extending `BaseExchange`. Uses `spotMetaAndAssetCtxs` for markets. |
| `hyperliquidspot.api.ts`         | Spot REST API client (same base pattern as perps).                                     |
| `hyperliquidspot.types.ts`       | Spot-specific type definitions and constants.                                          |
| `hyperliquidspot.constants.ts`   | Spot constants.                                                                        |
| `hyperliquidspot.errors.ts`      | Spot error handling.                                                                   |
| `hyperliquidspot.rateLimiter.ts` | Spot rate limiter.                                                                     |
| `hyperliquidspot.ws-public.ts`   | Spot public WebSocket.                                                                 |
| `hyperliquidspot.ws-private.ts`  | Spot private WebSocket.                                                                |

Spot private subscriptions use current `spotState`, `userFills`, notifications
and non-funding ledger updates. `activeAssetData` supports perpetuals only;
legacy balance response parsing remains for compatibility. Spot fill callbacks
preserve execution `tid` as `id` and native `oid` as `orderId` for chasers.

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
| `options.key`           | Signer/API-wallet address | May be an API agent; do not use as the `userFees` account subject                                                                                  |
| `options.secret`        | Private key         | Private key for the wallet (agent or main)                                                                                             |
| `options.applicationId` | Main wallet address | The main wallet address (for wallet-based connections). Used for builder fee checks.                                                   |
| `options.vault`         | Vault address       | Optional vault address (for vault trading). When set, sent as `vaultAddress` in exchange requests and used as `user` for info queries. |

**`applicationId` is load-bearing.** All private info queries (`clearinghouseState`, `frontendOpenOrders`, etc.) key off `vault || applicationId`. Without one of them, `hasPrivateSessionIdentity` is false → `isPublicDataOnly` flips true → balance/orders/positions never load. Both registration paths now require it: the auto flow (`HyperliquidAutoSetup.tsx`) writes `applicationId = mainWallet` from the connected MetaMask address; the manual form (`ApiKeyFormFragment.tsx`) renders a required "Main Wallet Address" input gated on `requiresApplicationId`. Legacy accounts predating this enforcement may ship without it — see "applicationId auto-resolution" below.

`fetchFee()` queries `userFees` for `vault || applicationId`, after the existing
startup identity resolution. It never queries the API agent's `key` as a fee
account. Explicit public-only sessions and unresolved identities keep the
existing default maker/taker rates without a `userFees` request. Fee response
aliases and request-failure defaults remain the same.

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

Builder fees (builder codes) allow Tealstreet to receive a portion of trading
fees. Builder info is attached to order actions as `{ b: builderAddress, f }`.

**The rate is NOT a constant any more.** An affiliate chooses which of three
builder wallets their referred users route through — 1bp, 3bp or 5bp — so `f` is
whatever `resolveHyperliquidBuilder` returns for that account. 10 tenths of a bp
is only the DEFAULT, taken when nothing configures one. `HYPERLIQUID_BUILDER_TIERS`
holds all three; `venue_builder_tiers` in Postgres is the authority and this is
the browser's copy, kept so the standalone approval page can validate a builder
that arrived in a URL.

**Flow**: On startup, check if user has approved builder fee via `maxBuilderFee` info type (`fetchBuilderFeeApproval`). If approved, include `builder` field in all order actions. Approval requires EIP-712 signing from the main wallet via `approveBuilderFeeWithBrowserWallet()`.

**Builder addresses** (from `hyperliquid.constants.ts`):

- Mainnet: `0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d`
- Testnet: `0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba`

**The address and rate are now a PARAMETER, not a constant.**
`ExchangeOptions.hyperliquidBuilder` names both, and every read goes through
`resolveHyperliquidBuilder(options)`, which falls back to the pair above at 1bp.
That fallback is centralized so a call site cannot re-derive it and drift.

`checkBuilderFeeApproved`'s `requiredFeeTenthsBps` parameter is a deliberate
SECOND copy of the default, so that a caller passing nothing behaves exactly as
it did before tiers existed. **Every web call site now passes the account's own
builder and rate**, which is what closes the gap this paragraph used to describe:
a caller taking the default checks 1bp for a 3bp user and waves through an
approval that does not cover them. If you add a call site, pass the pair.

Tealstreet runs three builder wallets so an affiliate can choose what their
referred users pay; see
[`docs/plans/AFFILIATE_FEE_TIERS_v1.md`](../../../../../docs/plans/AFFILIATE_FEE_TIERS_v1.md).

**Address and rate travel together and must never be mixed.** Approval on
Hyperliquid is per (builder, max rate), so sending one wallet's address with
another's rate is an order the venue rejects — which surfaces as "orders stopped
working", not as a fee problem.

**The SIGNED rate and the CHECKED rate must come from one value.**
`builderFeeRateFromTenthsBps` is that value: it produces the `maxFeeRate` string
the user signs AND formats the approval check's reading. `DEFAULT_BUILDER_FEE_RATE`
used to be the independent literal `'0.01%'`; had it stayed one while only the
check became configurable, a 3bp user would sign `0.01%`, the venue would record
10, the check would demand 30, and the approval prompt would reappear forever
with no way out.

**Approval status** is cached in `_builderFeeApproved` (null = unknown, true = approved, false = not approved). If unknown when placing an order, `getBuilderInfo()` re-fetches the status. On successful browser-wallet approval it emits the `hyperliquid.builder-fee-approved` telemetry event (`reportEvent`); the web host's sink turns that into the `/api/logger/builder-fee` POST.

`HyperliquidExchange.getBuilderInfo()` is the only order-path builder-info gate. Keep it tied to the live `maxBuilderFee` approval state; do not reintroduce environment-based signing helpers.

**Open feature: sub-builders.** Hyperliquid order actions now allow
`builder.s`, an optional list of up to five sub-builders that receive part of the
same builder fee. Each entry is `{ b, s }`, where `b` is an existing user address
and `s` is the share of the builder fee in 0.001% units; shares must be positive
and sum to at most 100%, with the primary builder receiving the remainder. This
could support direct partner/affiliate revenue sharing later, but it is NOT wired
today: `getBuilderInfo()` still returns only `{ b, f }`. Before implementing,
verify approval semantics, partner wallet eligibility, revenue ingestion, and
fallback behavior for accounts whose builder approval predates any split.

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
│       └── For each DEX (strictly sequential, budget = dexCount x 2s, floor 8s, cap 15s): POST /info { type: "metaAndAssetCtxs", dex: name }
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

| Endpoint         | Purpose                                                                               | Rate Limit                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `POST /info`     | Read operations (markets, positions, orders, balances, candles, fills)                | Shared 1200 weight/minute REST budget via `hyperliquidRestRateLimiter`; weighted by `type` |
| `POST /exchange` | Write operations (place/cancel/modify orders, update leverage, approve agent/builder) | Shared 1200 weight/minute REST budget plus the legacy 3 RPS write-client guard             |

The official Hyperliquid limit is a shared per-IP weighted REST budget, not separate per-client request-count buckets. Most `/info` calls cost 20, `userRole` costs 60, selected low-cost reads such as `clearinghouseState` cost 2, and unbatched `/exchange` actions cost 1. Keep `createExchangeAPI` and `createInfoAPI` on the shared weighted limiter so multiple Hyperliquid accounts in one session cannot each spend the full budget during startup or reconnect.

### Info Types (POST /info, `type` field)

| Type                            | Purpose                                                          | When Called                                   |
| ------------------------------- | ---------------------------------------------------------------- | --------------------------------------------- |
| `metaAndAssetCtxs`              | Market metadata + ticker data (core perps)                       | Startup                                       |
| `metaAndAssetCtxs` with `dex`   | Market metadata per HIP-3 DEX                                    | Startup (per DEX)                             |
| `perpDexs`                      | List of HIP-3 DEXes                                              | Startup                                       |
| `userFees`                      | User fee rates (taker/maker)                                     | Startup                                       |
| `maxBuilderFee`                 | Check builder fee approval status                                | Startup                                       |
| `userRole`                      | Resolve missing applicationId from key                           | Startup (legacy accounts only)                |
| `userAbstraction`               | Resolve abstraction mode (`unified`/`portfolio`/`standard`-like) | Startup (private accounts)                    |
| `clearinghouseState`            | Account positions + margin (core perps)                          | Startup (initial balance)                     |
| `clearinghouseState` with `dex` | HIP-3 positions per DEX                                          | Startup + every 120s safety net               |
| `frontendOpenOrders`            | Open orders with type/trigger info (core perps)                  | Startup + on-demand refresh                   |
| `frontendOpenOrders` with `dex` | HIP-3 open orders per DEX                                        | Startup + on-demand + retry chain             |
| `candleSnapshot`                | OHLCV candle data                                                | On chart load                                 |
| `userFills`                     | User fill history                                                | Executions tab                                |
| `userFillsByTime`               | User fills filtered by time                                      | Executions tab, ledger (`fetchIncomeHistory`) |
| `userFunding`                   | Funding payments                                                 | Ledger (`fetchIncomeHistory`)                 |
| `spotMetaAndAssetCtxs`          | Spot market metadata                                             | Spot startup                                  |
| `spotClearinghouseState`        | Spot balances / holds (collateral source for unified/portfolio)  | Spot exchange + perps unified/portfolio       |

### Public ticker field units

`metaAndAssetCtxs` / `spotMetaAndAssetCtxs` field names are easy to misread:

- `dayNtlVlm` is already 24h notional/quote volume. Do **not** multiply it by price again when populating `Ticker.quoteVolume`.
- `dayBaseVlm` is the base-asset volume. Use it for `Ticker.volume` when present; otherwise derive base volume as `dayNtlVlm / price`.
- Perp `openInterest` is base units, not USD notional. Convert it with mark price (falling back to mid) before writing `Ticker.openInterest`, because screener/dropdown per-exchange OI is displayed in notional terms and compared against USD aggregate OI.
- `prevDayPx` is present for the 24h change calculation. Guard malformed/zero previous prices, but do not treat a missing calculation as proof the venue does not publicize the field.

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
| `scheduleCancel`       | Set or clear the account-wide dead man's switch                    |
| `modify`               | Modify existing order                                              |
| `batchModify`          | Batch modify orders                                                |
| `updateLeverage`       | Change leverage/margin type for a market                           |
| `updateIsolatedMargin` | Adjust isolated margin                                             |
| `usdSend`              | Transfer USDC                                                      |
| `withdraw3`            | Withdraw funds                                                     |
| `vaultTransfer`        | Transfer to/from vault                                             |
| `approveAgent`         | Approve agent wallet                                               |
| `approveBuilderFee`    | Approve builder fee                                                |

### Future API announcements

As of 2026-10-30 these upstream changes have been announced but should not be
implemented during the code freeze:

- Hyperliquid announced that, on a future network upgrade, native TWAP orders
  with a stop price will be paused instead of canceled when the stop price is
  breached, and can resume if the stop price is no longer breached within the
  TWAP duration. The TS reference adapter does not currently place
  venue-native TWAP actions; Tealstreet TWAP is a worker/Rust algo that slices
  ordinary orders. If native Hyperliquid TWAP support or TWAP history/status
  handling is added later, do not model a stop-price breach as a terminal
  cancellation without checking for the new paused/resumable state.
- Hyperliquid also reminded integrators that batched actions are treated with
  less weight than equivalent unbatched actions during congestion. The current
  public docs still say `/exchange` IP-based request weight is
  `1 + floor(batch_length / 40)`, while batched requests count as `n` requests
  for address-based limits. `hyperliquid.rateLimiter.ts` already models the
  documented batch-length weight for `order`, `cancel`, `cancelByCloid`, and
  `batchModify`. After the freeze, prefer batching bursty order/cancel/modify
  flows where semantics allow it, but preserve per-order response mapping,
  cancel suppression, optimistic/reconciliation behavior, and the no-retry rule
  for writes whose accepted/failed state may be unknown.

### Dead man's switch (`scheduleCancel`)

`HyperliquidExchange.scheduleCancel(time)` exposes the upstream account-wide
dead man's switch through both the direct exchange API and the typed worker
method bridge. `time` is an absolute UTC millisecond timestamp and must be a
safe integer at least five seconds after `virtualClock.getCurrentTimeMs()`.
Passing `null` or omitting the argument removes the existing schedule by
omitting `action.time` from the signed L1 action; never serialize `time: null`.
The request uses the normal monotonic nonce, vault-aware phantom-agent
signature, `/exchange` transport, and propagates transport/upstream errors.

Rust perpetuals expose the same method through the native `scheduleCancel`
capability (wire flag 25); the spot adapter refuses it. SDK and direct host ingress
reject unsafe numbers before JSON can turn them into a clear. Native admission
and dispatch both recheck the five-second deadline using the synchronized host
clock. Controls share the existing 334 ms FIFO, signer nonce and vault-aware
signer with ordinary writes, without requiring a catalogue or builder check.
Nonce sharing remains bounded to one native process or WASM module.

Native completion requires a 2xx `status: ok` object response with `type: default`
and absent/null data or an empty `statuses` list. This conservative control ACK
shape is inferred from the existing SDK control response, not a live captured
schedule ACK. An explicit native `err` string below HTTP 500 (except 408) proves
rejection; transport loss, 408, 5xx and malformed ACKs remain uncertain. Both
native and worker/host interruption paths preserve that uncertainty, perform no
retry and never claim account/order reads prove the switch is armed. Preflight
and signing refusals remain `notSent`. Native success resolves the SDK's void
promise without mutating orders, settings or a local scheduling flag.

Lifecycle ownership is deliberately explicit. Safe-cex does not auto-arm,
refresh, or clear this switch during startup/disposal, and there is no implicit
account setting. A strategy or UI that opts in must periodically move the
deadline forward while healthy and treat every rejected refresh as unarmed.
This avoids silently cancelling orders when a browser worker is suspended and
avoids consuming Hyperliquid's limit of ten actual dead-man triggers per UTC
day. The upstream deadline cancels all open orders for the account, including
core perps, HIP-3, and spot; it is not scoped to the current safe-cex instance.

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

| Channel         | Subscription                                      | Data                                              |
| --------------- | ------------------------------------------------- | ------------------------------------------------- |
| `allMids`       | `{ type: "allMids" }`                             | Mid prices for all coins (ticker updates)         |
| `fastAssetCtxs` | `{ type: "fastAssetCtxs" }`                       | Raw-DEFLATE base64 mark/mid snapshot and diffs    |
| `l2Book`        | `{ type: "l2Book", coin: "BTC" }`                 | Standard 20-level orderbook depth, on demand      |
| `bbo`           | `{ type: "bbo", coin: "BTC" }`                    | Fresh best bid/ask overlay for each active ladder |
| `candle`        | `{ type: "candle", coin: "BTC", interval: "1m" }` | OHLCV candles (per coin+interval, on-demand)      |

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
- `onMessage`: **deliberately** calls `recordMessageHandlerStats('pong')` unconditionally on every inbound frame, in addition to the channel-specific stats record. This is a load-bearing hack against Hyperliquid's flaky pong cadence: the staleness watchdog (in `apps/web/src/utils/misc/getGroupedMessageTopicStats.ts`) gates on `now - pong.lastMessageTimestamp`, and without this bump the public WS gets falsely flagged as dead and reconnects even when tickers/orderbooks are flowing normally. **Do not "fix" this** — same hack lives in `hyperliquidspot.ws-private.ts → onMessage` for the same reason. It also survives `reconcileMessageHandlerStats`, which only zeroes topics that once had a `messageHandlers` entry: on this venue `pong` is a switch case rather than a handler key, so it keeps the `: 1` presumption and stays visible by design (venues that do register a real `pong` handler are reconciled like any other topic). The channel-specific record it sits beside is now the per-symbol `bbo.{coin}` / `candle.{s}.{i}` key rather than the bare channel — see `../CLAUDE.md` → "`messageHandlerStats` checks SUBSCRIPTIONS, not the connection" for why the bare key could never go idle. If the perp private WS ever starts spuriously reconnecting, that's the next place to apply the hack (it currently only records pong inside the dedicated pong branch).

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
| `spotState`                   | Conditional (unified/portfolio mode only) | No                              | Nested spot state; token 0 `tokenToAvailableAfterMaintenance` is authoritative unified free margin                |
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

## WebSocket Diagnostics

Hyperliquid public/private WebSocket diagnostics go through `BaseWebSocket`'s runtime-gated `emitWsDiagnostic*` helpers. They only populate the web Requests drawer when the Requests Log toggle enables WS diagnostics, and Grafana upload remains behind the separate WS upload opt-in. Keep wallet/user addresses redacted before emission.

Public diagnostics should include outbound subscribe/unsubscribe control frames and inbound subscription/error control frames. Do not log high-volume public market-data inbound frames (`allMids`, `fastAssetCtxs`, `l2Book`, `bbo`, `candle`) on the receive side.

Private diagnostics should include outbound user/order subscription frames and inbound order/user lifecycle frames (`orderUpdates`, `userEvents`, `userFills`, funding/non-funding updates, notifications, subscription responses, errors). Do not log high-volume private `activeAssetData` subscribe bursts or inbound account/market-state floods such as `activeAssetData`, `clearinghouseState`, `spotState`, `spotClearinghouseState`, `allMids`, or heartbeat pongs.

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

### Reduce-only sizing

Hyperliquid **rests a reduce-only order at whatever size it was sent** — it does not cap it to the open position server-side. A reduce-only sell of 336 against a 168-contract long stays open at 336. So the venue sets `autoDownSizeReduceOrders: true`, routing through the shared `downSizeReduceOrder` util in the web limit/market builders. Do not hand-roll a clamp in `placeOrder`/`createLimitOrderWires`; see `../CLAUDE.md` → "Reduce-only order downsizing". `hyperliquidspot` deliberately stays on the default `false`: spot has no positions, and `resolveReduceOnly` already forces `r:false` for `market.isSpot`.

For standalone TP/SL orders, explicit full-size reduce-only requests must be
routed as positional TP/SL (`grouping: "positionTpsl"`, `s: "0"`). This avoids
Hyperliquid's `$10` minimum notional floor rejecting tiny-but-valid full-position
stops. Partial TP/SL (`s > 0`) still uses standalone `na` grouping and can be
rejected by the venue if the requested partial size is below the minimum.

Market and trigger orders must not build the wire from a missing or zero ticker
reference price. A zero `p` on Hyperliquid trigger-market orders can behave like
a marketable close, so `placeOrder()` fails before POST when no finite positive
mark/last/mid/index price is available.

Position TP/SL placements can return `"waitingForTrigger"` without an `oid`.
Treat that as accepted-but-not-yet-identified: schedule a REST order refresh and
let the UI's optimistic order reconcile from WS/REST instead of surfacing an
"unknown order status" error.

Order response validation lives in `hyperliquid.order-response.ts`. Keep it
aligned with the official `/exchange` docs and CCXT: successful creates return a
`statuses` vector containing `{ resting: { oid } }`, `{ filled: { oid, ... } }`,
or `"waitingForTrigger"` for accepted TP/SL triggers; per-order rejections return
`{ error }` entries in that vector; whole-payload pre-validation can instead
return a single top-level `{ status: "err", response: "..." }` message. A `null`
or empty `/exchange` response body is not one of those accepted shapes and must
remain a clear transport/venue failure, not an accepted empty-id result.

**A non-2xx reaching that validator at all is the bug behind
`Invalid response from Hyperliquid API: null`.** The HOW tunnel adapter
resolves without axios' `settle()`, so a 4xx/5xx from Hyperliquid or from the
egress proxy arrives at the response interceptor as a _success_ with its status
intact but never read — and the caller then reports the body SHAPE instead of
the failure. Hyperliquid's own error bodies are not JSON objects (`422` returns
the bare text `Failed to deserialize the JSON body into the target type`), so
what the user saw was a body-shape complaint naming nothing actionable, every
one of them ending in `: null`. The
interceptor now reads `response.status`, and appends it to the venue's own error
text rather than choosing between them, via the shared
`describeHttpFailure('Hyperliquid', response)` in `utils/httpFailure.ts` — there
is no hyperliquid-specific formatter any more. It emits the status NUMBER and
never the canonical reason phrase; see `packages/safe-cex/CLAUDE.md` for the
health-classification collisions that rule exists to avoid. Only tunnelled
requests reach this branch; a direct axios request already rejects non-2xx in
`validateStatus`.

**That status branch is scoped to `/exchange` on purpose — do not widen it to
the shared interceptor.** `/info` answers 422 for probe types we ask about
optionally (`perpDexs`, `userRole`, `userAbstraction`) and 429 under load. Every
one of those is already swallowed by its caller, so throwing turns a silent
fallback into a full error toast per attempt, multiplied by the startup retries
and the 120s HIP-3 sweep — and `errorPatterns.ts` has no arm for
`Unprocessable Entity`, so it classifies `unknown` and toasts at error level. A
401/403 from the _egress proxy_ would classify as `auth_invalid` and tell the
user their API key is bad for what is a transport failure. Worse, `/info`
failures reach paged collectors: `fetchHistoricalFills` wraps its whole window
loop in one try/catch returning `[]`, and `collectAccountAxis` reads a resolved
`[]` as a successfully-empty window — so one 429 mid-walk would close the
`historical_coverage` row over a gap that is then never re-asked.

**Cancel of already-gone TP/SL after flatten is not a placement failure.** Close
All / chart flatten places a reduce-only market, then `createMarketOrder`
cancels leftover close-side TP/SL. Hyperliquid often already dropped those
orders with the fill, so `/exchange` cancel returns
`Order was never placed, already canceled, or filled. asset=N` (BTC is `0`)
once per oid. Dedupe + squelch that miss only — see Error-as-Success below.

An `/exchange` POST that fails **after** leaving the browser means the placement
is **unknown, not failed** — the order may be resting right now. `postOrderAction`
therefore schedules a REST order refresh on ANY rejection and rethrows: a tunnel
timeout, an egress 502 and an unreadable body all reject _before_ the response
parser runs, so hanging the refresh off the parse would miss every one of them.
`parseHyperliquidOrderResponse` additionally throws the typed
`HyperliquidUnreadableOrderResponseError` (venue rejections stay plain `Error`s)
so the 2xx-with-unreadable-body case reconciles too. Without this the user gets
a "failed to place" toast for a stop that landed and places it a second time.

One more trap in that empty-body branch: `BaseExchange.signHttpRequest` captures
a signed config by dispatching through a per-request adapter that resolves
`{data: null, status: 200}` without sending anything, and interceptors still
run. It is guarded with `isSigningCaptureRequest`, or every hosted-mode
signature of an `/exchange` action rejects inside the signer.

Still unexplained: **nothing in the tunnel path can produce `data === null`**, so
the original `: null` reports are not yet root-caused. The status now rides along
on the next occurrence, which should name it.

The optimistic ghost row in `inProgressOrdersAtom` will keep its `optim_*` id
forever in this case — we never get a real id to `confirm()` the reconciler
against. **The primary bridge is now the cloid**: the stop's ghost carries the
cloid `mintClientOrderId` gave it (`createStopLossOrder` sends through
`sendOrder`), a drag's `Updating` ghost is stamped with the replacement's cloid
from `updateOrder`'s `ReplacementOrderIds`, and the store order carries the
same `clientOrderId` from WS or REST, so `getOpenOrders` pairs them exactly.
The four mechanisms below remain the fallback for orders without one (placed
from another client) and for the window before a drag's result arrives:

1. `apps/web/src/utils/order/getOpenOrders.ts` — `mergeOrdersWithInProgress`
   fuzzy-matches `Creating` AND `Updating` trigger-like ghosts to same-slot
   Open orders in `store.orders` (same account/symbol/side/type/reduceOnly)
   and hides the ghost the moment the real WS-delivered order lands.
   It also collapses older same-slot Creating/Updating ghosts against the
   newest `lastUpdate`, because degraded WS can leave multiple no-id stop
   attempts alive inside the 15s safety window.
   Protective stops sit one-per-slot on every venue we support, so any Open
   match is the placement.
2. `apps/web/src/order/createStopLossOrder.ts` — on `waitingForTrigger` we call
   `optimistic.reject(new OptimisticOrderPlacementError(...))` so a click on the
   ghost's cancel-X in the sub-second gap before WS arrives fails fast instead
   of hanging on the reconciler's 15s timeout. The user can cancel via the real
   order's own row.
3. `apps/web/src/order/updateOrder.ts` — a chart drag of a positional TP/SL is
   a cancel+replace, and the placement half also comes back as
   `"waitingForTrigger"` with no oid. On that empty result we do NOT flip the
   optimistic ghost to `Open` (that would strand a phantom row on the just-
   cancelled oid at the new price, right next to the real replacement). We
   leave the ghost as `Updating` so the same fuzzy matcher can reconcile it,
   and we bump `removeUpdateDelay` to 15s so the ghost survives long enough
   for WS/REST to deliver the replacement.
4. `apps/web/src/components/chart/broker/lineManagers/OrderLineManager.ts` —
   the chart has its own adapter identity. When the order list hides a
   Creating/Updating same-slot ghost or a no-id drag gets its real replacement,
   re-key the existing line adapter to the newer same-slot id/real oid instead of remove/create.
   Without that handoff the list is deduped, but the chart still flickers or
   warps as stale and real snapshots race.

**Both paragraphs below are now the FALLBACK.** An order we placed is
identified by its cloid record first (see "Client order ID"), which gives the
exact type, trigger price, reduce-only and tif on the very first frame. The
deferral and the strict stop-entry match only meet frames with no cloid or no
record: orders from another client, and every order on the hosted
exchange-server, which never sees the browser's records.

**Every core-perp reduce-only frame without trigger fields is deferred to REST,
not only the size-0 positional one.** `orderUpdates` never carries `orderType`
/ `triggerPx` / `isTrigger`; the deferral in `handleOrderUpdates` used to
require `origSz === 0`, so a SIZED TP/SL (the legs attached to a limit entry
via `normalTpsl`, or a partial stop) fell through to generic inference and was
inserted as a Limit at `limitPx`. For an attached leg that `limitPx` is the
entry price, which is why a fresh stop sat on the chart's break-even line for
a second or more until REST corrected it. Widened 2026-10-11; the test is
`src/__tests__/hyperliquid.ws-private.reduce-only-defer.test.ts`. The price of
it is that a plain reduce-only limit reaches the store on the next REST refresh
rather than its first WS frame.

**A core-perp STOP ENTRY (non-reduce-only trigger) is recovered from the
pre-placement cache**, which `placeOrder` now fills for core perps as well as
HIP-3 when the order is not reduce-only. `matchPrePlacementTrigger` is a
strict variant of `matchPrePlacement`: trigger-type entries only, same
`origSz`, exactly one candidate, no price match and no FIFO fallback — so a
plain limit frame can never claim a cached stop. Without a hit the frame is
still inserted as a Limit at `limitPx` (the slippage cap), as before; a stop
entry placed from another client therefore still corrects on the next REST
refresh. A later frame for an order already in the store keeps the stored
type and trigger price.

Do not remove any of the four without the others, and do not extend the
ghost's `removeUpdateDelay` beyond 15s — the fuzzy-match makes the shorter
delay a non-issue in the common case, and the 15s floor is the safety net if
WS/REST never delivers.

`fetchOrdersInternal` must dedupe parsed `frontendOpenOrders` rows by oid before
mutating `store.orders`. In degraded/retry-heavy sessions the same stop can be
observed more than once across core/dex REST reads, and the reconciliation loop
uses a pre-mutation `currentOrdersById` snapshot; without input dedupe,
duplicate same-oid rows can both look new and be appended to the store.

WS terminal `orderUpdates` frames (`canceled`, `reduceOnlyCanceled`, etc.) must
also seed the cancel-suppression map before removing the local row. Hyperliquid
`frontendOpenOrders` snapshots can overlap the cancel and return a just-dead
positional TP/SL as still open; without the same suppression used by local
`cancelOrders()`, that stale REST response re-adds the row and the user waits
for a later reconcile to clear it again. Track raw observed REST oids separately
from parsed orders here: `parseOpenOrders()` filters suppressed orders out, and
using only parsed ids would clear the suppression after the first stale response
instead of waiting until REST actually stops returning the order.

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
6. **Placement records** (every order): before the REST call, `recordPlacement` stores one record per wire, keyed by that wire's cloid (`c`). Every wire carries one — see "Client order ID".
7. **HIP-3 pre-placement cache** (fallback): before the REST call, cache `{coin, side, type, price, amount, cloid}` for a frame that arrives without its cloid
8. POST to `/exchange`
9. Validate response via `validateResponseAndExtractOrderIds()` — extracts `oid` from `resting` or `filled` status
10. **HIP-3 post-placement**: Add to pending cache (`addPendingHip3Order`), add optimistic order to store with `OrderStatus.Creating`, start retry chain

### Update Flow (Cancel + Replace)

Hyperliquid does not support in-place order modification for TP/SL. The `HyperliquidExchange.updateOrder` method:

1. Build `newOrder` from existing order + updates
2. **TP/SL type re-evaluation**: If the new price crosses the current market price, the order type changes (e.g., TP becomes SL). This prevents accidental position closure when dragging orders past current price.
3. **HIP-3 optimistic update**: Add an order with `OrderStatus.Updating` at the new price for immediate UI feedback
4. Cancel original order via `cancelOrders()`
5. Mark original as "dead" in `_deadOrders` Set (prevents WS from re-adding it)
6. Place new order with updated parameters under a FRESH cloid (`placeOrderWithCloid`), and resolve to the new oids plus that cloid as `clientOrderId` (`ReplacementOrderIds`). A positional TP/SL replacement answers `"waitingForTrigger"` with no oid, so the cloid is the only handle the caller gets on it.

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

### Spot Markets (loaded into the perp container)

`HyperliquidExchange` loads spot pairs into the **same store** as perps via `fetchSpotMarketsAndTickers()`. This is safe because Hyperliquid addresses all three market kinds from **one asset-index space**, and the symbol namespaces are disjoint:

**Spot is opt-in per account and default-OFF** (`ExchangeAccountConfig.enableSpotMarkets` → `ExchangeOptions.enableSpotMarkets`, surfaced as the "Also trade spot" checkbox on the account form; the venue advertises the toggle via `supportsSpotToggle` metadata). With it off, `fetchSpotMarketsAndTickers` returns early **before issuing the request**, so a perp-only account's market list and request volume are byte-identical to before the merge. Keep that early return ahead of the fetch — the whole justification for merging without a rate-limit regression is that opting out costs literally nothing. The account form's toggle also feeds `getExchangeRuntimeAccountFingerprint` in `apps/web/src/hooks/exchange/useExchanges.ts` so flipping it reloads the connection; both sides of that comparison must coerce to `Boolean` (the account persists `false`, the options omit the key, and `JSON.stringify` drops `undefined` — comparing raw reports a phantom change and reloads every account on the first hook run).

`hyperliquidspot` is **superseded but still registered**: existing saved accounts keep loading against it, while `hiddenFromNewAccountPicker` removes it from the add-account picker so nobody creates a new split account. Do **not** retire it with `enabled: false` — availability gates also decide whether a _saved_ account may register at startup, so that would break every existing user account on it.

| Field                       | Format                                           | Example           |
| --------------------------- | ------------------------------------------------ | ----------------- |
| `market.id`                 | API coin handle from `universe[].name`           | `@1`, `PURR/USDC` |
| `market.symbol`             | `{TOKEN}/{QUOTE}` (both from the `tokens` table) | `HFUN/USDH`       |
| `market.info.hyperliquidId` | `universe[].index + SPOT_ASSET_INDEX_OFFSET`     | `10001`           |
| `market.isSpot`             | `true`                                           | —                 |

Asset-index ranges: **perps `0..N`**, **spot `10000+`**, **HIP-3 `110000+`**. Disjoint by construction, so one instance builds a correct order wire from `market.info.hyperliquidId` for any of them.

**`isSpot` is per-market, never a class field.** Consumers (order forms, margin widget, `generateFakePositionsFromMarkets`, `priceToPrecision`) must branch on `market.isSpot`. Do not reintroduce an exchange-level spot flag — that is exactly the assumption the merge removes.

**Unit-bridged tokens display without their `U` prefix.** HyperCore has no native BTC/ETH/SOL — there is no token named `ETH` anywhere in `spotMeta` (checked across all 477). Those assets are bridged by Unit and issued as `UBTC`/`UETH`/`USOL`. Hyperliquid's own UI shows them as `ETH/USDC`, but **that alias exists only in their frontend** — `spotMeta` and `tokenDetails` both return `UETH`, and `fullName` is `Unit Ethereum`, not `ETH`. `resolveSpotDisplayName` mirrors the alias into `prettySymbol` so our market list matches theirs.

Two guards keep it from being a blind prefix strip, and both are load-bearing:

- Only tokens whose `fullName` starts with `Unit ` qualify, so a token legitimately named `UNI` (fullName `Uniswap`) is never rendered as `NI`.
- The stripped name is rejected when a **distinct** token already owns it. Live data has three: `UPUMP` (Unit Pump Fun) vs a natively-issued `PUMP`, plus `UMON`/`MON` and `UANSEM`/`ANSEM`. Aliasing those would show two unrelated assets under one name. They keep the prefix.

The alias lands on **`base` and `prettySymbol`**; `symbol` and `id` keep the venue's real names, and the raw token is preserved at `info.hlTokenName`. So routing, the order wire, `marketsBySymbol` and the candle cache are untouched.

**`base` must carry the alias — `prettySymbol` alone is not enough.** The market dropdown renders `base || prettySymbol || symbol`, so a raw `base` short-circuits the fallback and the row reads `UBTC` no matter what `prettySymbol` says. Setting `base` also makes spot and perp share a base (`BTC`), which is what lets `computeAmbiguousQuoteBases` detect the clash and render `BTC/USDC` vs `BTC-PERP`; see `apps/web/src/components/marketDropdown/CLAUDE.md`. `base` is safe to alias here because `getMarginCurrency` returns `''` for spot, and coin icons resolve either way (`app.hyperliquid.xyz/coins/UBTC.svg` and `/BTC.svg` both 200).

**`symbol` is deliberately NOT aliased.** It backs saved favourites, layouts, TradingView symbols and the candle cache, and the alias is data-dependent — it backs off the day Hyperliquid lists a real `ETH` token. Aliasing `symbol` would silently break every saved reference at that moment.

**The quote is NOT always USDC — read `universe[].tokens[1]`.** `tokens` is `[baseIndex, quoteIndex]` into the top-level `tokens` table. A live probe on 2026-07-27 found 299 USDC pairs but also **11 USDH, 5 USDT0, and 1 USDE**. Hardcoding `USDC` in the display symbol produced **five duplicate symbols** (`HYPE/USDC` three times — the real one plus `HYPE/USDH` and `HYPE/USDT0`), and because `store.marketsBySymbol` is symbol-keyed, the duplicates silently clobbered each other and made markets unreachable. Reading the real quote token yields 284 live pairs with **zero** collisions. Skip any pair whose base or quote token is missing from the table.

**The asset-context join is by `coin`, NOT positional.** This is the single most dangerous difference from the perp path. `metaAndAssetCtxs` returns `assetCtxs[i]` aligned with `universe[i]`, so the perp code indexes positionally. `spotMetaAndAssetCtxs` does **not**: a live probe on 2026-07-27 returned **316 universe entries against 707 asset contexts**, out of universe order. A positional join mismatched **245 of 316 pairs** — i.e. it silently prices most spot markets off an unrelated asset, with no error. Join on `SpotAssetCtx.coin`. Locked by `__tests__/audit-regressions/hyperliquid.spot-markets-merge.test.ts`. The standalone spot adapter also accepts the official documented coinless format, strictly when the entire contexts array omits `coin` and its length equals the universe. Mixed named/unnamed or differently sized arrays never use positional fallback.

### Rust standalone spot product

`hyperliquidspot` selects `Hyperliquid::new_spot` in the existing Rust crate.
Combined spot/perpetual accounts are the `hyperliquid` engine with
`enable_spot_markets` (below).
Native spot asset IDs are `10000 + universe.index`, with eight-minus-size
price decimals and fixed 1x/cross settings. Both token indices determine the
display symbol and actual quote: `HYPE/USDC` and `HYPE/USDH` must stay distinct.
USD valuation prefers direct USDC prices, otherwise converting through at most
three market hops; unpriced holdings retain raw amounts with zero notional.
Raw token currencies and USD coin
valuation travel in the balance frame; zero derivative positions are represented
by settings. Private tapes are documented inputs, not live funded-account proof.

Spot book subscriptions use full precision plus documented `nSigFigs` 2–5;
the old 1/6 subscriptions could close the upstream connection. Both TS and
Rust retain finest-to-coarsest book aggregation. Placement returns string IDs
for resting and immediately filled acknowledgements; `waitingForTrigger`
contributes no invented ID. Native cancel/amend race and stale-balance guards
are independently tested. Inherited empty history capabilities remain branded.
See `docs/plans/RUST_SPOT_PRODUCTS.md` for review and gate evidence.

Other spot invariants worth keeping:

- **Spot never generates fake positions.** Spot exposure is a coin balance (`Balance.coins`), not a `Position`. Synthesizing zero-size spot rows would also corrupt `DisplayedMarginPostprocessor`, whose long/short exposure is defined over derivatives notional only.
- **Price decimals differ by kind** — 8 for spot, 6 for perps (`SPOT_MAX_PRICE_DECIMALS` / `PERP_MAX_PRICE_DECIMALS`). `priceToPrecision` reads the market to pick; a perp-capped price on a spot pair silently truncates the order price.
- **Pairs with a null `midPx`, or with no matching asset context at all, are skipped** (`if (!sourceTicker?.midPx) continue`) rather than published as unpriceable markets. The no-context case is the likelier one given the 316-universe-vs-707-contexts shape.
- **Spot loading is best-effort**, bounded by `SPOT_MARKETS_LOAD_TIMEOUT_MS` and swallowing its own errors. A slow or failing spot call must never delay or break perp startup.
- `getSpotCollateralMarkPrice()` now prefers the real `{COIN}/USDC` spot ticker and falls back to the perp ticker, fixing spot-only tokens that previously contributed **0 notional** to a unified-account balance. Note this only helps accounts that opted into spot — with the toggle off no spot tickers exist, so the 0-notional gap remains for them. The field preference within a ticker is deliberately unchanged (`mark -> last -> index`), because this balance path is NOT gated by `enableSpotMarkets` and re-ordering would re-value collateral for users who never opted in.

### Rust merged spot markets (`enable_spot_markets`)

The perp engine's `spot_markets` mode mirrors `fetchSpotMarketsAndTickers`;
the standalone `spot` mode is unchanged. Every per-order decision reads
`market.is_spot`, never the venue mode.

- **Discovery**: `spotMetaAndAssetCtxs` runs alongside the perp read. Its
  result is held until the perp and HIP-3 catalogue is published, so spot lists
  after both, as in TS. The Unit alias goes on `base`/`pretty_symbol` only.
  Spot tickers carry no oracle or funding. A failed read logs a warning and
  retries on the discovery backoff (key `@spot`); perps never wait on it.
- **Settings and streams**: spot pairs seed 1x cross and get no
  `activeAssetData`. `setLeverage`/`setMarginType` on a spot pair are no-ops.
  `spotState` is subscribed and REST-refreshed whenever spot holdings are
  tracked (`tracks_spot_holdings`: unified/portfolio or spot enabled).
- **Balance**: standard accounts keep perp-only totals; holdings publish as
  coin balances valued by the TS `getSpotCollateralMarkPrice` ladder, written
  only when the valuation changes.
- **Orders and fills**: spot rows from `frontendOpenOrders`/`orderUpdates` are
  kept. Cancels use the spot asset id (10000 + index). A spot fill emits a fill
  event and refreshes holdings, never a position.
- **Trading**: 8 - szDecimals price decimals; never reduce-only; base price
  last, mid, mark, index.

- **Underlying token disclosure**: each merged spot pair publishes its venue
  token name and full name in the typed `UNDERLYING_TOKENS` frame section; the
  bridge applies them as TS's `info.hlTokenName`/`hlTokenFullName`, which the
  web Margin "Underlying Token" row reads (e.g. `UBTC (Unit Bitcoin)` behind
  `BTC/USDC`). This is a risk disclosure for a wrapped asset behind a familiar
  name; keep it whenever the display alias exists.
- Until the spot catalogue is committed, spot open-order rows are skipped
  rather than invalidating the snapshot, so a failing spot read never hides
  perp orders; standard accounts never value spot as collateral.

Gated by `sdk-rs/venues/hyperliquid/tests/spot_markets.rs` and the
`hyperliquid-spot` private replay (`rust:private-tape-hyperliquid --spot`).

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

**Retry on network errors, reads only**: Both `xhr` and `infoXhr` use `axios-retry` with `retries: 3` and `networkErrorOnRead(isHyperliquidRead)`: `/info` reads retry, `/exchange` actions never do, because a lost response may hide an accepted order (see `packages/safe-cex/CLAUDE.md` → "Automatic retries are for reads only").

**HIP-3 position fetch**: Sequential per-DEX to avoid rate limiting (`fetchAllHip3Positions` uses a `for` loop, not `Promise.all`).

**Order fetch per-DEX**: Sequential (`fetchOrdersInternal` iterates HIP-3 dexes in a `for` loop).

**HIP-3 market metadata**: Sequential per DEX. Official `/info` weights are 20 for `metaAndAssetCtxs`, 60 for `userRole`, and 1200 total per IP per minute. Live discovery on 2026-07-15 returned one core plus nine HIP-3 entries on mainnet; testnet's much larger catalog remains restricted to the four-DEX whitelist. Do not turn this loop back into `Promise.all`: multiple browser accounts can share one proxy egress budget.

**The budget is DERIVED from the dex count, because a fixed one is a hazard.**
`hyperliquidRestRateLimiter` paces by weight at 50ms per unit and `metaAndAssetCtxs`
is weight 20, so each dex reserves up to a second of a strictly serial timeline.
Against the flat 8 seconds this used to carry, a long enough dex list runs out
mid-loop. On 2026-09-25 `perpDexs` returned ten (`xyz flx vntl hyna km abcd cash para
mkts io`; note mainnet's is `vntl` while `TESTNET_HIP3_WHITELIST` carries `vntls`).

**This was NOT observed failing in production, and the record should say so.** It was
investigated as the cause of "HIP-3 markets not showing" and was not: a client on the
pre-change build was confirmed holding markets from dex position 9 AND position 10, so
the loop was reaching the end of the list. The reported symptom was six `*500` markets
where only two appeared, and those four are simply `isDelisted` with no `midPx` —
correctly skipped. Six of the ten dexes are entirely delisted; only xyz (109 live),
para (29), mkts (4) and io (8) carry anything. The arithmetic that predicted a timeout
came from simulating the pacer under assumed contention; real startup leaves idle gaps
that let `nextAvailableAt` drain, so the loop is far cheaper than the worst case.
Treat the budget and the partial-keep below as hazard removal, not as a shipped fix
for a live outage.

**Partial results are KEPT, and that is the difference between degraded and absent.**
The loop used to discard every completed dex when a later one ran out of budget, so
one slow dex at the end of the list could remove every HIP-3 market from the app.
`hip3DexNames` is now set to exactly the dexes that loaded, which matters beyond
the market list: it drives the HIP-3 WS subscriptions and the order/position refresh,
so listing a dex whose markets are absent (or clearing it while markets are present)
desynchronises both.

**The cap is tied to `ExchangeWorker.start()`'s 30s race.** That race covers the
whole of `start()`, so HIP-3 cannot be allowed to spend it — core perps, spot, fees,
builder fee and the sockets share the same 30s. 15s is the ceiling for that reason,
not a round number. Beyond ~13 dexes the load goes partial, which is now graceful;
an unbounded budget would convert a partial market list into a startup failure.

**A dex that FAILED is not a dex that returned nothing.** `fetchHip3Markets` returns
`null` when its own fetch dies after retry, rather than `{markets: [], tickers: []}`
— which used to make a broken dex count as successfully loaded and contribute
silence. One dex's failure no longer stops the others, and each outcome (loaded,
failed, never reached) is named in its own log line.

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
- Updates omit order type fields. Our own orders are identified by their cloid record; for frames without one, pending/pre-placement caches preserve known metadata and unknown orders trigger the bounded REST retry chain.
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

Rust FILLS revision 2 preserves `tid` as the execution event `id` for perp,
merged spot and spot-only accounts, with `oid` retained separately as `orderId`.
Live financial conventions and the existing snapshot/deduplication rules stay
the same.

### Realized PnL Tracking

The private WS tracks per-position realized PnL in `handleFills`:

- On "Open" fill with `startPosition === 0`: Reset accumulator to 0
- On "Close" fill: Increment by `closedPnl - fee` (net realized PnL)
- Applied to current position via `store.updatePosition`
- Fill ID tracking bounded to 20,000 entries (`MAX_TRACKED_REALIZED_PNL_FILL_IDS`) with FIFO eviction

## Exchange Metadata

### HIP-3 Order Type Inference

HIP-3 `orderUpdates` WS channel omits `orderType`, `triggerPx`, and `tpsl` (core perps do too — see Known Quirks 1). Our own orders are identified by cloid first; the rest of the chain is the fallback for frames without a cloid record:

```
Order type resolution chain (in priority order):
0. Cloid record     → cloid → type/price/reduceOnly/tif (written before the REST call; every market)
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

Entries expire after 30 seconds. Cleared on cancel to prevent stale matches. Consuming a pending cache entry also consumes the corresponding pre-placement entry. Entries carry the order's cloid, and when the WS handler identifies an order from its cloid record it drops exactly that entry (`removePrePlacementByCloid`) — otherwise the FIFO fallback could hand it to a different, cloid-less frame.

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

1. **HIP-3 `orderUpdates` missing fields**: `orderType`, `triggerPx`, `tpsl` are omitted (API limitation). Our own orders are identified by their cloid record instead (see "Client order ID"); everything in this entry — the tif fallback, the defer branch — is what still handles frames without one.

   **`tif` is omitted for EVERY market, core perps included** — the field is absent from the `WsOrder.order` payload entirely, so `parseHyperliquidOrderTimeInForce(orderData.tif)` never resolves on the WS path. Only REST `frontendOpenOrders` carries it. This used to be believed HIP-3-only, and the preserve-from-store fallback was gated on `isHip3`; core-perp orders therefore had `timeInForce` overwritten with `undefined` by every order update, and the UI's TIF column rendered blank (caught by the Cypress `expected '' to equal 'GTC'` assertion). Two things make the value survive now, and **both are needed**:

   - the fallback in `handleOrderUpdates` is ungated, so a stored value is preserved; and
   - `placeLimitOrder` records the placement-time TIF via `rememberLimitOrderTimeInForce(orderIds[0], …)`, because core perps get **no** optimistic store row (that is HIP-3-only) and `tick()` only re-fetches orders when HIP-3 dexes are configured — so without the placement record there is nothing in the store to preserve, and nothing ever repairs it.

   Only `orderIds[0]` is recorded: it is always the main limit wire, and any extra ids are TP/SL triggers, which have no TIF of their own. The map is bounded (500, FIFO), cleared on `dispose`, and entries are dropped on terminal order statuses.

   **The placement cache does NOT cover hosted mode, and that is why there is a
   third mechanism.** Under the hosted exchange-server, order writes run on the
   browser's main-thread mirror while `handleOrderUpdates` runs on the _server_
   instance — two different objects, so the server never sees the placement and
   its cache is always empty for browser-placed orders. Since `tick()` does not
   re-fetch core-perp orders, REST is the server's only tif source, so a resting
   limit order that still resolves to `undefined` triggers
   `scheduleOrderRefresh()` (500 ms debounce, 1 s minimum interval, so a burst of
   orders costs one fetch). It converges: REST sets the value and every later WS
   update preserves it from the store. This is another instance of the
   dual-instance trap — a fix that works in worker mode can be a complete no-op
   in hosted mode, which is exactly how the first version of this fix shipped and
   moved hyperliquid 4/11 → 5/11 without touching the TIF assertion at all.

   **Positional TP/SL cancel+replace also carries this omission on core perps.** When a positional TP/SL is placed via cancel+replace (Hyperliquid does not support amend for stops), the very first `orderUpdates` frame for the new oid arrives with `orderType`, `triggerPx`, `isTrigger`, and `tpsl` **all `undefined`** — only `limitPx`, `reduceOnly`, and `origSz: "0.0"` are populated. The correct fields only materialize on the REST `frontendOpenOrders` snapshot ~500–700 ms later. This was previously believed to be HIP-3-only; a Aug 2026 debug dump confirmed it happens on plain core perps too.

   Feeding that first WS frame through `inferHyperliquidOrderTypeAndPrice()` mis-classifies it as `type: Limit` at `limitPx`. Downstream, three amplifiers turn that into a visible SL-drag flicker: (1) the store row carries the wrong price and status; (2) the frontend's `formatOrderPricesInBounds` snaps the "limit" toward bestBid/bestAsk (a full spread away from the intended stop); (3) `OrderLineManager._claimPendingNoIdTriggerMove` gates on `isProtectiveStopOrder(candidate)` and skips the claim for `type: Limit`, so the chart spawns a phantom line beside the parked ghost line, then executes a DESTRUCTIVE `_moveOrderLineMeta` once REST arrives with the correct type — producing the "SL teleports between 3–4 positions" flicker in ~1 second.

   The `handleOrderUpdates` fix is a defer branch scoped to `reduceOnly && (!origSz || Number(origSz) === 0) && !orderType && !triggerPx && !isTrigger` on **non-HIP-3** markets. If the oid is already in the store (later WS frame after REST populated it), preserve type/price. Otherwise skip the insert entirely and call `scheduleOrderRefresh()`; the frontend-side `Updating` ghost keeps the parked line at the drag price until REST provides the authoritative first snapshot, and the pending-no-id trigger claim then fires as a RENAME (no phantom, no destructive move). `formatOrderPricesInBounds` in `apps/web/src/utils/order/getOpenOrders.ts` also skips the snap for `amount === 0 && remaining === 0` orders as a defensive belt-and-braces layer.

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
19. **Dead man's switch lifecycle is caller-owned**: `scheduleCancel` is an explicit API/worker capability, not a startup heartbeat. Callers must refresh an armed deadline themselves; safe-cex neither auto-arms nor clears it on disposal.

## Ledger (`fetchIncomeHistory`) — fills carry the fee, and three traps

The account ledger is built from `userFillsByTime` + `userFunding`. **Hyperliquid emits
no separate fee row** — the fee rides on the fill next to that fill's `closedPnl`, and
`closedPnl` EXCLUDES it (the same arithmetic `handleFills` already does as
`netRealizedPnl = closedPnl - fee`). So one fill maps to up to two `IncomeRecord`s;
mapping `closedPnl` alone drops every fee from the ledger. Mapper:
`hyperliquid.income.ts`, pure and exported.

Three traps, none of which a typecheck catches:

- **`fee` can be NEGATIVE** (maker rebate). `fetchHistoricalFills` in this directory does
  `Math.abs(feeRaw)`, which is already wrong for rebates — do not copy it into new code.
- **REST and WS funding rows have different shapes.** REST `UserFundingEntry` nests under
  `delta` (`{ time, hash, delta: { coin, usdc, … } }`); the WS `WsUserFunding` is flat.
  A mapper written against one silently yields `undefined` against the other.
- **`tranId` keys on the fill's `tid`**, never a price/size/time composite.
  `fetchHistoricalFills` dedupes on `orderId:ts:price:qty:side`, which collapses genuine
  identical partial fills.

`fetchIncomeHistory` deliberately diverges from `fetchHistoricalFills` in the same file:

|        | `fetchHistoricalFills`                                                         | `fetchIncomeHistory`                     |
| ------ | ------------------------------------------------------------------------------ | ---------------------------------------- |
| client | `xhr` (3 RPS)                                                                  | `infoXhr`, like every other private read |
| user   | `applicationId` only — **returns the master wallet's data on a vault account** | `vault \|\| applicationId`               |
| range  | 30-day floor, 7-day chunks                                                     | neither                                  |

Those 30-day/7-day constants are **ours**, bybit-shaped, with no upstream citation.
Hyperliquid is address-queryable with no retention ceiling — the entire reason this venue
matters for a journal — and bounds a response by ROW COUNT, not window width. So the
pager walks `userFillsByTime` forward until a page comes back short, with an **exclusive**
cursor (`newest + 1`): `startTime` is inclusive, so reusing the boundary re-returns rows,
and relying on the `tid` dedupe to absorb that makes any raw-vs-mapped comparison
double-count. That asymmetry has already produced one false FAIL in this repo.

**Two known gaps, stated rather than hidden:**

- Deposits, withdrawals and liquidations live in `userNonFundingLedgerUpdates`, which this
  does not read — so `transfer` and `liquidation` are always empty here and the full
  equity identity cannot close. That is precisely what distinguishes a losing trade from
  a withdrawal.
- **HIP-3 scope is UNVERIFIED.** Every other private read in this file is per-dex, and
  ws-private subscribes `userFills` _with_ a `dex` param, but `fetchIncomeHistory` makes a
  single non-dex call. If REST `userFillsByTime` is dex-scoped, HIP-3 income is silently
  absent. Do not assume the ledger covers HIP-3 until someone checks against an address
  with HIP-3 fills (their coins are formatted `dex:ASSET`, so presence is detectable).

**What the ledger audit run proves, and what it does not.** `yarn history-audit --ledger
--targets hyperliquid:50:30` reported 378 raw rows to 463 records, residual 0, zero
uncategorised. Its raw side computes `closedPnl - fee` from the same two fields the mapper
splits, so a PASS demonstrates **row coverage, parsing and paging** — no row dropped, none
left uncategorised. It does **not** independently validate the choice of those fields, and
it cannot see either gap above, because a truncated or dex-scoped response shrinks both
sides together and the residual stays 0. Hyperliquid has no second axis to cross-check
against (`--closed-pnl` needs one), and synthesising `fetchClosedPnl` from these same fills
would be a fake oracle that always agrees.

## Order history (`historicalOrders`) — one row per TRANSITION, and no time filter

`fetchHistoricalOrders` reads `POST /info { type: "historicalOrders", user }`. Mappers:
`hyperliquid.order-history.ts` (`collapseHyperliquidOrderHistory` +
`mapHyperliquidHistoricalOrder`), both pure and exported.

Measured live (sheet row 50): **2000 rows carrying 1020 distinct `oid`s**, of which only
315 filled. Over a 7-day window that is 490 orders with **63% never filled** — the largest
blind spot of any axis on this venue, since a cancelled order appears in no fill, no
execution and no closed position.

Four properties, every one of them measured, and each silently wrong if assumed:

- **One row per status TRANSITION, not per order.** The surplus 980 rows are the `open`
  state of orders that later filled or cancelled; the sampled `open` and `canceled` rows
  were byte-identical apart from `status`/`statusTimestamp`. Collapse before mapping.
  Note this is the **inverse** of the okx and bybit traps in this epic, where an id dedupe
  destroyed real rows — which is exactly why it must be measured per venue, never
  generalised in either direction.
- **`statusTimestamp` alone cannot order those two rows.** An order that fills on
  placement carries the SAME `statusTimestamp` on both, so breaking the tie by array
  position let `open` win and the axis returned **zero filled orders out of 490** while the
  raw snapshot held 315. The conformance check passed straight through it — ids, counts and
  malformed were all correct — and only the status census exposed it. A terminal state now
  outranks `open` at an equal timestamp, which is also order-independent.
- **The time window is IGNORED.** Passing `startTime`/`endTime` returns a byte-identical
  response, so `from`/`to` are applied client-side and `paginateHyperliquidByTime` would
  degenerate to one call. The **2000-row ceiling is a retention limit, not a page size** —
  there is no cursor past it, so unlike every other private read on this venue, order
  history is NOT address-queryable-complete.
  Rust emits `history.incomplete` with the actual query command before returning
  this retained history, so the host flags the owning array and routes its
  warning only to that caller. Transition collapse, time/limit filtering and
  strict history failures retain their existing behavior.
- **`sz` is REMAINING, `origSz` is original.** A `filled` row carries `sz: "0.0"`; a
  cancelled one carries `sz === origSz`. Reading `sz` as the amount reports every completed
  order as size zero.

`side` is `A`/`B` (ask/bid), not buy/sell. There is no average fill price on this endpoint,
so `averagePrice` is left absent rather than guessed from `limitPx`.

**HIP-3 scope is UNVERIFIED**, the same open question as the ledger: this is a single
non-dex call, while every other private read here is per-dex. HIP-3 coins are formatted
`dex:ASSET`, so presence is detectable on an address that trades them.

## A bad key is invisible on reads — check the pair at startup

Hyperliquid is a DEX: `/info` takes the wallet address as a **parameter** and is
unauthenticated. So a saved account whose private key no longer matches its API
wallet loads looking completely healthy — balance, positions and orders all
populate off `applicationId` — and the first sign of trouble is a rejected order.
There is no auth error to surface, because nothing authenticated ever happened.

`validateAccount` does check the key/wallet pair, but it only runs on the
**add-key form**; an account restored from storage never goes through it. That is
the gap `warnOnMismatchedApiWallet` closes, called from `start()`.

- It is **non-fatal**. Public and read-only use still work, and the pair is
  legitimately absent for wallet-based connections.
- It compares the private key to the saved signing/API wallet first (`key`),
  falling back to `vault || applicationId` only when no signing wallet is saved.
  Agent-wallet accounts deliberately have `key` = agent wallet and
  `applicationId` = master wallet; validating the agent private key against the
  master wallet falsely flags valid accounts. The bad-key e2e keeps
  `applicationId` unchanged while corrupting `key` and `secret`, so the
  diagnostic must catch a broken signing pair even when private reads still
  look healthy.
- It **catches**, rather than propagates, the ethers error. A malformed secret
  makes `validateWalletAndPrivateKey` throw instead of returning false, and an
  unguarded call took the whole of `start()` down with it — caught by the
  existing startup sequencing suite, which uses `'0xsecret'` as a fixture.
- The wording has to stay auth-shaped (`invalid`, `credentials`, `API wallet`).
  The e2e bad-api-key spec greps the requests log against a pattern list; a
  warning it cannot recognise is worth the same as no warning.
- In hosted exchange-server mode the server-side `start()` copy has
  `secret: ''`, so this safe-cex startup check cannot see restored browser
  secrets. The web controller repeats the same signing-wallet check after the
  browser mirror is active and emits a forced `'error'` from that mirror so
  the request log and toast assertions still get an auth-shaped signal.

## Hosted mode: L1 actions are signed by the browser, not the exchange-server

Hyperliquid signs in the **method body** (`signL1Action(createWalletFromPrivateKey(this.options.secret), …)`), not in an axios interceptor. The hosted exchange-server runs with `secret: ''` by design, and `createWalletFromPrivateKey('')` **throws** — so every worker-reachable signed method died before issuing a request. `useMarginType` swallows the throw into a toast, so the symptom was silent: **hosted users could not change margin type or leverage at all**, and the e2e log showed zero `updateLeverage` actions rather than a failed one.

Order placement was never affected, which is what made this easy to miss: order writes run on the browser's main-thread mirror, while `callMethodInWorker` (`setLeverage`, `changeMarginType`, `scheduleCancel`) runs on the server.

The fix reuses the machinery that already exists rather than adding a signer type. `POST /exchange` is **already** relayed to the browser by `setInnerAdapterForRemoteSigning`, and `BaseExchange.signHttpRequest` replays it through the browser's own axios instance — so a request interceptor is enough:

- **Server**: `signActionOrDefer` returns `undefined` when `options.secret` is empty, and the payload omits `signature`.
- **Browser**: `addSigningInterceptor` (`hyperliquid.api.ts`) fills it in on the replay, keyed off a body carrying `action` + `nonce` and no `signature`.
- Locally-signed requests (browser-placed orders) already carry a signature, so the interceptor skips them — no double-signing.

Two things are deliberate:

- **The action type is allowlisted.** The relay lets the server ask the browser to sign whatever it sends, so `usdSend` / `withdraw3` / `vaultTransfer` are excluded — a transferable signature for those moves funds. Adding a new signed action means adding it to `RELAY_SIGNABLE_ACTION_TYPES`.
- **This is a two-target deploy.** The defer half ships in the exchange-server, the signing half in the web bundle.

**Known remaining gap: `hyperliquidspot`'s own signed methods are NOT fixed.** Spot builds its clients from the same `createExchangeAPI` (it imports it as `createAPI`), so the interceptor **is already installed there** — it is simply inert, because every spot body carries an inline signature. Spot's `setLeverage` / `changeMarginType` still sign in the method body and therefore still throw in hosted mode. Completing it is not just adding `signActionOrDefer`: those calls pass four arguments to `signL1Action`, leaving `isMainnet` at its `true` default, while the shared interceptor derives it from `!options.testnet`. On mainnet the two agree; on testnet they do not. Reconcile the `isMainnet` argument first. Spot is `hiddenFromNewAccountPicker` and outside the e2e matrix, which is why this was left rather than half-changed.

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
- **Terminal cancel misses are not user errors.** After a full reduce-only market close, Hyperliquid auto-drops positional TP/SL. The follow-up `cancel` then returns `Order was never placed, already canceled, or filled. asset=N` once per oid (BTC is `asset=0`). `extractHyperliquidResponseError` unique-dedupes identical status strings; `isOnlyHyperliquidTerminalCancelError` matches a cancel/`cancelByCloid` batch that is _only_ that miss. The interceptor still throws so callers can distinguish it from a 2xx, but it does **not** `emitAndLogError` (no toast). `cancelOrders` treats it as success and still suppresses/removes the local rows. Any other error in the same batch still toasts and throws.

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

## `Execution` position fields — `dir` is NOT a hedge leg

`fetchHistoricalFills` populates `realizedPnl` (from `closedPnl`, gross of fees) and `maker`
(from `!crossed`), but deliberately leaves `positionSide` **absent**.

Hyperliquid is one-way only (`positionModes: [PositionMode.OneWay]`,
`hidePositionModeSelector: true`). Its `dir` field reads `"Open Long"` / `"Close Long"`,
which describes what the fill DID, not which hedge leg it belongs to. Mapping it splits
one continuous position across `long` / `short` buckets that a one-way consumer never
seeds — `processTrades` only fills `positions.both` when `isHedged` is false — so every
fill would land in an empty bucket and the position would never reconcile.

`closedPnl` excludes the fee, which rides on the same fill (see the ledger section
above); do not net it a second time.

## Client order ID

**Every order we place carries a cloid** — `0x` + 32 lowercase hex chars, wire
field `c` (`hyperliquid.cloid.ts`). That includes market orders, every wire of a
`normalTpsl` bracket (the entry and each TP/SL leg get their own, so a bracket
carries up to three), standalone and positional TP/SL, the cancel+replace in
`updateOrder`, and `hyperliquidspot`. The minting happens in `placeOrder`, where
the wire is built, so the hosted exchange-server path is covered too: order
writes run on the browser's main-thread mirror, and signing-capture
(`isSigningCaptureRequest`) is untouched. `c` is the LAST key of the wire, after
`t` — msgpack key order is part of the signed hash.

**Caller IDs are honored on every shape** (`hyperliquidClientOrderIdRule`): the
caller's cloid goes on the main wire verbatim, legs get minted ones. Only a
lowercase cloid is accepted, because the venue echoes cloids lowercase and an
uppercase one could never be matched. `mintClientOrderId` therefore returns a
cloid for stops too, which is what lets the web stamp a stop's optimistic ghost
with the ID of the order it becomes.

**Verified live 2026-10-11:** `orderUpdates` frames carry `order.cloid` (open
and canceled frames for resting limits; trigger orders were not observed, the
docs list `cloid` on every `WsBasicOrder`), and `frontendOpenOrders` rows carry
`cloid` (`null` when the order has none). `parseOpenOrders` maps it to
`Order.clientOrderId` — a top-level field, because `info` is stripped before the
store worker posts to the main thread.

**`updateOrder` resolves to `ReplacementOrderIds`**: the new oids, plus the
replacement's freshly minted cloid as `clientOrderId`. Callers reading
`result[0]` are unaffected. The cancelled order's cloid is never reused.

**Lookup** (`findOrderByClientOrderId`) asks `orderStatus` with the cloid as
`oid`, which also answers for filled and cancelled orders and, unlike
`fetchOrders` here, never writes the store. A miss is still never definitive.

**The WS handler reads the record first.** `handleOrderUpdates` looks the
frame's `cloid` up (`getPlacementRecord`) before any inference. On a hit it
builds the order from the record — type, trigger price as `price`,
reduce-only, tif — plus the frame's live `sz`/`origSz`, stamps
`clientOrderId`, and skips the pending cache, the pre-placement matchers, the
reduce-only deferral and the tif-driven REST refresh. A terminal frame drops
the record. Records live 30 minutes (`PLACEMENT_RECORD_TTL_MS`, capped at 500):
the first frame arrives within a second, so the TTL only covers a slow or
reconnecting socket; after it a later frame finds the order in the store.

**The record does not reach the hosted exchange-server.** It is written where
the order is placed (the browser's main-thread mirror) and shared through
sessionStorage + BroadcastChannel, which the server cannot see; its
`handleOrderUpdates` still runs the fallback chain. Same for orders placed from
another client. Frames with a cloid but no record still get `clientOrderId`
stamped, so the web's exact match works for them once REST/WS delivers.

**Cancel** still goes by oid; `cancelByCloid` exists but nothing needs it yet.

Rust unified/portfolio startup caches a native spot collateral snapshot when a
nonstable conversion quote or the initial core exposure baseline is unavailable.
It retains the prior USD aggregate; cold private accounts publish no synthetic
zero balance. Valid pending live snapshots fence older REST reads immediately.
A real live/shared/context quote or the first core snapshot rechecks that native
collateral and derives wallet principal using current proven position PnL.
Ordinary successful private snapshots keep the existing valuation cadence.
