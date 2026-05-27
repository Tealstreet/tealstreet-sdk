import type { ExchangeOptions } from '../../types';

export const ASTERDEX_ENDPOINTS = {
  REST: {
    PRODUCTION: 'https://fapi.asterdex.com',
    TESTNET: 'https://testnet-fapi.asterdex.com', // TODO: Verify testnet URL
  },
  WS: {
    PUBLIC: 'wss://fstream.asterdex.com/stream',
    PRIVATE: 'wss://fstream.asterdex.com/stream',
  },
};

export type AsterDEXAPIVersion = 'v1' | 'v3';
export type AsterDEXFapiVersion = AsterDEXAPIVersion | 'v4';

export type AsterDEXV3AuthContext = {
  userAddress?: string;
  signerAddress?: string;
  signerPrivateKey?: string;
};

const trimNonEmpty = (value?: string): string | undefined => {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
};

const ASTERDEX_PRIVATE_KEY_REGEX = /^(0x)?[0-9a-fA-F]{64}$/;
const ASTERDEX_ADDRESS_REGEX = /^0x[0-9a-fA-F]{40}$/;

export const isAsterDEXV3PrivateKey = (value?: string): boolean => {
  return Boolean(value && ASTERDEX_PRIVATE_KEY_REGEX.test(value));
};

export const isAsterDEXAddress = (value?: string): boolean => {
  return Boolean(value && ASTERDEX_ADDRESS_REGEX.test(value));
};

export const resolveAsterDEXV3AuthContext = (options: ExchangeOptions): AsterDEXV3AuthContext => {
  return {
    // Keep auth.* as highest priority but allow account-model fallback fields.
    userAddress: trimNonEmpty(options.auth?.userAddress) || trimNonEmpty(options.applicationId),
    signerAddress: trimNonEmpty(options.auth?.signerAddress) || trimNonEmpty(options.key),
    signerPrivateKey: trimNonEmpty(options.auth?.privateKey) || trimNonEmpty(options.secret),
  };
};

export const hasAsterDEXV3AuthContext = (options: ExchangeOptions): boolean => {
  const auth = resolveAsterDEXV3AuthContext(options);
  return Boolean(
    isAsterDEXAddress(auth.userAddress) &&
      isAsterDEXAddress(auth.signerAddress) &&
      isAsterDEXV3PrivateKey(auth.signerPrivateKey),
  );
};

export const resolveAsterDEXApiVersion = (options: ExchangeOptions): AsterDEXAPIVersion => {
  const configuredVersion = options.extra?.asterdexApiVersion;
  if (configuredVersion === 'v1' || configuredVersion === 'v3') {
    return configuredVersion;
  }

  if (hasAsterDEXV3AuthContext(options)) {
    return 'v3';
  }

  return 'v1';
};

export const getAsterDEXFapiPath = (endpoint: string, version: AsterDEXFapiVersion): string => {
  const normalizedEndpoint = endpoint.replace(/^\/+/, '');
  return `/fapi/${version}/${normalizedEndpoint}`;
};

export const ASTERDEX_RATE_LIMIT_ENDPOINTS = {
  HEAVY: ['/order', '/batch', '/position'],
  MEDIUM: ['/account', '/balance', '/trades'],
  LIGHT: ['/ticker', '/klines', '/depth'],
};

export const asterdexOrderTypeMap: Record<string, string> = {
  // Map from OrderType enum values (lowercase) to AsterDEX API values (uppercase)
  limit: 'LIMIT',                           // OrderType.Limit = 'limit'
  market: 'MARKET',                         // OrderType.Market = 'market'
  stop_market: 'STOP_MARKET',              // OrderType.StopLoss = 'stop_market'
  take_profit_market: 'TAKE_PROFIT_MARKET', // OrderType.TakeProfit = 'take_profit_market'
  trailing_stop_market: 'TRAILING_STOP_MARKET', // OrderType.TrailingStopLoss = 'trailing_stop_market'
};

export const asterdexOrderSideMap: Record<string, string> = {
  buy: 'BUY',    // Map from OrderSide.Buy ('buy') to AsterDEX 'BUY'
  sell: 'SELL',  // Map from OrderSide.Sell ('sell') to AsterDEX 'SELL'
};

export const asterdexPositionSideMap: Record<string, string> = {
  LONG: 'LONG',
  SHORT: 'SHORT',
  BOTH: 'BOTH',
};

export const asterdexTimeInForceMap: Record<string, string> = {
  GoodTillCancel: 'GTC',
  ImmediateOrCancel: 'IOC',
  FillOrKill: 'FOK',
  PostOnly: 'GTX', // AsterDEX uses GTX for PostOnly (Good Till Crossing)
};

export const asterdexOrderStatusMap: Record<string, string> = {
  NEW: 'NEW',
  PARTIALLY_FILLED: 'PARTIALLY_FILLED',
  FILLED: 'FILLED',
  CANCELED: 'CANCELED',
  REJECTED: 'REJECTED',
  EXPIRED: 'EXPIRED',
};

// Response Types
export interface AsterDEXMarketResponse {
  symbol: string;
  baseAsset: string;
  quoteAsset: string;
  contractType: string;
  deliveryDate?: number;
  onboardDate?: number;
  status: string;
  maintMarginPercent: string;
  requiredMarginPercent: string;
  pricePrecision: number;
  quantityPrecision: number;
  baseAssetPrecision: number;
  quotePrecision: number;
  underlyingType?: string;
  underlyingSubType?: string[];
  settlePlan?: number;
  triggerProtect: string;
  filters: Array<{
    filterType: string;
    maxPrice?: string;
    minPrice?: string;
    tickSize?: string;
    maxQty?: string;
    minQty?: string;
    stepSize?: string;
    limit?: number;
    notional?: string;
    multiplierUp?: string;
    multiplierDown?: string;
    multiplierDecimal?: string;
  }>;
  orderTypes: string[];
  timeInForce: string[];
  liquidationFee: string;
  marketTakeBound: string;
}

export interface AsterDEXAccountResponse {
  accountAlias: string;
  feeTier: number;
  canTrade: boolean;
  canDeposit: boolean;
  canWithdraw: boolean;
  updateTime: number;
  totalInitialMargin: string;
  totalMaintMargin: string;
  totalWalletBalance: string;
  totalUnrealizedProfit: string;
  totalMarginBalance: string;
  totalPositionInitialMargin: string;
  totalOpenOrderInitialMargin: string;
  totalCrossWalletBalance: string;
  totalCrossUnPnl: string;
  availableBalance: string;
  maxWithdrawAmount: string;
  assets: Array<{
    asset: string;
    walletBalance: string;
    unrealizedProfit: string;
    marginBalance: string;
    maintMargin: string;
    initialMargin: string;
    positionInitialMargin: string;
    openOrderInitialMargin: string;
    maxWithdrawAmount: string;
    crossWalletBalance: string;
    crossUnPnl: string;
    availableBalance: string;
    marginAvailable: boolean;
    updateTime: number;
  }>;
  positions: Array<{
    symbol: string;
    initialMargin: string;
    maintMargin: string;
    unrealizedProfit: string;
    positionInitialMargin: string;
    openOrderInitialMargin: string;
    leverage: string;
    isolated: boolean;
    entryPrice: string;
    maxNotional: string;
    positionSide: string;
    positionAmt: string;
    notional: string;
    isolatedWallet: string;
    updateTime: number;
    bidNotional: string;
    askNotional: string;
  }>;
}

export interface AsterDEXPositionModeResponse {
  dualSidePosition: boolean | 'true' | 'false';
}

export interface AsterDEXPositionResponse {
  symbol: string;
  positionAmt: string;
  entryPrice: string;
  breakEvenPrice: string;
  markPrice: string;
  unRealizedProfit: string;
  liquidationPrice: string;
  leverage: string;
  maxNotionalValue: string;
  marginType: string;
  isolatedMargin: string;
  isAutoAddMargin: string;
  positionSide: string;
  notional: string;
  isolatedWallet: string;
  updateTime: number;
}

export interface AsterDEXOrderResponse {
  symbol: string;
  orderId: number;
  clientOrderId: string;
  price: string;
  origQty: string;
  executedQty: string;
  cummulativeQuoteQty: string;
  status: string;
  timeInForce: string;
  type: string;
  side: string;
  stopPrice?: string;
  icebergQty?: string;
  time: number;
  updateTime: number;
  isWorking: boolean;
  workingTime: number;
  origQuoteOrderQty: string;
  positionSide?: string;
  closePosition?: boolean;
  reduceOnly?: boolean;
  activatePrice?: string;
  priceRate?: string;
  priceProtect?: boolean;
}

export interface AsterDEXTickerResponse {
  symbol: string;
  priceChange: string;
  priceChangePercent: string;
  weightedAvgPrice: string;
  lastPrice: string;
  lastQty: string;
  openPrice: string;
  highPrice: string;
  lowPrice: string;
  volume: string;
  quoteVolume: string;
  openTime: number;
  closeTime: number;
  firstId: number;
  lastId: number;
  count: number;
}

export interface AsterDEXOpenInterestResponse {
  symbol: string;
  openInterest: string;
  time?: number;
}

export interface AsterDEXDepthResponse {
  lastUpdateId: number;
  E: number; // Message output time
  T: number; // Transaction time
  bids: Array<[string, string]>; // [price, quantity]
  asks: Array<[string, string]>; // [price, quantity]
}

export interface AsterDEXTradeResponse {
  id: number;
  price: string;
  qty: string;
  quoteQty: string;
  time: number;
  isBuyerMaker: boolean;
}

export interface AsterDEXUserTradeResponse {
  symbol: string;
  id: number;
  orderId: number;
  price: string;
  qty: string;
  quoteQty?: string;
  realizedPnl?: string;
  side: string;
  positionSide?: string;
  buyer?: boolean;
  maker?: boolean;
  commission?: string;
  commissionAsset?: string;
  time: number;
}

const ASTERDEX_DEFAULT_POSITION_SIDE = 'BOTH';

const getAsterDEXRealizedPnlKey = (
  trade: Pick<AsterDEXUserTradeResponse, 'symbol' | 'positionSide'>,
): string => {
  const normalizedPositionSide = String(trade.positionSide || ASTERDEX_DEFAULT_POSITION_SIDE).toUpperCase();
  return `${trade.symbol}:${normalizedPositionSide}`;
};

/**
 * Aster userTrades realizedPnl behaves as a running cumulative value (same
 * notion as ACCOUNT_UPDATE.a.P[].cr). Convert it to per-trade deltas so
 * downstream history math remains session-local instead of inheriting an old
 * account baseline.
 */
export const normalizeAsterDEXRealizedPnlDeltas = (
  trades: AsterDEXUserTradeResponse[],
): Array<number | undefined> => {
  const normalized = new Array<number | undefined>(trades.length).fill(undefined);
  const groupedTrades = new Map<
    string,
    Array<{ index: number; id: number; time: number; realizedPnl: number }>
  >();

  trades.forEach((trade, index) => {
    if (trade.realizedPnl === undefined) {
      return;
    }

    const parsedRealizedPnl = Number.parseFloat(trade.realizedPnl);
    if (!Number.isFinite(parsedRealizedPnl)) {
      return;
    }

    const key = getAsterDEXRealizedPnlKey(trade);
    const bucket = groupedTrades.get(key) || [];
    bucket.push({
      index,
      id: Number.isFinite(trade.id) ? trade.id : 0,
      time: Number.isFinite(trade.time) ? trade.time : 0,
      realizedPnl: parsedRealizedPnl,
    });
    groupedTrades.set(key, bucket);
  });

  for (const bucket of groupedTrades.values()) {
    bucket.sort((a, b) => {
      if (a.time !== b.time) return a.time - b.time;
      if (a.id !== b.id) return a.id - b.id;
      return a.index - b.index;
    });

    let previousCumulative: number | undefined;
    for (const trade of bucket) {
      if (previousCumulative === undefined) {
        // Oldest visible trade acts as baseline for this window.
        normalized[trade.index] = 0;
      } else {
        const delta = trade.realizedPnl - previousCumulative;
        normalized[trade.index] = Number.isFinite(delta) ? delta : 0;
      }
      previousCumulative = trade.realizedPnl;
    }
  }

  return normalized;
};

// AsterDEX returns klines as arrays, not objects
// Format: [openTime, open, high, low, close, volume, closeTime, quoteAssetVolume, trades, takerBuyBaseAssetVolume, takerBuyQuoteAssetVolume, ignore]
export type AsterDEXKlineResponse = [
  number,  // openTime
  string,  // open
  string,  // high
  string,  // low
  string,  // close
  string,  // volume
  number,  // closeTime
  string,  // quoteAssetVolume
  number,  // trades
  string,  // takerBuyBaseAssetVolume
  string,  // takerBuyQuoteAssetVolume
  string   // ignore
];

// WebSocket Message Types
export interface AsterDEXWebSocketMessage {
  e: string; // Event type
  E: number; // Event time
  s?: string; // Symbol
  [key: string]: any;
}

export interface AsterDEXStreamTicker {
  e: 'ticker'; // Event type
  E: number; // Event time
  s: string; // Symbol
  p: string; // Price change
  P: string; // Price change percent
  w: string; // Weighted average price
  c: string; // Last price
  Q: string; // Last quantity
  o: string; // Open price
  h: string; // High price
  l: string; // Low price
  v: string; // Total traded base asset volume
  q: string; // Total traded quote asset volume
  O: number; // Statistics open time
  C: number; // Statistics close time
  F: number; // First trade ID
  L: number; // Last trade ID
  n: number; // Total number of trades
}

export interface AsterDEXStreamDepth {
  e: 'depthUpdate'; // Event type
  E: number; // Event time
  T: number; // Transaction time
  s: string; // Symbol
  U: number; // First update ID in event
  u: number; // Final update ID in event
  pu: number; // Final update ID in last stream
  b: Array<[string, string]>; // Bids [price, quantity]
  a: Array<[string, string]>; // Asks [price, quantity]
}

export interface AsterDEXStreamTrade {
  e: 'aggTrade'; // Event type
  E: number; // Event time
  s: string; // Symbol
  a: number; // Aggregate trade ID
  p: string; // Price
  q: string; // Quantity
  f: number; // First trade ID
  l: number; // Last trade ID
  T: number; // Trade time
  m: boolean; // Is buyer maker
}

export interface AsterDEXStreamMarkPrice {
  e: 'markPriceUpdate'; // Event type
  E: number; // Event time
  s: string; // Symbol
  p: string; // Mark price
  i: string; // Index price
  P: string; // Estimated settle price
  r: string; // Funding rate
  T: number; // Next funding time
}

// Account Update Event
export interface AsterDEXAccountUpdate {
  e: 'ACCOUNT_UPDATE'; // Event type
  E: number; // Event time
  T: number; // Transaction time
  a: {
    m: string; // Event reason type
    B: Array<{
      a: string; // Asset
      wb: string; // Wallet balance
      cw: string; // Cross wallet balance
      bc: string; // Balance change except PnL and commission
    }>;
    P: Array<{
      s: string; // Symbol
      pa: string; // Position amount
      ep: string; // Entry price
      cr: string; // (Pre-fee) accumulated realized
      up: string; // Unrealized PnL
      mt: string; // Margin type
      iw: string; // Isolated wallet (if isolated position)
      ps: string; // Position side
    }>;
  };
}

// Order Update Event
export interface AsterDEXOrderUpdate {
  e: 'ORDER_TRADE_UPDATE'; // Event type
  E: number; // Event time
  T: number; // Transaction time
  o: {
    s: string; // Symbol
    c: string; // Client order ID
    S: string; // Side
    o: string; // Order type
    f: string; // Time in force
    q: string; // Original quantity
    p: string; // Original price
    ap: string; // Average price
    sp: string; // Stop price
    x: string; // Execution type
    X: string; // Order status
    i: number; // Order ID
    l: string; // Order last filled quantity
    z: string; // Order filled accumulated quantity
    L: string; // Last filled price
    N: string; // Commission asset
    n: string; // Commission
    T: number; // Order trade time
    t: number; // Trade ID
    b: string; // Bids notional
    a: string; // Ask notional
    m: boolean; // Is this trade the maker side?
    R: boolean; // Is this reduce only
    wt: string; // Stop price working type
    ot: string; // Original order type
    ps: string; // Position side
    cp: boolean; // Close all
    AP: string; // Activation price (only for TRAILING_STOP)
    cr: string; // Callback rate (only for TRAILING_STOP)
    pP: boolean; // Ignore
    si: number; // Ignore
    ss: number; // Ignore
    rp: string; // Realized profit
  };
}

// Listen Key Response
export interface AsterDEXListenKeyResponse {
  listenKey: string;
}
