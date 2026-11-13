// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./env.d.ts" />
export type { HistoricalOrder } from './generated/HistoricalOrder';
export type { HistoricalTrade } from './generated/HistoricalTrade';
export type { ClosedPnlEntry } from './generated/ClosedPnlEntry';
export type { IncomeRecord } from './generated/IncomeRecord';
export type { DataProvenance } from './generated/DataProvenance';
export type { IncomeCategory } from './generated/IncomeCategory';
import { OrderStatus } from './generated/OrderStatus';
import { OrderSide } from './generated/OrderSide';
import type { Order } from './generated/Order';
import type { StoreData } from './generated/StoreData';
export type { StoreData } from './generated/StoreData';
import type { StoreOptions } from './generated/StoreOptions';
export type { StoreOptions } from './generated/StoreOptions';
import type { StoreDataLoaded } from './generated/StoreDataLoaded';
export type { StoreDataLoaded } from './generated/StoreDataLoaded';
export type { StoreUpdateIds } from './generated/StoreUpdateIds';
import type { FetchedLeverage } from './generated/FetchedLeverage';
export type { FetchedLeverage } from './generated/FetchedLeverage';
export type { PositionSettings } from './generated/PositionSettings';
export type { DisplayedMargin } from './generated/DisplayedMargin';

import type { OHLCVFrameRejection, OrderBookFrameRejection } from './exchanges/base';
import type { ExchangeErrorKind } from './commandOutcome';
import type { Candle } from './generated/Candle';
import type { OrderBook } from './generated/OrderBook';
export type { Candle, OrderBook };
export type { OrderBookOrders } from './generated/OrderBookOrders';
import type { Market } from './generated/Market';
import type { OptionRight } from './generated/OptionRight';
import type { OptionTerms } from './generated/OptionTerms';
import type { OptionGreeks } from './generated/OptionGreeks';
import type { Position } from './generated/Position';
import type { ExchangeOptions } from './generated/ExchangeOptions';
import type { Ticker } from './generated/Ticker';
import type { ChaserOrigin } from './webworker/chaserOrder/ChaserOrderOptions';
import type { TwapInstance } from './webworker/twap/TwapInstance';
import type { TwapOptions } from './webworker/twap/TwapOptions';
import type { TwapState } from './webworker/twap/TwapState';

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
  | 'bingxspot'
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

export const readOnlyOrderStatuses: OrderStatus[] = [
  OrderStatus.Creating,
  OrderStatus.Updating,
  OrderStatus.Canceling,
] as const;

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
  listenOrderBook: (
    symbol: string,
    callback: (orderbook: OrderBook) => void,
    onRejection?: (signal: OrderBookFrameRejection) => void,
  ) => () => void;
  listenOHLCV: (
    opts: OHLCVOptions,
    callback: (candle: Candle) => void,
    onRejection?: (signal: OHLCVFrameRejection) => void,
  ) => () => void;
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
      | 'kraken-sha512'
      | 'apexomni-base64'
      | 'apexomni-batch'
      | 'bitunix-double-sha256'
      | 'backpack-ed25519'
      | 'orderly-ed25519'
      | 'kucoin-batch'
      // AsterDEX: device-owned slot selection and canonical wallet statements.
      | 'asterdex-address'
      | 'asterdex-hmac'
      | 'asterdex-wallet'
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
      | 'coinbase-jwt'
      | 'coinbase-jws';
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
 * `coinbase-jws`: exact JWS input; return unpadded base64url of 64 signature bytes.
 * `stark`: `payload` is the 0x-hex typed-data message hash, and `signature` must be
 * the serialised `["<r>","<s>"]` the venue expects, or a stand-in for it.
 */
export type PayloadSignerRequest = {
  exchangeName: ExchangeName;
  key: string;
  payload: string;
} & ({ scheme: 'hmac-sha256'; encoding: 'hex' | 'base64' } | { scheme: 'stark'; encoding: 'felt-pair' }
  | { scheme: 'coinbase-jws'; encoding: 'base64url'; algorithm: 'es256' | 'ed25519' });

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

export type { ExchangeOptions };
export type { ExchangeAccount } from './generated/ExchangeAccount';
export type { ExchangeAccountConfig } from './generated/ExchangeAccountConfig';
