// dYdX v4 Type Definitions

export const BASE_URLS = {
  PRODUCTION: 'https://indexer.dydx.trade',
  TESTNET: 'https://indexer.v4testnet.dydx.exchange',
};

export const VALIDATOR_URLS = {
  PRODUCTION: 'https://dydx-ops-rpc.kingnodes.com:443',
  TESTNET: 'https://test-dydx-rpc.kingnodes.com',
};

export const WSS_URLS = {
  PRODUCTION: 'wss://indexer.dydx.trade/v4/ws',
  TESTNET: 'wss://indexer.v4testnet.dydx.exchange/v4/ws',
};

export const CHAIN_IDS = {
  PRODUCTION: 'dydx-mainnet-1',
  TESTNET: 'dydx-testnet-4',
};

// REST API Endpoints
export const ENDPOINTS = {
  // Markets
  PERPETUAL_MARKETS: '/v4/perpetualMarkets',
  ORDERBOOK: '/v4/orderbooks/perpetualMarket',
  TRADES: '/v4/trades/perpetualMarket',
  CANDLES: '/v4/candles/perpetualMarkets',
  HISTORICAL_FUNDING: '/v4/historicalFunding',

  // Account
  ADDRESSES: '/v4/addresses',
  PERPETUAL_POSITIONS: '/v4/perpetualPositions',
  ASSET_POSITIONS: '/v4/assetPositions',
  ORDERS: '/v4/orders',
  FILLS: '/v4/fills',
  TRANSFERS: '/v4/transfers',
  HISTORICAL_PNL: '/v4/historicalPnl',
  FUNDING_PAYMENTS: '/v4/historical-funding',

  // Utility
  TIME: '/v4/time',
  HEIGHT: '/v4/height',
  COMPLIANCE: '/v4/screen',
};

// Grafana monitoring endpoints
export const GRAFANA_ENDPOINTS_EVERY: string[] = [];
export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.PERPETUAL_MARKETS,
  ENDPOINTS.PERPETUAL_POSITIONS,
  ENDPOINTS.ASSET_POSITIONS,
  ENDPOINTS.ORDERS,
];

// WebSocket Channels
export const WS_CHANNELS = {
  MARKETS: 'v4_markets',
  TRADES: 'v4_trades',
  ORDERBOOK: 'v4_orderbook',
  CANDLES: 'v4_candles',
  SUBACCOUNT: 'v4_subaccounts',
  BLOCK_HEIGHT: 'v4_block_height',
  PARENT_SUBACCOUNTS: 'v4_parent_subaccounts',
};

// Candle Resolutions
export enum CandleResolution {
  ONE_MIN = '1MIN',
  FIVE_MINS = '5MINS',
  FIFTEEN_MINS = '15MINS',
  THIRTY_MINS = '30MINS',
  ONE_HOUR = '1HOUR',
  FOUR_HOURS = '4HOURS',
  ONE_DAY = '1DAY',
}

// Order Side (from SDK)
export enum DydxOrderSide {
  BUY = 'BUY',
  SELL = 'SELL',
}

// Order Type (from SDK)
export enum DydxOrderType {
  LIMIT = 'LIMIT',
  MARKET = 'MARKET',
  STOP_LIMIT = 'STOP_LIMIT',
  STOP_MARKET = 'STOP_MARKET',
  TAKE_PROFIT_LIMIT = 'TAKE_PROFIT_LIMIT',
  TAKE_PROFIT_MARKET = 'TAKE_PROFIT_MARKET',
}

// Time In Force (from SDK)
export enum DydxTimeInForce {
  GTT = 'GTT', // Good Till Time
  IOC = 'IOC', // Immediate or Cancel
  FOK = 'FOK', // Fill or Kill
}

// Order Flags (from SDK)
export enum DydxOrderFlags {
  SHORT_TERM = 0, // In-memory orders, up to 20 blocks
  LONG_TERM = 64, // Stateful orders on blockchain
  CONDITIONAL = 32, // Conditional/trigger orders
}

// Order Status
export enum DydxOrderStatus {
  OPEN = 'OPEN',
  FILLED = 'FILLED',
  CANCELED = 'CANCELED',
  BEST_EFFORT_CANCELED = 'BEST_EFFORT_CANCELED',
  UNTRIGGERED = 'UNTRIGGERED',
  PARTIALLY_FILLED = 'PARTIALLY_FILLED',
}

// Position Side
export enum DydxPositionSide {
  LONG = 'LONG',
  SHORT = 'SHORT',
  NONE = 'NONE',
}

// Asset Types
export const ASSET_TYPES = {
  USDC: 'ibc/8E27BA2D5493AF5636760E354E46004562C46AB7EC0CC4C1CA14E9E20E2545B5',
  NATIVE_TOKEN_MAINNET: 'adydx',
  NATIVE_TOKEN_TESTNET: 'adv4tnt',
};

// API Response Types

export interface DydxMarket {
  ticker: string;
  marketId: number;
  clobPairId: string;
  baseAsset: string;
  quoteAsset: string;
  status: string;
  priceChange24H: string;
  volume24H: string;
  trades24H: number;
  nextFundingRate: string;
  initialMarginFraction: string;
  maintenanceMarginFraction: string;
  openInterest: string;
  atomicResolution: number;
  quantumConversionExponent: number;
  tickSize: string;
  stepSize: string;
  stepBaseQuantums: number;
  subticksPerTick: number;
}

export interface DydxMarketsResponse {
  markets: {
    [ticker: string]: DydxMarket;
  };
}

export interface DydxTicker {
  ticker: string;
  price: string;
  size: string;
}

export interface DydxOrderbookLevel {
  price: string;
  size: string;
}

export interface DydxOrderbook {
  bids: DydxOrderbookLevel[];
  asks: DydxOrderbookLevel[];
}

export interface DydxTrade {
  id: string;
  side: DydxOrderSide;
  size: string;
  price: string;
  type: string;
  createdAt: string;
  createdAtHeight: string;
}

export interface DydxCandle {
  startedAt: string;
  ticker: string;
  resolution: string;
  low: string;
  high: string;
  open: string;
  close: string;
  baseTokenVolume: string;
  usdVolume: string;
  trades: number;
  startingOpenInterest: string;
}

export interface DydxSubaccount {
  address: string;
  subaccountNumber: number;
  equity: string;
  freeCollateral: string;
  openPerpetualPositions: {
    [market: string]: DydxPerpetualPosition;
  };
  assetPositions: {
    [asset: string]: DydxAssetPosition;
  };
  marginEnabled: boolean;
}

export interface DydxPerpetualPosition {
  market: string;
  status: string;
  side: DydxPositionSide;
  size: string;
  maxSize: string;
  entryPrice: string;
  realizedPnl: string;
  createdAt: string;
  createdAtHeight: string;
  sumOpen: string;
  sumClose: string;
  netFunding: string;
  unrealizedPnl: string;
  closedAt?: string;
  exitPrice?: string;
}

export interface DydxAssetPosition {
  symbol: string;
  side: string;
  size: string;
  assetId: string;
}

export interface DydxOrder {
  id: string;
  subaccountId: string;
  clientId: string;
  clobPairId: string;
  side: DydxOrderSide;
  size: string;
  totalFilled: string;
  price: string;
  type: DydxOrderType;
  status: DydxOrderStatus;
  timeInForce: DydxTimeInForce;
  postOnly: boolean;
  reduceOnly: boolean;
  orderFlags: string;
  goodTilBlock?: string;
  goodTilBlockTime?: string;
  createdAtHeight: string;
  clientMetadata: string;
  triggerPrice?: string;
  updatedAt?: string;
  updatedAtHeight?: string;
}

export interface DydxFill {
  id: string;
  side: DydxOrderSide;
  liquidity: string;
  type: string;
  market: string;
  marketType: string;
  price: string;
  size: string;
  fee: string;
  createdAt: string;
  createdAtHeight: string;
  orderId?: string;
  clientMetadata?: string;
}

export interface DydxTransfer {
  id: string;
  sender: {
    address: string;
    subaccountNumber: number;
  };
  recipient: {
    address: string;
    subaccountNumber: number;
  };
  size: string;
  createdAt: string;
  createdAtHeight: string;
  symbol: string;
  type: string;
  transactionHash?: string;
}

export interface DydxHistoricalPnl {
  id: string;
  subaccountId: string;
  equity: string;
  totalPnl: string;
  netTransfers: string;
  createdAt: string;
  blockHeight: string;
  blockTime: string;
}

export interface DydxFundingPayment {
  market: string;
  payment: string;
  rate: string;
  positionSize: string;
  price: string;
  effectiveAt: string;
}

// WebSocket Message Types

export interface DydxWsSubscribeMessage {
  type: 'subscribe' | 'unsubscribe';
  channel: string;
  id?: string;
  batched?: boolean;
}

export interface DydxWsChannelData {
  type: 'channel_data';
  connection_id: string;
  message_id: number;
  channel: string;
  id?: string;
  contents: any;
}

export interface DydxWsChannelBatchData {
  type: 'channel_batch_data';
  connection_id: string;
  message_id: number;
  channel: string;
  id?: string;
  contents: any[];
}

export interface DydxWsMarketsUpdate {
  trading: {
    [ticker: string]: {
      oraclePrice: string;
      priceChange24H: string;
      volume24H: string;
      trades24H: number;
      nextFundingRate: string;
      openInterest: string;
    };
  };
}

export interface DydxWsTradesUpdate {
  trades: DydxTrade[];
}

export interface DydxWsOrderbookUpdate {
  bids: DydxOrderbookLevel[];
  asks: DydxOrderbookLevel[];
}

export interface DydxWsCandlesUpdate {
  candles: DydxCandle[];
}

export interface DydxWsSubaccountUpdate {
  orders?: DydxOrder[];
  fills?: DydxFill[];
  positions?: DydxPerpetualPosition[];
  subaccount?: Partial<DydxSubaccount>;
}

// SDK Client Types (we'll use these internally)
export interface DydxClientConfig {
  address: string; // dydx wallet address
  privateKey: string; // private key for signing
  subaccountNumber: number; // default subaccount (usually 0)
  testnet: boolean;
}

export interface DydxPlaceOrderParams {
  market: string;
  side: DydxOrderSide;
  type: DydxOrderType;
  price: number;
  size: number;
  clientId: number;
  timeInForce: DydxTimeInForce;
  postOnly: boolean;
  reduceOnly: boolean;
  triggerPrice?: number;
  goodTilBlock?: number;
  goodTilTime?: number;
}

export interface DydxCancelOrderParams {
  orderId?: string;
  clientId?: number;
  clobPairId: string;
  orderFlags: DydxOrderFlags;
  goodTilBlock?: number;
  goodTilBlockTime?: number;
}
