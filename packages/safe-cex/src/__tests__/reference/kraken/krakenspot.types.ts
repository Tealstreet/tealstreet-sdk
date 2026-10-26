import { OrderSide, OrderTimeInForce, OrderType, Timeframe } from '../../../types';

// Kraken Spot REST API (completely separate from Futures API)
export const BASE_URL = {
  livenet: 'https://api.kraken.com',
  testnet: 'https://api.kraken.com', // Kraken Spot has no testnet
};

// Kraken Spot WebSocket v2
export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws.kraken.com/v2',
    testnet: 'wss://ws.kraken.com/v2',
  },
  private: {
    livenet: 'wss://ws-auth.kraken.com/v2',
    testnet: 'wss://ws-auth.kraken.com/v2',
  },
};

export const ENDPOINTS = {
  // Public (GET)
  ASSET_PAIRS: '/0/public/AssetPairs',
  TICKER: '/0/public/Ticker',
  OHLC: '/0/public/OHLC',
  ORDERBOOK: '/0/public/Depth',
  TRADES: '/0/public/Trades',
  SERVER_TIME: '/0/public/Time',

  // Private (POST)
  BALANCE: '/0/private/Balance',
  EXTENDED_BALANCE: '/0/private/BalanceEx',
  TRADE_BALANCE: '/0/private/TradeBalance',
  OPEN_ORDERS: '/0/private/OpenOrders',
  CLOSED_ORDERS: '/0/private/ClosedOrders',
  ADD_ORDER: '/0/private/AddOrder',
  AMEND_ORDER: '/0/private/AmendOrder',
  CANCEL_ORDER: '/0/private/CancelOrder',
  CANCEL_ALL: '/0/private/CancelAll',
  TRADES_HISTORY: '/0/private/TradesHistory',
  OPEN_POSITIONS: '/0/private/OpenPositions',
  WS_TOKEN: '/0/private/GetWebSocketsToken',
};

export const PUBLIC_ENDPOINTS = [
  ENDPOINTS.ASSET_PAIRS,
  ENDPOINTS.TICKER,
  ENDPOINTS.OHLC,
  ENDPOINTS.ORDERBOOK,
  ENDPOINTS.TRADES,
  ENDPOINTS.SERVER_TIME,
];

export const GRAFANA_ENDPOINTS_EVERY = [ENDPOINTS.ADD_ORDER, ENDPOINTS.AMEND_ORDER, ENDPOINTS.CANCEL_ORDER];

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.BALANCE, ENDPOINTS.OPEN_ORDERS, ENDPOINTS.TRADE_BALANCE];

// Kraken Spot uses minute-based intervals for OHLC
export const INTERVAL: Record<Timeframe, number> = {
  '1m': 1,
  '3m': 1, // Not supported, use 1m
  '5m': 5,
  '15m': 15,
  '30m': 30,
  '1h': 60,
  '2h': 60, // Not supported, use 1h
  '4h': 240,
  '6h': 240, // Not supported, use 4h
  '12h': 240, // Not supported; use the nearest valid Kraken interval (4h)
  '1d': 1440,
  '1w': 10080,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  limit: OrderType.Limit,
  market: OrderType.Market,
  'stop-loss': OrderType.StopLoss,
  'take-profit': OrderType.TakeProfit,
  'stop-loss-limit': OrderType.StopLoss,
  'take-profit-limit': OrderType.TakeProfit,
  'trailing-stop': OrderType.TrailingStopLoss,
  'trailing-stop-limit': OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  buy: OrderSide.Buy,
  sell: OrderSide.Sell,
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  GTD: OrderTimeInForce.GoodTillCancel, // Map GTD to GTC (closest equivalent)
};

// Kraken uses non-standard asset names; map them to standard
export const ASSET_ALIASES: Record<string, string> = {
  XBT: 'BTC',
  XXBT: 'BTC',
  XDG: 'DOGE',
  XXDG: 'DOGE',
  XETH: 'ETH',
  XXRP: 'XRP',
  XLTC: 'LTC',
  XXLM: 'XLM',
  XXMR: 'XMR',
  XZEC: 'ZEC',
  XREP: 'REP',
  XETC: 'ETC',
  XDAO: 'DAO',
  XMLN: 'MLN',
  ZUSD: 'USD',
  ZEUR: 'EUR',
  ZGBP: 'GBP',
  ZJPY: 'JPY',
  ZCAD: 'CAD',
  ZAUD: 'AUD',
};

// Normalize Kraken asset codes to standard symbols
export function normalizeAsset(asset: string): string {
  return ASSET_ALIASES[asset] || asset;
}
