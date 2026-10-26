/**
 * Transport-neutral WOO X V3 migration contracts.
 *
 * This module is intentionally not imported by the production Woo exchange,
 * API client, or websocket implementations. It captures the vendor contract
 * needed for a later gated migration without changing any live routing.
 */

export const WOO_V3_REST_BASE_URL = {
  livenet: 'https://api.woox.io',
  testnet: 'https://api.staging.woox.io',
} as const;

export const WOO_V3_WS_URL = {
  public: {
    livenet: 'wss://wss.woox.io/v3/public',
    testnet: 'wss://wss.staging.woox.io/v3/public',
  },
  private: {
    livenet: 'wss://wss.woox.io/v3/private',
    testnet: 'wss://wss.staging.woox.io/v3/private',
  },
} as const;

export const WOO_V3_ENDPOINTS = {
  PUBLIC_INSTRUMENTS: '/v3/public/instruments',
  PUBLIC_FUTURES: '/v3/public/futures',
  PUBLIC_ORDERBOOK: '/v3/public/orderbook',
  PUBLIC_KLINE: '/v3/public/kline',
  PUBLIC_KLINE_HISTORY: '/v3/public/klineHistory',
  TRADE_ORDER: '/v3/trade/order',
  TRADE_ORDERS: '/v3/trade/orders',
  TRADE_ALGO_ORDER: '/v3/trade/algoOrder',
  TRADE_ALGO_ORDERS: '/v3/trade/algoOrders',
  TRADE_ALL_ORDERS: '/v3/trade/allOrders',
  TRADE_CANCEL_ALL_AFTER: '/v3/trade/cancelAllAfter',
  TRANSACTION_HISTORY: '/v3/trade/transactionHistory',
  ACCOUNT_INFO: '/v3/account/info',
  ACCOUNT_LISTEN_KEY: '/v3/account/listenKey',
  ASSET_BALANCES: '/v3/asset/balances',
  FUTURES_POSITIONS: '/v3/futures/positions',
  FUTURES_POSITION_MODE: '/v3/futures/positionMode',
  FUTURES_LEVERAGE: '/v3/futures/leverage',
  SPOT_MARGIN_LEVERAGE: '/v3/spotMargin/leverage',
} as const;

export type WooV3Endpoint = (typeof WOO_V3_ENDPOINTS)[keyof typeof WOO_V3_ENDPOINTS];
export type WooV3HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE';

export type WooV3MigrationGate =
  | 'publicReadParity'
  | 'privateReadParity'
  | 'regularWriteDemoLifecycle'
  | 'algoWriteDemoLifecycle'
  | 'cancelAllAfterDemoLifecycle'
  | 'accountWriteDemoLifecycle'
  | 'publicWebsocketParity'
  | 'privateWebsocketParity';

/** Every route family stays disabled until its named release evidence exists. */
export const WOO_V3_MIGRATION_GATES: Readonly<Record<WooV3MigrationGate, false>> = Object.freeze({
  publicReadParity: false,
  privateReadParity: false,
  regularWriteDemoLifecycle: false,
  algoWriteDemoLifecycle: false,
  cancelAllAfterDemoLifecycle: false,
  accountWriteDemoLifecycle: false,
  publicWebsocketParity: false,
  privateWebsocketParity: false,
});

export const WOO_V3_RUNTIME_STATUS = 'scaffold-only' as const;

export const WOO_V3_WEBSOCKET_MIGRATION = {
  public: {
    legacyUrl: 'wss://wss.woox.io/ws/stream/:applicationId',
    v3Url: WOO_V3_WS_URL.public.livenet,
    authentication: 'none',
    gate: 'publicWebsocketParity',
  },
  private: {
    legacyUrl: 'wss://wss.woox.io/v2/ws/private/stream/:applicationId',
    v3Url: `${WOO_V3_WS_URL.private.livenet}?key=:listenKey`,
    listenKeyEndpoint: WOO_V3_ENDPOINTS.ACCOUNT_LISTEN_KEY,
    authentication: 'listen-key-query',
    gate: 'privateWebsocketParity',
  },
} as const;

export type WooV3MigrationRoute = Readonly<{
  id: string;
  legacyMethod: WooV3HttpMethod;
  legacyPath: string;
  v3Method: WooV3HttpMethod;
  v3Path: WooV3Endpoint;
  gate: WooV3MigrationGate;
  consumer: string;
  /** Omitted routes are real legacy-to-V3 migrations with an active owner. */
  runtimeLifecycle?: 'already-v3' | 'ownerless';
}>;

/**
 * Explicit legacy-to-V3 route mapping for every current Woo runtime consumer.
 * Repeated legacy paths are intentional where HTTP methods have different
 * migration semantics.
 */
export const WOO_V3_MIGRATION_ROUTES = [
  {
    id: 'markets',
    legacyMethod: 'GET',
    legacyPath: '/v1/public/info',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.PUBLIC_INSTRUMENTS,
    gate: 'publicReadParity',
    consumer: 'fetchMarkets',
  },
  {
    id: 'tickers',
    legacyMethod: 'GET',
    legacyPath: '/v1/public/futures',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.PUBLIC_FUTURES,
    gate: 'publicReadParity',
    consumer: 'fetchTickersRaw',
  },
  {
    id: 'orderbook',
    legacyMethod: 'GET',
    legacyPath: '/v1/public/orderbook/:symbol',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.PUBLIC_ORDERBOOK,
    gate: 'publicReadParity',
    consumer: 'fetchOrderBook',
  },
  {
    id: 'recent-klines',
    legacyMethod: 'GET',
    legacyPath: '/v1/kline',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.PUBLIC_KLINE,
    gate: 'publicReadParity',
    consumer: 'fetchOHLCV',
  },
  {
    id: 'historical-klines',
    legacyMethod: 'GET',
    legacyPath: 'https://api-pub.woox.io/v1/hist/kline',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.PUBLIC_KLINE_HISTORY,
    gate: 'publicReadParity',
    consumer: 'fetchOHLCV',
  },
  {
    id: 'place-order',
    legacyMethod: 'POST',
    legacyPath: '/v1/order',
    v3Method: 'POST',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDER,
    gate: 'regularWriteDemoLifecycle',
    consumer: 'placeOrder',
  },
  {
    id: 'cancel-order',
    legacyMethod: 'DELETE',
    legacyPath: '/v1/order',
    v3Method: 'DELETE',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDER,
    gate: 'regularWriteDemoLifecycle',
    consumer: 'cancelOrder',
  },
  {
    id: 'fetch-orders',
    legacyMethod: 'GET',
    legacyPath: '/v1/orders',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDERS,
    gate: 'privateReadParity',
    consumer: 'fetchLimitOrders',
  },
  {
    id: 'cancel-symbol-orders',
    legacyMethod: 'DELETE',
    legacyPath: '/v1/orders',
    v3Method: 'DELETE',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDERS,
    gate: 'regularWriteDemoLifecycle',
    consumer: 'cancelSymbolOrders',
  },
  {
    id: 'cancel-all-orders',
    legacyMethod: 'DELETE',
    legacyPath: '/v3/orders/pending',
    v3Method: 'DELETE',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDERS,
    gate: 'regularWriteDemoLifecycle',
    consumer: 'cancelAllOrders',
  },
  {
    id: 'amend-order',
    legacyMethod: 'PUT',
    legacyPath: '/v3/order/:orderId',
    v3Method: 'PUT',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ORDER,
    gate: 'regularWriteDemoLifecycle',
    consumer: 'updateOrder',
  },
  {
    id: 'place-algo-order',
    legacyMethod: 'POST',
    legacyPath: '/v3/algo/order',
    v3Method: 'POST',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER,
    gate: 'algoWriteDemoLifecycle',
    consumer: 'placeAlgoOrder',
  },
  {
    id: 'amend-algo-order',
    legacyMethod: 'PUT',
    legacyPath: '/v3/algo/order/:orderId',
    v3Method: 'PUT',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER,
    gate: 'algoWriteDemoLifecycle',
    consumer: 'updateOrder',
  },
  {
    id: 'cancel-algo-order',
    legacyMethod: 'DELETE',
    legacyPath: '/v3/algo/order/:orderId',
    v3Method: 'DELETE',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER,
    gate: 'algoWriteDemoLifecycle',
    consumer: 'cancelAlgoOrder',
  },
  {
    id: 'fetch-algo-orders',
    legacyMethod: 'GET',
    legacyPath: '/v3/algo/orders',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDERS,
    gate: 'privateReadParity',
    consumer: 'fetchAlgoOrders',
  },
  {
    id: 'cancel-all-algo-orders',
    legacyMethod: 'DELETE',
    legacyPath: '/v3/algo/orders/pending',
    v3Method: 'DELETE',
    v3Path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDERS,
    gate: 'algoWriteDemoLifecycle',
    consumer: 'no active safe-cex runtime owner',
    runtimeLifecycle: 'ownerless',
  },
  {
    id: 'cancel-all-after',
    legacyMethod: 'POST',
    legacyPath: '/v1/order/cancel_all_after',
    v3Method: 'POST',
    v3Path: WOO_V3_ENDPOINTS.TRADE_CANCEL_ALL_AFTER,
    gate: 'cancelAllAfterDemoLifecycle',
    consumer: 'dead-man-switch (not currently owned by safe-cex runtime)',
    runtimeLifecycle: 'ownerless',
  },
  {
    id: 'recent-trades',
    legacyMethod: 'GET',
    legacyPath: '/v1/client/trades',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.TRANSACTION_HISTORY,
    gate: 'privateReadParity',
    consumer: 'fetchHistoricalFills',
  },
  {
    id: 'historical-trades',
    legacyMethod: 'GET',
    legacyPath: '/v1/client/hist_trades',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.TRANSACTION_HISTORY,
    gate: 'privateReadParity',
    consumer: 'fetchHistoricalTrades',
  },
  {
    id: 'transaction-history',
    legacyMethod: 'GET',
    legacyPath: '/v3/trade/transactionHistory',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.TRANSACTION_HISTORY,
    gate: 'privateReadParity',
    consumer: 'fetchHistoricalFills',
    runtimeLifecycle: 'already-v3',
  },
  {
    id: 'account-info',
    legacyMethod: 'GET',
    legacyPath: '/v3/accountinfo',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.ACCOUNT_INFO,
    gate: 'privateReadParity',
    consumer: 'fetchBalance/validateAccount',
  },
  {
    id: 'balances',
    legacyMethod: 'GET',
    legacyPath: '/v3/balances',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.ASSET_BALANCES,
    gate: 'privateReadParity',
    consumer: 'no active safe-cex runtime owner; fetchBalance uses account-info',
    runtimeLifecycle: 'ownerless',
  },
  {
    id: 'positions',
    legacyMethod: 'GET',
    legacyPath: '/v3/positions',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.FUTURES_POSITIONS,
    gate: 'privateReadParity',
    consumer: 'fetchPositions',
  },
  {
    id: 'position-mode',
    legacyMethod: 'POST',
    legacyPath: '/v1/client/position_mode',
    v3Method: 'PUT',
    v3Path: WOO_V3_ENDPOINTS.FUTURES_POSITION_MODE,
    gate: 'accountWriteDemoLifecycle',
    consumer: 'changePositionMode',
  },
  {
    id: 'fetch-futures-leverage',
    legacyMethod: 'GET',
    legacyPath: '/v1/client/futures_leverage',
    v3Method: 'GET',
    v3Path: WOO_V3_ENDPOINTS.FUTURES_LEVERAGE,
    gate: 'privateReadParity',
    consumer: 'getLeverage',
  },
  {
    id: 'set-futures-leverage',
    legacyMethod: 'POST',
    legacyPath: '/v1/client/futures_leverage',
    v3Method: 'PUT',
    v3Path: WOO_V3_ENDPOINTS.FUTURES_LEVERAGE,
    gate: 'accountWriteDemoLifecycle',
    consumer: 'setLeverage',
  },
] as const satisfies readonly WooV3MigrationRoute[];

export type WooV3Decimal = string;

export type WooV3Response<T> = Readonly<{
  success: boolean;
  data: T;
  timestamp: number;
}>;

export type WooV3PageMeta = Readonly<{
  total: number;
  recordsPerPage: number;
  currentPage: number;
}>;

export type WooV3Rows<T> = Readonly<{
  rows: readonly T[];
  meta?: WooV3PageMeta;
}>;

export type WooV3Instrument = Readonly<{
  symbol: string;
  status?: string;
  baseAsset: string;
  quoteAsset: string;
  baseAssetMultiplier?: WooV3Decimal | number;
  baseMin: WooV3Decimal | number;
  baseMax: WooV3Decimal | number;
  baseTick: WooV3Decimal | number;
  quoteTick: WooV3Decimal | number;
  minNotional: WooV3Decimal | number;
  baseIMR: WooV3Decimal | number;
  fundingIntervalHours?: number;
  isAllowedRpi?: boolean;
}>;

export type WooV3Balance = Readonly<{
  token: string;
  holding: WooV3Decimal;
  frozen: WooV3Decimal;
  availableBalance: WooV3Decimal;
  pendingShortQty: WooV3Decimal;
  pendingLongQty: WooV3Decimal;
  averageOpenPrice: WooV3Decimal;
  markPrice: WooV3Decimal;
  timestamp: number;
}>;

export type WooV3Position = Readonly<{
  symbol: string;
  holding: WooV3Decimal;
  positionSide: 'BOTH' | 'LONG' | 'SHORT';
  pendingLongQty: WooV3Decimal;
  pendingShortQty: WooV3Decimal;
  settlePrice: WooV3Decimal;
  averageOpenPrice: WooV3Decimal;
  markPrice: WooV3Decimal;
  estLiqPrice: WooV3Decimal;
  timestamp: number;
}>;

export type WooV3AccountInfo = Readonly<{
  applicationId: string;
  account: string;
  alias: string;
  accountMode: string;
  positionMode: 'ONE_WAY' | 'HEDGE_MODE';
  leverage: number;
  marginRatio: WooV3Decimal;
  totalCollateral: WooV3Decimal;
  totalAccountValue: WooV3Decimal;
  accountType: string;
}>;

export type WooV3PlaceOrderRequest = Readonly<{
  symbol: string;
  side: 'BUY' | 'SELL';
  type: 'LIMIT' | 'MARKET' | 'IOC' | 'FOK' | 'POST_ONLY' | 'ASK' | 'BID' | 'AC' | 'RPI';
  clientOrderId?: number;
  orderTag?: string;
  positionSide?: 'BOTH' | 'LONG' | 'SHORT';
  price?: WooV3Decimal;
  quantity?: WooV3Decimal;
  amount?: WooV3Decimal;
  reduceOnly?: boolean;
  visibleQuantity?: WooV3Decimal;
  marginMode?: 'CROSS' | 'ISOLATED';
  bidAskLevel?: '1' | '2' | '3' | '4' | '5';
  postOnlyAdjusted?: boolean;
}>;

export type WooV3PlaceOrderResult = Readonly<{
  orderId: number;
  type: string;
  price: WooV3Decimal;
  quantity: WooV3Decimal;
  amount: WooV3Decimal;
  clientOrderId: number;
  bidAskLevel: number;
}>;

export type WooV3ListenKeyRequest = Readonly<{
  type: 'WEBSOCKET';
}>;

export type WooV3ListenKeyResult = Readonly<{
  authKey: string;
  expiredTime: number;
}>;
