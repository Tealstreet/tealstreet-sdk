import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://dapi.binance.com',
  testnet: 'https://demo-dapi.binance.com',
};

export const BASE_WS_URL = {
  livenet: 'wss://dstream.binance.com/ws',
  testnet: 'wss://dstream.binancefuture.com/ws',
};

export const ENDPOINTS = {
  BALANCE: '/dapi/v1/balance',
  MARKETS: '/dapi/v1/exchangeInfo',
  ACCOUNT: '/dapi/v1/account',
  LEVERAGE_BRACKET: '/dapi/v2/leverageBracket',
  TICKERS_24H: '/dapi/v1/ticker/24hr',
  TICKERS_BOOK: '/dapi/v1/ticker/bookTicker',
  TICKERS_PRICE: '/dapi/v1/premiumIndex',
  HEDGE_MODE: '/dapi/v1/positionSide/dual',
  SET_LEVERAGE: '/dapi/v1/leverage',
  OPEN_ORDERS: '/dapi/v1/openOrders',
  CANCEL_SYMBOL_ORDERS: '/dapi/v1/allOpenOrders',
  ORDER: '/dapi/v1/order',
  BATCH_ORDERS: '/dapi/v1/batchOrders',
  KLINE: '/dapi/v1/klines',
  LISTEN_KEY: '/dapi/v1/listenKey',
  ORDERBOOK: '/dapi/v1/depth',
  ALL_ORDERS: '/dapi/v1/allOrders',
  USER_TRADES: '/dapi/v1/userTrades',
  SET_MARGIN_TYPE: '/dapi/v1/marginType',
  POSITION_RISK: '/dapi/v1/positionRisk',
  TRADES: '/dapi/v1/trades',
  INCOME: '/dapi/v1/income',
};

export const PUBLIC_ENDPOINTS = [
  ENDPOINTS.MARKETS,
  ENDPOINTS.TICKERS_24H,
  ENDPOINTS.TICKERS_BOOK,
  ENDPOINTS.TICKERS_PRICE,
  ENDPOINTS.KLINE,
  ENDPOINTS.ORDERBOOK,
  ENDPOINTS.TRADES,
];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.SET_MARGIN_TYPE,
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

export type BinanceInversePositionRisk = {
  symbol: string;
  positionAmt: string;
  entryPrice: string;
  markPrice: string;
  unRealizedProfit: string;
  liquidationPrice: string;
  leverage?: string;
  maxQty?: string;
  marginType?: string;
  isolatedMargin: string;
  positionSide: string;
  updateTime: number;
};

export type GET_POSITION_RISK_RESPONSE = BinanceInversePositionRisk[];

export type BinanceIncomeRecord = {
  symbol: string;
  incomeType: string;
  income: string;
  asset: string;
  info?: string;
  time: number;
  tranId: string;
  tradeId?: string;
};
