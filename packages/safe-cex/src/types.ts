// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./env.d.ts" />
import { OrderStatus } from './generated/OrderStatus';
import { OrderSide } from './generated/OrderSide';
import { OrderType } from './generated/OrderType';
import type { Order } from './generated/Order';
import type { AccountReconnectDiagnostics } from '@tealstreet/http-over-websocket/src/connectionDiagnostics';
import type { LocalSignerMetadata } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeErrorKind } from './commandOutcome';
import type { RustPublicFeedPolicy } from './exchanges/rust/publicFeed';
import type { Market } from './generated/Market';
import type { OptionRight } from './generated/OptionRight';
import type { OptionTerms } from './generated/OptionTerms';
import type { OptionGreeks } from './generated/OptionGreeks';
import type { Position } from './generated/Position';
import type { Ticker } from './generated/Ticker';
import type { TransportProvider } from './transport/types';
import type { ChaserOrigin } from './webworker/chaserOrder/ChaserOrderOptions';
import type { TwapInstance } from './webworker/twap/TwapInstance';
import type { TwapOptions } from './webworker/twap/TwapOptions';
import type { TwapState } from './webworker/twap/TwapState';

import { MessageHandlersStats } from './exchanges/types';
import { MarginType } from './generated/MarginType';
import { PositionMode } from './generated/PositionMode';
import { PositionSide } from './generated/PositionSide';

export type { Position, OptionGreeks, Market, OptionRight, OptionTerms, Ticker };
export type { OrderFillEvent } from './generated/OrderFillEvent';
export type { Execution } from './generated/Execution';
export type { PublicTrade } from './generated/PublicTrade';
export { PositionSide, PositionMode, MarginType };

export type ExchangeName =
  | 'apexomni'
  | 'asterdex'
  | 'backpack'
  | 'binance'
  | 'binanceinverse'
  | 'binanceoptions'
  | 'binancespot'
  | 'binancepm'
  | 'bingx'
  | 'bitget'
  | 'bitgetinverse'
  | 'bitgetusdc'
  | 'bitrue'
  | 'bitunix'
  | 'blofin'
  | 'bybit'
  | 'bybitdemo'
  | 'bybitinverse'
  | 'bybitspot'
  | 'hyrotrader'
  | 'coinbase'
  | 'coinbasespot'
  | 'cryptocom'
  | 'grvt'
  | 'hyperliquid'
  | 'hyperliquidspot'
  | 'lighter'
  | 'lighter-robinhood'
  | 'mexc'
  | 'okx'
  | 'okxdemo'
  | 'okxeu'
  | 'okxeudemo'
  | 'paradex'
  | 'phemex'
  | 'polymarket'
  | 'kodiakfi'
  | 'mode'
  | 'txflow'
  | 'what'
  | 'weex'
  | 'woo'
  | 'woofi'
  | 'kucoin'
  | 'kucoinspot'
  | 'kraken'
  | 'krakenspot';

export type ExchangeOptions = {
  readonly key: string;
  readonly secret: string;
  readonly passphrase?: string;
  /** Rust local-provider setup only; consumed before generic options are retained.
   * Stable IDs are caller-assigned, never derived from credential material. */
  readonly localSigner?: LocalSignerMetadata;

  readonly secondary_key?: string;
  readonly secondary_secret?: string;
  readonly secondary_passphrase?: string;

  // Proxy URL for the secondary (read-only) key's transport. Lets reads route
  // through a separate cheap fleet while writes stay on the primary transport.
  // Undefined = secondary client keeps the primary/default transport.
  readonly secondaryProxyUrl?: string;

  // AsterDEX Web3 authentication
  readonly auth?: {
    userAddress?: string;
    signerAddress?: string;
    privateKey?: string;
  };

  // Paradex Ethereum authentication (private key stored in key field)
  readonly paradexAddress?: string;
  readonly subkeyAddress?: string;

  // GRVT authentication (Ethereum address for order signing, must be tagged to API key)
  readonly grvtSignerAddress?: string;
  readonly grvtTradingAccountId?: string;

  /** Polymarket CLOB V2; `key` is the local signing wallet's private key. */
  readonly polymarket?: {
    /** Discover only these Gamma market slugs; omitted means the full universe. */
    readonly marketSlugs?: string[];
    /** Supply all three L2 credentials together, or derive them with L1 auth. */
    readonly apiKey?: string;
    readonly apiSecret?: string;
    readonly apiPassphrase?: string;
    /** EOA, proxy, Safe, or ERC-1271 deposit wallet signing. Defaults to EOA. */
    readonly signerType?: 0 | 1 | 2 | 3;
    /** Deposit wallet/maker address; required for non-EOA signature types. */
    readonly funderAddress?: string;
  };

  readonly starkPrivateKey?: string;
  readonly vault?: string;
  readonly applicationId?: string;
  readonly apiKeyIndex?: string;
  /**
   * Which Hyperliquid builder wallet this account routes through, and the rate
   * it charges — `feeTenthsBps` is tenths of a basis point, so 10 is 1bp.
   *
   * Absent means the historical default (`getBuilderAddress` +
   * `TEALSTREET_BUILDER_FEE`), which is what every caller took before this
   * existed. Resolve it through `resolveHyperliquidBuilder` rather than reading
   * the field, so the fallback is defined in exactly one place.
   *
   * **The address and the rate travel together and must never be mixed.** A
   * user approves a specific (builder, max rate) pair, so sending one builder's
   * address with another's rate is an order Hyperliquid rejects.
   */
  readonly hyperliquidBuilder?: {
    readonly address: string;
    readonly feeTenthsBps: number;
  };
  /** Extra fast HL perp depth topic per book. Default off; budget all sockets against the per-IP cap. */
  readonly hyperliquidFastBook?: boolean;
  readonly testnet?: boolean;
  /** See `ExchangeAccountConfig.enableSpotMarkets`. Default off. */
  readonly enableSpotMarkets?: boolean;
  /** See `ExchangeAccountConfig.enableOptionMarkets`. Default off. */
  readonly enableOptionMarkets?: boolean;
  /** See `ExchangeAccountConfig.enableInverseMarkets`. Default off. */
  readonly enableInverseMarkets?: boolean;
  /** Bybit Brazil accounts require this site id on REST and WS upgrade requests. */
  readonly bybitSiteId?: 'BRA_BTL';
  readonly publicDataOnly?: boolean;
  /** The host names a refused placement itself, so the exchange does not also report it. */
  readonly callerReportsPlacementFailures?: boolean;
  /**
   * A main-thread instance paired with a worker that owns the account. Its reads
   * and trading go to the worker, waiting for the bridge; it never runs an engine.
   */
  readonly workerMirror?: boolean;
  /** Retained input compatibility; this build always uses Rust for beta venues. */
  readonly engine?: 'rust';
  readonly privateOnly?: boolean;
  /** Rust live public-feed ownership. Shared ports must be explicitly transferred to workers. */
  readonly publicFeed?: RustPublicFeedPolicy;
  readonly tealstreetProxy?: boolean;
  // tunnel (http over websocket)
  readonly tealstreetTunnel?: boolean;
  readonly tealstreetWsRoute?: boolean;
  readonly overrideProxyUrl?: string;
  readonly proxyRegion?: string;
  readonly omniKeySeed?: string;
  readonly subaccount?: string;
  readonly orderlyExchangeId?: string;
  readonly tuning?: {
    readonly activeTickerSymbols?: string[];
    readonly calculateUpnlFromRealtimeLastPrice?: boolean;
    readonly dataUpdateThrottleMultiplier?: number;
    readonly skipRendersWhenWindowIsHidden?: boolean;
    readonly consoleLogging?: boolean;
  };
  readonly extra?: {
    /** `Infinity` turns periodic REST polling off, for callers that fetch on demand. */
    tickInterval?: number;
    /** Rust command waiting deadline in ms (default 120000); expiry never retries or cancels a write. */
    rustCommandTimeoutMs?: number;
    recvWindow?: number;
    // Optional AsterDEX REST API version override.
    asterdexApiVersion?: 'v1' | 'v3';
    // Optional Aster Code builder attribution overrides.
    // Aster accepts one feeRate per order; maker/taker fields are Tealstreet placement policy.
    asterBuilderAddress?: string;
    asterBuilderFeeRateMaker?: string;
    asterBuilderFeeRateTaker?: string;
    // Lighter builder/integrator fee mode.
    lighterBuilderFeeMode?: 'dynamic' | 'static';
    // Optional override for Lighter integrator account index.
    lighterIntegratorAccountIndex?: string;
    // Lighter TP/SL allowed execution slippage, as a percentage from trigger price.
    lighterStopSlippagePct?: number;
    // HyroTrader uses the same connector for the shared simulator demo and real user-pasted challenge keys.
    hyrotraderMode?: 'demo' | 'production';
    /**
     * KuCoin broker attribution config.
     *
     * Browser builds may supply this from NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_*
     * or NEXT_PUBLIC_KUCOIN_SPOT_BROKER_* env vars when product policy permits
     * sending signed broker attribution from the client runtime.
     */
    kucoinBroker?: {
      readonly partner: string;
      readonly brokerKey: string;
      readonly brokerName: string;
      readonly verify?: boolean;
    };
  };

  /** Delegates request signing to a signer outside this exchange runtime. */
  readonly remoteSigner?: RemoteSigner;

  /**
   * Overrides the runtime's registered transport (`setDefaultTransport`) for this
   * in-process instance. Never set it where options cross a worker or wire boundary:
   * a function does not survive structuredClone, and exchange-server drops it.
   */
  readonly transport?: TransportProvider;

  /**
   * Stands in for `secret`: receives the exact bytes the exchange would HMAC and
   * returns the signature value, plus any headers to add to the same request.
   * Bybit and Binance: REST only, so the private websocket is not connected.
   * Paradex: signs the auth and order typed data, and `key` is the subkey public key.
   */
  readonly payloadSigner?: PayloadSigner;

  /**
   * When true, exchange-independent post-processors run client-side instead of server-side.
   * Only exchange-dependent post-processors (MessageHandlerStats, ChaserOrder) run on the server.
   */
  readonly hostedMode?: boolean;

  /**
   * External public data provider configuration.
   * When set, the exchange does not create its own public WebSocket connection.
   * Instead, it receives public data from a gateway or another in-process exchange.
   * When undefined, the exchange uses its direct public-data connection.
   */
  readonly externalPublicData?:
    | {
        /**
         * Type of external data provider
         * - 'websocket': Connect to a WebSocket gateway that fans out public data
         */
        type: 'websocket';

        /**
         * WebSocket URL of the public data gateway
         * Example: 'ws://public-data-gateway:8080'
         */
        url: string;

        /**
         * Exchange identifier used by the gateway to route messages
         * Example: 'bybit', 'binance'
         */
        exchangeId: string;

        /**
         * Optional authentication token for the gateway connection
         */
        authToken?: string;
      }
    | {
        /**
         * Type of external data provider
         * - 'in-process': Use another Exchange instance in the same process as the data source.
         *   The source exchange's store updates (tickers/markets) are forwarded to this exchange.
         *   Orderbook and candle subscriptions are delegated to the source's listen* methods.
         *
         * This enables sharing a single public-data exchange connection across N authenticated
         * instances (e.g., 100 Bybit accounts sharing one public WS connection).
         */
        type: 'in-process';

        /**
         * A running Exchange instance that provides public data.
         * Typically created via createPublicExchange() and started before being passed here.
         */
        source: PublicDataSource;
      };
};

/**
 * Exchange account configuration - used across web, CLI, telegram, etc.
 * This represents a user's saved exchange account with credentials.
 */
// Result of validating an account's credentials. `error` is '' when the keys are
// valid (preserving the old "empty string = ok" semantics). `balance` is the Balance
// the validation call fetched in the same request (read `.total` for the USD value),
// so callers get key-validity AND balance from one round-trip. Undefined for venues
// whose validation doesn't fetch balance.
/** What a nuke stopped before cancelling and closing: chaser and TWAP ids. */
export interface NukeResult {
  stoppedChasers: string[];
  stoppedTwaps: string[];
  /** Set where the venue's cancels can miss orders placed seconds ago: the user should check. */
  warning?: string;
}

export interface ValidateAccountResult {
  error: string;
  balance?: Balance;
  /** Typed reason when the engine classified the failure (Rust venues), e.g. 'authInvalid'. */
  kind?: ExchangeErrorKind;
}

export interface ExchangeAccountConfig {
  exchange: ExchangeName;
  name: string;
  testnet?: boolean;

  // API credentials (also used for private keys on wallet-based exchanges)
  apiKey: string;
  apiSecret: string;
  passphrase?: string;

  // Secondary API keys (for exchanges that support multiple keys)
  secondary_apiKey?: string;
  secondary_apiSecret?: string;
  secondary_passphrase?: string;

  /**
   * Opt this account into the exchange's spot markets, for venues that serve
   * spot and derivatives from one connection (`supportsSpotToggle` metadata).
   *
   * Default-off is deliberate: a perp trader's market list and request volume
   * must be unchanged unless they explicitly ask for spot.
   */
  enableSpotMarkets?: boolean;

  /**
   * Opt this account into the exchange's option markets, for venues that trade options
   * in the same account as their derivatives (`supportsOptionsToggle` metadata).
   *
   * Default off, for the same reason as `enableSpotMarkets`: a perp trader's market list
   * and request volume must be unchanged unless they ask for options.
   */
  enableOptionMarkets?: boolean;

  /**
   * Opt this account into the exchange's inverse (coin-margined) markets, for venues that trade them in the same
   * account as their linear derivatives (`supportsInverseToggle` metadata). Default off, as for options.
   */
  enableInverseMarkets?: boolean;

  /**
   * Bybit Brazil accounts use the global Bybit hosts but require
   * `x-site-id: BRA_BTL` on REST and WebSocket connection requests.
   */
  bybitSiteId?: 'BRA_BTL';

  // Exchange-specific authentication fields
  applicationId?: string;
  apiKeyIndex?: string;
  vault?: string;
  omniKeySeed?: string;
  subaccount?: string;
  orderlyExchangeId?: string;

  // Paradex Ethereum authentication (private key stored in apiKey field)
  paradexAddress?: string;
  subkeyAddress?: string;

  // GRVT authentication (Ethereum address for order signing, must be tagged to API key)
  grvtSignerAddress?: string;
  grvtTradingAccountId?: string;

  // Proxy settings
  tealstreetProxy?: boolean;
  tealstreetTunnel?: boolean;
  tealstreetWsRoute?: boolean;
  proxyRegion?: string;

  /**
   * True only for the bundled shared /hyrotrader-demo account. Real HyroTrader
   * accounts use user-pasted challenge API keys and should behave as live
   * accounts even though the connector reuses simulator-compatible endpoints.
   */
  hyrotraderDemo?: boolean;

  /** When set, and the Tealstreet browser extension is present, this account
   *  connects directly to the exchange (no proxy/tunnel) — the extension's
   *  declarativeNetRequest rules handle CORS/header injection. Resolved
   *  web-side only; ignored when the extension is absent (proxy stays the
   *  fallback). */
  useExtensionDirect?: boolean;

  // UI/metadata fields (optional, not used for trading)
  user?: string;
  color?: string;
  active?: boolean;
  pinned?: boolean;
  telegramBotToken?: string;
  lastSymbol?: string;

  /** Runtime health status — set by error classification, auto-clears on successful connect */
  /**
   * `startup_failed` is not an auth verdict: the exchange's `start()` threw, so
   * the account is alive with no sockets and the topic watchdog reloads it on a
   * loop. It exists so that failure has a visible name instead of a silent
   * reconnect spinner. Device-local (see `DEVICE_LOCAL_ACCOUNT_FIELDS`).
   */
  healthStatus?: 'ok' | 'auth_error' | 'auth_expired' | 'permissions_error' | 'startup_failed';

  /** Last write timestamp (ms since epoch). Used for last-write-wins
   *  across CLI ↔ web account sync. Missing → treated as 0 for LWW. */
  updatedAt?: number;
  /** Tombstone marker for sync deletes. Live accounts have undefined. */
  deletedAt?: number;
  /** Stable identity minted at creation, carried across renames and key
   *  rotations. Merge still keys on `name`; this is what tells a rotated
   *  key (same id, plain last-write-wins) from two devices that
   *  independently created different accounts under one name (different
   *  id, both worth keeping). Missing on pre-sync rows — see
   *  `apps/web/src/utils/accountSync/accountFields.ts`. */
  syncId?: string;
}

export type PublicExchangeOptions = Omit<ExchangeOptions, 'key' | 'secret'> & {
  readonly key?: string;
  readonly secret?: string;
  readonly publicDataOnly: true;
  /**
   * Force public exchange construction to turn off all Tealstreet proxy/tunnel
   * routing, even when metadata or caller options would enable it. Server-side
   * public market-data services use this when direct exchange REST is expected
   * and proxy auth/CORS behavior would be wrong.
   */
  readonly forceAllProxiesOff?: boolean;
};

import type { Balance } from './generated/Balance';
export type { Balance } from './generated/Balance';
export type { CoinBalance } from './generated/CoinBalance';

/** One option's prices and implied volatilities. IVs are fractions; 0 means unavailable. */
export type OptionQuote = {
  readonly symbol: string;
  readonly bid: number;
  readonly ask: number;
  readonly mark: number;
  readonly markIv: number;
  readonly bidIv: number;
  readonly askIv: number;
  readonly underlyingPrice: number;
  readonly forwardPrice?: number;
  readonly greeks?: OptionGreeks;
};

export type OptionChainQuery = {
  readonly underlying: string;
  readonly expiry?: number;
  readonly right?: OptionRight;
  readonly strikeMin?: number;
  readonly strikeMax?: number;
};

export type OptionChainRow = {
  readonly market: Market;
  readonly quote: OptionQuote;
};

export type AccountGreeks = {
  readonly underlying: string;
  readonly delta: number;
  readonly gamma: number;
  readonly vega: number;
  readonly theta: number;
  readonly units: string;
};

/** [last, mark, index] source receipt-clock milliseconds; not seconds; not snapshot receipt time. Null is unobserved; zero is valid. */
export type TickerObservedAt = NonNullable<Ticker['observedAt']>;

export { OrderSide } from './generated/OrderSide';
export { OrderStatus } from './generated/OrderStatus';
export { OrderType } from './generated/OrderType';
export { OrderTimeInForce } from './generated/OrderTimeInForce';
export { OrderStopTriggerType } from './generated/OrderStopTriggerType';
export { OrderRejectReason } from './generated/OrderRejectReason';
export type { Order } from './generated/Order';
export type { PlaceOrderOpts } from './generated/PlaceOrderOpts';
export type { UpdateOrderOpts } from './generated/UpdateOrderOpts';

export type DisplayedPosition = Position & {
  readonly account: string;
  readonly ticker?: Ticker;
  readonly market?: Market;
  readonly balance?: Balance;
  readonly exchangeName?: string;
  readonly oneMinUpnl?: number;
  readonly fiveMinUpnl?: number;
  readonly hasActiveChaser?: boolean;
  readonly hasActiveTWAP?: boolean;
  readonly isFakePosition?: boolean;
  readonly chaserSize?: number;
  readonly hasFullStop?: boolean;
  readonly hasFullTP?: boolean;
};

export type HistoricalTrade = {
  readonly id?: string;
  readonly symbol: string;
  readonly orderId: string;
  readonly price: number;
  readonly qty: number;
  readonly quoteQty?: number;
  readonly realizedPnl?: number;
  readonly positionSide: PositionSide | 'both';
  readonly side: OrderSide;
  readonly buyer?: boolean;
  readonly maker?: boolean;
  readonly commission?: number;
  readonly commissionAsset?: string;
  readonly time: number;
};

export type HistoricalTradeOptions = {
  readonly symbol: string;
  readonly orderId?: string;
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
  readonly fromId?: number;
  readonly accountName?: string;
};

export type SecondsTimeframe = '1s' | '5s' | '15s' | '30s';
export type Timeframe = '1m' | '3m' | '5m' | '15m' | '30m' | '1h' | '2h' | '4h' | '6h' | '12h' | '1d' | '1w';
export type ExtendedTimeframe = '3h' | '5h' | '7h' | '8h' | '9h' | '10h' | '11h' | '24h' | '2d' | '3d' | '1M';
export type AnyTimeframe = SecondsTimeframe | Timeframe | ExtendedTimeframe;
export const timeframeToSeconds: { [key in Timeframe | SecondsTimeframe | ExtendedTimeframe]: number } = {
  '1s': 1,
  '5s': 5,
  '15s': 15,
  '30s': 30,
  '1m': 60,
  '3m': 180,
  '5m': 300,
  '15m': 900,
  '30m': 1800,
  '1h': 3600,
  '2h': 7200,
  '3h': 10800,
  '4h': 14400,
  '5h': 18000,
  '6h': 21600,
  '7h': 25200,
  '8h': 28800,
  '9h': 32400,
  '10h': 36000,
  '11h': 39600,
  '12h': 43200,
  '24h': 86400,
  '1d': 86400,
  '2d': 172800,
  '3d': 259200,
  '1w': 604800,
  '1M': 2592000, // 30 days
};

export type Candle = {
  readonly timestamp: number;
  readonly open: number;
  readonly high: number;
  readonly low: number;
  readonly close: number;
  readonly volume: number;
  readonly lastSide?: OrderSide;
};

export type OHLCVOptions = {
  readonly symbol: string;
  readonly interval: Timeframe;
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
};

export type ExecutionsOptions = {
  readonly symbol?: string;
  readonly orderId?: string;
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
};

export type OHLCVSOptions = {
  readonly symbols: string[];
  readonly interval: Timeframe;
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
};

/**
 * One execution. `amount` is what THIS execution filled — never the order's
 * running total, which consumers would add up again (the chaser, provisional
 * positions). Venues reporting a cumulative total convert with
 * `utils/incrementalFills`.
 */
export { LogSeverity } from '@tealstreet/logging/src/LogSeverity';

export type OrderBookOrders = {
  price: number;
  amount: number;
  total: number;
};

export type OrderBook = {
  bids: OrderBookOrders[];
  asks: OrderBookOrders[];
};

export type StoreDataLoaded = {
  readonly balance: boolean;
  readonly orders: boolean;
  readonly markets: boolean;
  readonly tickers: boolean;
  readonly positions: boolean;
};

export type StoreOptions = {
  //TODO: deprecate for positionMode
  readonly isHedged: boolean;
};

export type ActiveChaser = {
  id: string;
  symbol: string;
  size: number;
  side: OrderSide;
  maxAttempts: number;
  reduceOnly?: boolean;
  attempts: number;
  isStopped: boolean;
  stopReason: string;
  currentOrderIds?: string[];
  origin?: ChaserOrigin;
  /** Set on a TWAP's slice chase: the TWAP it belongs to. */
  twapId?: string;
};

export type OrderRiskProfile = {
  isLookingLikeAFullStop?: boolean;
  orderPnl: number;
  totalRealizedPnl: number;
  rollingUnrealizedPnl: number;
  orderLinePosSide: PositionSide;
  positionSize: number;
  orderLineText?: string;
  fee?: number;
};

export type ActiveTWAP = TwapOptions & TwapState & Pick<TwapInstance, 'id'> & { symbol: string };

export type FetchedLeverage = {
  buyLeverage: number;
  sellLeverage: number;
  marginType?: MarginType;
  positionMode?: PositionMode;
  isFetching?: boolean;
};

/**
 * Where a number came from, and therefore what it can be used for.
 *
 * This rides ON the data rather than living in a capability table, because derived data
 * is INDISTINGUISHABLE from reported data at the call site — a fold of fills does not
 * look derived, it looks like a closed position. A lookup table someone has to remember
 * to consult is the same shape as `supportsTradesHistory`, which advertises `true` on
 * three venues that always return empty and has no production consumer at all.
 *
 * - `venue`   — the exchange reported this directly. Low multiplicity, error is local to
 *               the row, and it can serve as an INDEPENDENT check on another axis.
 * - `derived` — folded client-side from another axis (e.g. closed positions from fills).
 *               Error is PATH-DEPENDENT: one bad fill corrupts every position after it,
 *               and it cannot validate the axis it was folded from — comparing them just
 *               compares data to itself.
 */
export type DataProvenance = 'venue' | 'derived';

export type ClosedPnlEntry = {
  /**
   * REQUIRED, deliberately. Optional would mean a new producer that forgets it gets the
   * benefit of the doubt — and the whole lesson of this axis is that silent absence reads
   * as data rather than as an error. Make the compiler ask.
   */
  provenance: DataProvenance;
  symbol: string;
  notional: string | number;
  entryPrice: string | number;
  exitPrice: string | number;
  unrealizedPnl: string | number;
  side: string;
  fees: string | number;
  time: number;
  closedTime: number;
  account: string;
  contracts: string | number;
  leverage: number | string;
};

/**
 * A historical order, including ones that never filled. Distinct from `Order` (open
 * orders) and from fills: it is the only record of intent, so it carries what was
 * asked for alongside what happened — the basis for slippage and unfilled-plan
 * analysis.
 */
/**
 * Options for historical orders. Deliberately NOT `HistoricalTradeOptions`, whose
 * `symbol` is required: a journal needs the account-wide sweep, and per-symbol fan-out
 * is what makes deep backfill economically infeasible. Mirrors the per-axis shape of
 * `IncomeHistoryOptions`.
 */
export type OrderHistoryOptions = {
  readonly symbol?: string;
  readonly orderId?: string;
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
  /** Restrict to terminal states; omit for everything the venue returns. */
  readonly status?: OrderStatus[];
};

export type HistoricalOrder = {
  readonly id: string;
  readonly symbol: string;
  readonly side: OrderSide;
  readonly type: OrderType;
  readonly status: OrderStatus;
  /** Requested price; absent for market orders. */
  readonly price?: number;
  /** Average fill price actually achieved, when the venue reports one. */
  readonly averagePrice?: number;
  readonly amount: number;
  readonly filled: number;
  readonly reduceOnly?: boolean;
  readonly positionSide?: PositionSide | 'both';
  readonly createdTime: number;
  readonly updatedTime?: number;

  /**
   * Venue-reported realized PnL for this order, **GROSS of fees** where the venue reports
   * it that way — which is every venue populating it today.
   *
   * **Absent means NOT MAPPED OR not reported — the two are not distinguishable here, and
   * saying otherwise would be false today.** Only woo populates it. okx (`pnl`), blofin
   * (`pnl`) and bitunix (`realizedPNL`) all RECEIVE the value in rows their own mappers
   * document and drop it on the floor; bybit, binance, paradex and hyperliquid genuinely
   * cannot report it on this axis. So a consumer must not read absence as "this order
   * realized nothing".
   *
   * A sign trap waits for whoever maps the other three: okx and blofin report the paired
   * fee **negative when charged**, woo reports it **positive**. See `fees` below.
   *
   * Never `Number(null)` this field into existence. That coercion yields `0`, and it is
   * exactly how woo's v3 fills route — which returns `realizedPnl: null` on all 534 rows —
   * made "this account has no PnL" indistinguishable from "this route carries no PnL", and
   * got the venue written off as unsupportable for a week.
   *
   * On woo it requires `realized_pnl=true` on the request; the field is simply absent
   * otherwise.
   */
  readonly realizedPnl?: number;

  /** Positive = cost to the trader, matching `PositionRecord.fees`. */
  readonly fees?: number;
  readonly feeAsset?: string;
  /** Positive = received by the trader. Kept separate from `fees`, never netted into it. */
  readonly rebate?: number;
  readonly rebateAsset?: string;
};

export type MessageHandlerStatsType = 'public' | 'private' | 'futures' | 'business';

/**
 * REST request outcomes, classified from the HTTP status alone.
 *
 * Status-only on purpose. The fuzzy classifier in `health/errorPatterns.ts` is
 * a linear regex scan over ~20 pattern groups against a stringified message —
 * correct for a toast, far too expensive for a counter that runs on every
 * request. Anything needing nuance reads the response itself.
 *
 * `network` is separate from `other` because a request that never got a
 * response (timeout, DNS, CORS, tunnel drop) is a different incident from one
 * the venue answered with a 5xx, and collapsing them makes the distinction
 * unrecoverable downstream.
 */
export type RequestOutcome = 'ok' | 'rateLimit' | 'auth' | 'network' | 'other';

export type RequestOutcomeCounts = Record<RequestOutcome, number>;

export type StoreUpdateIds = {
  positions: number;
  orders: number;
};

export type DisplayedMargin = {
  readonly longExposure: number;
  readonly shortExposure: number;
  readonly totalNotional: number;
};

export type StoreData = {
  readonly latency: number | null;
  readonly balance: Balance;
  readonly markets: Market[];
  readonly tickers: Ticker[];
  readonly orders: Order[];
  readonly starkConfig?: {
    usdtConfig?: any;
    usdcConfig?: any;
    positionId?: any;
    takerFeePerToken?: any;
  };
  readonly zkLinkConfig?: {
    contractConfig?: any;
    l2Key?: any;
    ethereumAddress?: any;
    id?: any;
    spotAccount?: any;
    spotWallets?: any;
    contractAccount?: any;
    contractWallets?: any;
  };
  readonly userData?: any;
  readonly positions: Position[];
  readonly displayedPositions: DisplayedPosition[];
  readonly displayedMargin?: DisplayedMargin;
  readonly ordersRisk: { [orderId: string]: OrderRiskProfile[] };
  readonly activeChasers: ActiveChaser[];
  readonly activeTWAPs: ActiveTWAP[];
  readonly loaded: StoreDataLoaded;
  readonly options: StoreOptions;
  readonly updateIds: StoreUpdateIds;
  readonly messageHandlerStats?: Partial<Record<MessageHandlerStatsType, MessageHandlersStats>>;
  /**
   * Cumulative REST request counts for this exchange instance, by outcome.
   *
   * Rides the store for the same reason `socketHealth` does: safe-cex ships in
   * the OSS mirror and must not gain a dependency on any app's telemetry, so
   * the store is the only channel out. The consuming app diffs these against
   * its own last-sent snapshot and reports; the package only counts.
   *
   * Monotonic per exchange-instance lifetime, and deliberately NOT reset on
   * reconnect — a counter that resets is indistinguishable from one that never
   * moved, which is the failure this whole surface exists to avoid.
   */
  readonly requestStats?: RequestOutcomeCounts;
  readonly fetchedLeverage: Record<string, FetchedLeverage>;
  readonly socketHealth?: {
    publicHealthy: boolean;
    privateHealthy: boolean;
    publicInitialized: boolean;
    privateInitialized: boolean;
    publicLastMessage?: number;
    privateLastMessage?: number;
    /**
     * Reconnect diagnostics. All optional so every existing construction site
     * and the OSS-mirror slice keep compiling untouched.
     *
     * `*CloseCode` is the last WebSocket close code — the single most
     * diagnostic reconnect field (1006 network/middlebox, 1000 clean,
     * 4xxx app/auth). `*CloseCountTotal` is monotonic; `*CloseStreak` is the
     * streak-scoped counter that resets on healthy traffic, so it reads 0
     * after recovery and only the total survives.
     *
     * `*ReconnectPaused` is a boolean rather than a remaining-ms countdown on
     * purpose: a live countdown would mutate socketHealth on every watchdog
     * tick, manufacturing store deltas throughout a pause. It is also
     * clock-free, which matters because hosted exchange-server stamps these
     * on the server's clock.
     */
    reconnectDiagnostics?: AccountReconnectDiagnostics;
    publicCloseCode?: number;
    privateCloseCode?: number;
    publicCloseCountTotal?: number;
    privateCloseCountTotal?: number;
    publicCloseStreak?: number;
    privateCloseStreak?: number;
    publicReconnectPaused?: boolean;
    privateReconnectPaused?: boolean;
    /**
     * Optional exchange-specific private stream degradation reason. This covers
     * cases where the socket itself is alive but the authenticated account/order
     * stream is not delivering the data users depend on.
     */
    privateDegradedReason?: string;
  };
};

export type Writable<T> =
  // check for things that are objects but don't need changing
  T extends Date | RegExp | ((...args: any[]) => any)
    ? T
    : T extends ReadonlyMap<infer K, infer V> // maps
      ? Map<Writable<K>, Writable<V>> // make key and values writable
      : T extends ReadonlySet<infer U> // sets
        ? Set<Writable<U>> // make elements writable
        : T extends readonly unknown[] // is an array or tuple?
          ? `${bigint}` extends `${any & keyof T}` // is tuple
            ? { -readonly [K in keyof T]: Writable<T[K]> }
            : Array<Writable<T[number]>> // is regular array
          : T extends Record<string, unknown> // is regular object
            ? { -readonly [K in keyof T]: Writable<T[K]> }
            : T; // is primitive or literal value

export type WritableStoreData = {
  latency: number | null;
  balance: Writable<Balance>;
  markets: Array<Writable<Market>>;
  tickers: Array<Writable<Ticker>>;
  orders: Array<Writable<Order>>;
  positions: Array<Writable<Position>>;
  loaded: Writable<StoreDataLoaded>;
  options: Writable<StoreOptions>;
  userData: Writable<any>;
  fetchedLeverage: Record<string, FetchedLeverage>;
};

export type ExchangeAccount = {
  readonly userId: string;
  readonly subId?: string | null;
  readonly affiliateId?: string | null;
};

export const readOnlyOrderStatuses: OrderStatus[] = [
  OrderStatus.Creating,
  OrderStatus.Updating,
  OrderStatus.Canceling,
] as const;

// Generic income record for journal sync
/**
 * Venue-neutral ledger category. `IncomeRecord.incomeType` stays the venue's own raw
 * string — it is part of the custom-module surface (`apps/module-frame`) and must not
 * change shape — so normalisation is added alongside it rather than replacing it.
 *
 * These are the terms of the reconciliation identity a journal is validated against:
 *   delta equity == realized + funding - fees +/- delta unrealized +/- transfers
 * Without `transfer` you cannot tell a losing trade from a withdrawal.
 */
export type IncomeCategory =
  'realized_pnl' | 'funding' | 'commission' | 'rebate' | 'transfer' | 'liquidation' | 'other';

export type IncomeRecord = {
  symbol: string;
  /** The venue's own raw type string. Unchanged — consumed by user modules. */
  incomeType: string;
  income: number;
  asset: string;
  timestamp: number;
  tranId: string;
  tradeId?: string;
  /** Normalised category. Optional so existing producers stay valid. */
  category?: IncomeCategory;
};

export type IncomeHistoryOptions = {
  symbol?: string;
  incomeType?: string;
  startTime?: number;
  endTime?: number;
  limit?: number;
};

/**
 * Minimal interface for an exchange instance that can serve as a public data source.
 * Used by the 'in-process' external public data provider to avoid circular dependencies
 * with the full Exchange interface (defined in exchanges/base.ts).
 *
 * Any Exchange/BaseExchange instance satisfies this interface naturally.
 */
export interface PublicDataSource {
  readonly store: {
    subscribe: (cb: (data: StoreData) => void) => () => void;
    readonly tickers: StoreData['tickers'];
    readonly markets: StoreData['markets'];
    readonly loaded: StoreData['loaded'];
  };
  listenOrderBook: (symbol: string, callback: (orderbook: OrderBook) => void) => () => void;
  listenOHLCV: (opts: OHLCVOptions, callback: (candle: Candle) => void) => () => void;
}

// Remote signing types — used for hosted CLI servers that delegate signing to clients
export interface RemoteSignerRequest {
  id: string;
  type: 'http' | 'ws-auth';
  exchangeName: ExchangeName;
  accountName?: string; // Account name for multi-account matching
  // For HTTP: unsigned axios config
  config?: {
    method: string;
    url: string;
    baseURL: string;
    params?: Record<string, unknown>;
    data?: unknown;
    headers?: Record<string, string>;
  };
  // For WS auth: exchange-specific auth data
  wsAuth?: {
    toSign: string;
    outputFormat:
      | 'hex'
      | 'base64'
      | 'hex-base64'
      | 'kraken-challenge'
      | 'apexomni-base64'
      | 'bitunix-double-sha256'
      | 'backpack-ed25519'
      | 'kucoin-batch'
      // Hyperliquid: `toSign` is a structured L1 statement the device re-derives
      // the digest from; `hyperliquid-address` binds the agent address.
      | 'grvt-order'
      | 'grvt-address'
      | 'hyperliquid-l1'
      | 'hyperliquid-address'
      // Lighter: `toSign` is a transaction or auth statement the device re-derives
      // the signed words from; the reply concatenates the 80-byte signatures.
      | 'lighter-tx'
      // Paradex: `toSign` is a closed `paradex/1` descriptor the device rebuilds
      // the Stark digests from; `paradex-public` binds the Stark public key.
      | 'paradex-stark'
      | 'paradex-public'
      | 'coinbase-jwt';
    extraData?: unknown;
  };
}

export interface RemoteSignerResponse {
  id: string;
  config?: {
    url?: string;
    params?: Record<string, unknown>;
    data?: unknown;
    headers: Record<string, string>;
  };
  wsAuthSignature?: string;
  error?: string;
}

/**
 * `hmac-sha256`: `payload` is the exact string the venue HMACs.
 * `stark`: `payload` is the 0x-hex typed-data message hash, and `signature` must be
 * the serialised `["<r>","<s>"]` the venue expects, or a stand-in for it.
 */
export type PayloadSignerRequest = {
  exchangeName: ExchangeName;
  key: string;
  payload: string;
} & ({ scheme: 'hmac-sha256'; encoding: 'hex' | 'base64' } | { scheme: 'stark'; encoding: 'felt-pair' });

export interface PayloadSignerResult {
  signature: string;
  headers?: Record<string, string>;
}

export type PayloadSigner = (request: PayloadSignerRequest) => PayloadSignerResult | Promise<PayloadSignerResult>;

export interface RemoteSigner {
  sign(request: RemoteSignerRequest): Promise<RemoteSignerResponse>;
  isAvailable(): boolean;
  onAvailabilityChange(callback: (available: boolean) => void): () => void;
  /** Account name for multi-account matching (e.g., "bear2") */
  accountName?: string;
}
