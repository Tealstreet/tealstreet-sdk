// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./env.d.ts" />
import type { ChaserOrigin } from './webworker/chaserOrder/ChaserOrderOptions';
import type { TwapInstance } from './webworker/twap/TwapInstance';
import type { TwapOptions } from './webworker/twap/TwapOptions';
import type { TwapState } from './webworker/twap/TwapState';

import { MessageHandlersStats } from './exchanges/types';

export type ExchangeName =
  | 'apexomni'
  | 'asterdex'
  | 'backpack'
  | 'binance'
  | 'binancespot'
  | 'binancepm'
  | 'bingx'
  | 'bitget'
  | 'bitgetinverse'
  | 'bitgetusdc'
  | 'bitmex'
  | 'bitmextestnet'
  | 'bitrue'
  | 'bitunix'
  | 'blofin'
  | 'breakoutprop'
  | 'bybit'
  | 'bybitdemo'
  | 'bybitinverse'
  | 'bybitspot'
  | 'coinbase'
  | 'coinbasespot'
  | 'cryptocom'
  | 'grvt'
  | 'hyperliquid'
  | 'hyperliquidspot'
  | 'lighter'
  | 'okx'
  | 'okxeu'
  | 'paradex'
  | 'phemex'
  | 'polymarket'
  | 'kodiakfi'
  | 'mode'
  | 'salsadex'
  | 'tealstreet-dex'
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

  readonly starkPrivateKey?: string;
  readonly vault?: string;
  readonly applicationId?: string;
  readonly apiKeyIndex?: string;
  readonly testnet?: boolean;
  /** See `ExchangeAccountConfig.enableSpotMarkets`. Default off. */
  readonly enableSpotMarkets?: boolean;
  readonly publicDataOnly?: boolean;
  readonly privateOnly?: boolean;
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
    readonly isMobile?: boolean;
    readonly calculateUpnlFromRealtimeLastPrice?: boolean;
    readonly dataUpdateThrottleMultiplier?: number;
    readonly skipRendersWhenWindowIsHidden?: boolean;
    readonly consoleLogging?: boolean;
  };
  readonly extra?: {
    tickInterval?: number;
    recvWindow?: number;
    // Optional AsterDEX REST API version override.
    asterdexApiVersion?: 'v1' | 'v3';
    // Lighter builder/integrator fee mode.
    lighterBuilderFeeMode?: 'dynamic' | 'static';
    // Optional override for Lighter integrator account index.
    lighterIntegratorAccountIndex?: string;
    // Lighter TP/SL allowed execution slippage, as a percentage from trigger price.
    lighterStopSlippagePct?: number;
  };

  /** Delegates request signing to a signer outside this exchange runtime. */
  readonly remoteSigner?: RemoteSigner;

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
export interface ValidateAccountResult {
  error: string;
  balance?: Balance;
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
  healthStatus?: 'ok' | 'auth_error' | 'auth_expired' | 'permissions_error';

  /** Last write timestamp (ms since epoch). Used for last-write-wins
   *  across CLI ↔ web account sync. Missing → treated as 0 for LWW. */
  updatedAt?: number;
  /** Tombstone marker for sync deletes. Live accounts have undefined. */
  deletedAt?: number;
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

export type CoinBalance = {
  base: string;
  amount: number;
  notional: number;
};

export type Balance = {
  readonly used: number;
  readonly free: number;
  readonly total: number;
  readonly upnl: number;
  readonly info?: any;
  readonly currencies?: { [currency: string]: Balance };
  readonly coins?: { [coin: string]: CoinBalance };
};

export type Market = {
  readonly id: string;
  readonly symbol: string;
  readonly prettySymbol?: string;
  readonly base: string;
  readonly quote: string;
  readonly active: boolean;
  readonly precision: {
    readonly amount: number;
    readonly price: number;
  };
  readonly limits: {
    readonly amount: {
      readonly min: number;
      readonly max: number;
    };
    readonly leverage: {
      readonly min: number;
      readonly max: number;
    };
  };
  readonly contractSize?: number;
  readonly isExpiring?: boolean;
  readonly isInverse?: boolean;
  readonly isSpot?: boolean;
  readonly isReadOnly?: boolean;
  readonly info?: any;
  readonly takerFee?: number;
  readonly makerFee?: number;
};

export type Ticker = {
  readonly id: string;
  readonly symbol: string;
  readonly bid: number;
  readonly ask: number;
  readonly last: number;
  /**
   * Derivatives metadata uses `0` when the venue does not publish the field,
   * including spot venues where the concept does not apply. Consumers must
   * not interpret zero as proof of a real zero funding rate, mark/index price,
   * funding time, or open-interest value.
   */
  readonly mark: number;
  readonly index: number;
  /**
   * Exchange-published midpoint when the venue exposes one separately
   * from `(bid + ask) / 2`. Hyperliquid publishes `midPx` directly
   * (and their `bid`/`ask` are impact-weighted, not L1 quotes), so
   * consumers who want the canonical mid should prefer this when
   * set and fall back to `(bid + ask) / 2` otherwise. Most exchanges
   * leave this unset.
   */
  readonly midPrice?: number;
  readonly percentage: number;
  readonly openInterest: number;
  readonly fundingRate: number;
  readonly fundingTime: number;
  readonly volume: number;
  readonly quoteVolume: number;
  readonly info?: any;
};

export type PublicTrade = {
  readonly id: string;
  readonly symbol: string;
  readonly price: number;
  readonly qty: number;
  readonly quoteQty?: number;
  readonly timestamp: number; // ms
  // which side is taker
  readonly side: OrderSide;
};

export enum OrderStatus {
  Open = 'open',
  Closed = 'closed',
  Canceled = 'canceled',

  // local statuses
  Creating = 'creating',
  Updating = 'updating',
  Canceling = 'canceling',
}

export enum OrderType {
  Market = 'market',
  Limit = 'limit',
  Conditional = 'conditional',
  StopLoss = 'stop_market',
  TakeProfit = 'take_profit_market',
  TrailingStopLoss = 'trailing_stop_market',
}

export enum OrderSide {
  Buy = 'buy',
  Sell = 'sell',
}

export enum OrderTimeInForce {
  GoodTillCancel = 'GoodTillCancel',
  ImmediateOrCancel = 'ImmediateOrCancel',
  FillOrKill = 'FillOrKill',
  PostOnly = 'PostOnly',
}

export enum OrderStopTriggerType {
  MarkPrice = 'mark_price',
  LastPrice = 'last_price',
  IndexPrice = 'index_price',
}

// some exchanges reject orders async (via ws after order sent over http). this lets us track why it failed
// needed for things like chaser order to handle post only order failure
export enum OrderRejectReason {
  PostOnly = 'PostOnly',
  InsufficientBalance = 'InsufficientBalance',
  ReduceOnly = 'ReduceOnly',
}

export type Order = {
  readonly id: string;
  readonly parentId?: string;
  readonly status: OrderStatus;
  readonly symbol: string;
  readonly type: OrderType;
  readonly side: OrderSide;
  readonly price: number;
  readonly amount: number;
  readonly filled: number;
  // always in whole units of base for easy math elsewhere
  readonly remaining: number;
  readonly reduceOnly: boolean;
  readonly close?: boolean;
  readonly info?: any;
  readonly timeInForce?: OrderTimeInForce;
  readonly lastUpdate: number;
  readonly triggerType?: OrderStopTriggerType;
  readonly tpTriggerType?: OrderStopTriggerType;
  readonly slTriggerType?: OrderStopTriggerType;
  readonly takeProfit?: number;
  readonly stopLoss?: number;
  readonly extra?: {
    // actual remaining size in contracts considering contract size, inverse, quanto, etc
    readonly remainingNative?: number;
    readonly remainingNotional?: number;
    readonly conditionalOrderPrice?: number;
    readonly rejectReason?: OrderRejectReason;
    // Chaser-related fields
    readonly isFromChaser?: boolean;
    readonly chaserId?: string;
    readonly chaserAttempts?: number;
    readonly chaserMaxAttempts?: number;
  };
};

export enum PositionSide {
  Long = 'long',
  Short = 'short',
}

export enum PositionMode {
  Hedged = 'hedged',
  OneWay = 'oneway',
}

export enum MarginType {
  Isolated = 'isolated',
  Cross = 'cross',
}

/**
 * If you're modifying this file, update createEmptyPositionFrom in store.base.ts
 * accordingly as well.
 */
export type Position = {
  readonly id: string;
  readonly symbol: string;
  readonly side: PositionSide;
  readonly entryPrice: number;
  readonly notional: number;
  readonly leverage: number;
  readonly marginType?: MarginType;
  readonly positionMode?: PositionMode;
  readonly unrealizedPnl: number;
  readonly realizedPnl?: number;
  // always in whole units of base for easy math elsewhere
  readonly contracts: number;
  readonly liquidationPrice: number;
  // absolute quote-currency margin allocated to the position, when the
  // exchange exposes it. Cross-margin positions may be reported as 0 by
  // some exchanges; mappers should fall back to notional/leverage in that
  // case if a meaningful number can be derived.
  readonly marginSize?: number;
  readonly info?: any;
  readonly extra?: {
    // actual size in contracts considering contract size, inverse, quanto, etc
    readonly contractsNative?: number;
    readonly unrealizedPnlNative?: number;
  };
  readonly lastUpdate?: number;
};

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

export type Execution = {
  readonly id?: string;
  readonly timestamp: number;
  readonly symbol: string;
  readonly orderId?: string;
  readonly price: number;
  readonly qty: number;
  readonly side: OrderSide;
  readonly commission?: number;
  readonly commissionAsset?: string;
};

export type PlaceOrderOpts = {
  readonly symbol: string;
  readonly type: OrderType;
  readonly side: OrderSide;
  readonly amount: number;
  readonly price?: number;
  readonly triggerPrice?: number;
  readonly stopLoss?: number;
  readonly takeProfit?: number;
  readonly reduceOnly?: boolean;
  readonly timeInForce?: OrderTimeInForce;
  readonly triggerType?: OrderStopTriggerType;
  readonly tpTriggerType?: OrderStopTriggerType;
  readonly slTriggerType?: OrderStopTriggerType;
  readonly maxSlippagePct?: number;
  readonly close?: boolean;
};

export type UpdateOrderOpts = {
  readonly order: Order;
  readonly update: { readonly amount: number } | { readonly price: number };
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

export type OrderFillEvent = Pick<Order, 'id' | 'amount' | 'price' | 'side' | 'symbol'> & {
  orderId?: string; // Order ID for tracking fills per order (used by chaser)
  realizedPnl?: number;
  fee?: number;
};

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
  isFetching?: boolean;
};

export type ClosedPnlEntry = {
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

export type MessageHandlerStatsType = 'public' | 'private' | 'futures' | 'business';

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
  readonly latency: number;
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
  readonly fetchedLeverage: Record<string, FetchedLeverage>;
  readonly socketHealth?: {
    publicHealthy: boolean;
    privateHealthy: boolean;
    publicInitialized: boolean;
    privateInitialized: boolean;
    publicLastMessage?: number;
    privateLastMessage?: number;
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
  latency: number;
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
export type IncomeRecord = {
  symbol: string;
  incomeType: string;
  income: number;
  asset: string;
  timestamp: number;
  tranId: string;
  tradeId?: string;
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
    outputFormat: 'hex' | 'base64' | 'hex-base64' | 'kraken-challenge' | 'apexomni-base64';
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

export interface RemoteSigner {
  sign(request: RemoteSignerRequest): Promise<RemoteSignerResponse>;
  isAvailable(): boolean;
  onAvailabilityChange(callback: (available: boolean) => void): () => void;
  /** Account name for multi-account matching (e.g., "bear2") */
  accountName?: string;
}
