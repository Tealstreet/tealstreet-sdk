import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BASE_PAPI_URL = {
  livenet: 'https://papi.binance.com',
  testnet: 'https://demo-fapi.binance.com',
};

export const BASE_FAPI_URL = {
  livenet: 'https://fapi.binance.com',
  testnet: 'https://demo-fapi.binance.com',
};

// Binance Portfolio Margin WebSocket URLs.
// PM shares Binance's USDⓈ-M public market data feed — and therefore inherits
// the 2026-04-23 base URL split:
//   - /public/ws  -> bookTicker, @depth (high-frequency)
//   - /market/ws  -> ticker, markPrice, kline (regular market data)
// PM's user-data feed lives on the dedicated /pm/ws/{listenKey} endpoint and is
// NOT affected by the split (per CCXT verification, ccxt/ccxt#28091).
// See: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Important-WebSocket-Change-Notice
export const BASE_WS_URL = {
  public: {
    livenet: 'wss://fstream.binance.com/public/ws',
    testnet: 'wss://demo-fstream.binance.com/public/ws',
  },
  market: {
    livenet: 'wss://fstream.binance.com/market/ws',
    testnet: 'wss://demo-fstream.binance.com/market/ws',
  },
  private: {
    livenet: 'wss://fstream.binance.com/pm/ws',
    testnet: 'wss://demo-fstream.binance.com/pm/ws',
  },
};

export const ENDPOINTS = {
  BALANCE: '/papi/v1/balance',
  MARKETS: '/fapi/v1/exchangeInfo',
  ACCOUNT: '/papi/v1/account',
  LEVERAGE_BRACKET: '/papi/v1/um/leverageBracket',
  TICKERS_24H: '/fapi/v1/ticker/24hr',
  TICKERS_BOOK: '/fapi/v1/ticker/bookTicker',
  TICKERS_PRICE: '/fapi/v1/premiumIndex',
  HEDGE_MODE: '/papi/v1/um/positionSide/dual',
  SET_LEVERAGE: '/papi/v1/um/leverage',
  OPEN_ORDERS: '/papi/v1/um/openOrders',
  OPEN_CONDITIONAL_ORDERS: '/papi/v1/um/conditional/openOrders',
  CANCEL_SYMBOL_ORDERS: '/papi/v1/um/allOpenOrders',
  CANCEL_SYMBOL_CONDITIONAL_ORDERS: '/papi/v1/um/conditional/allOpenOrders',
  ORDER: '/papi/v1/um/order',
  CONDITIONAL_ORDER: '/papi/v1/um/conditional/order',
  KLINE: '/fapi/v1/klines',
  LISTEN_KEY: '/papi/v1/listenKey',
  ORDERBOOK: '/fapi/v1/depth',
  USER_TRADES: '/papi/v1/um/userTrades',
  POSITION_RISK: '/papi/v1/um/positionRisk',
};

export const PUBLIC_ENDPOINTS = [
  ENDPOINTS.MARKETS,
  ENDPOINTS.TICKERS_24H,
  ENDPOINTS.TICKERS_BOOK,
  ENDPOINTS.TICKERS_PRICE,
  ENDPOINTS.KLINE,
  ENDPOINTS.ORDERBOOK,
];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.HEDGE_MODE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.BALANCE,
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.LEVERAGE_BRACKET,
  ENDPOINTS.OPEN_ORDERS,
  ENDPOINTS.POSITION_RISK,
];

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP_MARKET: OrderType.StopLoss,
  TAKE_PROFIT_MARKET: OrderType.TakeProfit,
  TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  LONG: PositionSide.Long,
  SHORT: PositionSide.Short,
};

export const TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  GTX: OrderTimeInForce.PostOnly,
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
  CONTRACT_PRICE: OrderStopTriggerType.LastPrice,
};

// [
//   {
//     "entryPrice": "0.00000",
//     "breakEvenPrice": "0.0",
//     "marginType": "isolated",
//     "isAutoAddMargin": "false",
//     "isolatedMargin": "0.00000000",
//     "leverage": "10",
//     "liquidationPrice": "0",
//     "markPrice": "6679.50671178",
//     "maxNotionalValue": "20000000",
//     "positionAmt": "0.000",
//     "notional": "0",,
//   "isolatedWallet": "0",
//   "symbol": "BTCUSDT",
//   "unRealizedProfit": "0.00000000",
//   "positionSide": "BOTH",
//   "updateTime": 0
// }
// ]

// {
//   "symbol": "CTSIUSDT",
//   "positionAmt": "0",
//   "entryPrice": "0.0",
//   "breakEvenPrice": "0.0",
//   "markPrice": "0.00000000",
//   "unRealizedProfit": "0.00000000",
//   "liquidationPrice": "0",
//   "leverage": "20",
//   "maxNotionalValue": "25000",
//   "marginType": "cross",
//   "isolatedMargin": "0.00000000",
//   "isAutoAddMargin": "false",
//   "positionSide": "SHORT",
//   "notional": "0",
//   "isolatedWallet": "0",
//   "updateTime": 0,
//   "isolated": false,
//   "adlQuantile": 0
// }
export type GET_POSITION_RISK_RESPONSE = {
  symbol: string;
  positionAmt: string;
  entryPrice: string;
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
  isolated: boolean;
  adlQuantile: number;
}[];
