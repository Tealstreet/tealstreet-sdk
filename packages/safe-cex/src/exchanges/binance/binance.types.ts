import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://fapi.binance.com',
  testnet: 'https://testnet.binancefuture.com',
};

// Binance USDⓈ-M Futures WebSocket URLs.
// As of 2026-04-23, the legacy single `/ws` endpoint is decommissioned. Streams must
// be routed by category to one of three dedicated base URLs:
//   - /public/ws  -> high-frequency public data: bookTicker, @depth, rpiDepth
//   - /market/ws  -> regular market data: ticker, markPrice, kline, aggTrade, forceOrder, ...
//   - /private/ws -> user data via `?listenKey=...` query param (and optional `&events=...`)
// See: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Important-WebSocket-Change-Notice
export const BASE_WS_URL = {
  public: {
    livenet: 'wss://fstream.binance.com/public/ws',
    testnet: 'wss://stream.binancefuture.com/public/ws',
  },
  market: {
    livenet: 'wss://fstream.binance.com/market/ws',
    testnet: 'wss://stream.binancefuture.com/market/ws',
  },
  private: {
    livenet: 'wss://fstream.binance.com/private/ws',
    testnet: 'wss://stream.binancefuture.com/private/ws',
  },
};

export const ENDPOINTS = {
  BALANCE: '/fapi/v2/balance',
  MARKETS: '/fapi/v1/exchangeInfo',
  ACCOUNT: '/fapi/v2/account',
  LEVERAGE_BRACKET: '/fapi/v1/leverageBracket',
  TICKERS_24H: '/fapi/v1/ticker/24hr',
  TICKERS_BOOK: '/fapi/v1/ticker/bookTicker',
  TICKERS_PRICE: '/fapi/v1/premiumIndex',
  HEDGE_MODE: '/fapi/v1/positionSide/dual',
  SET_LEVERAGE: '/fapi/v1/leverage',
  OPEN_ORDERS: '/fapi/v1/openOrders',
  CANCEL_SYMBOL_ORDERS: '/fapi/v1/allOpenOrders',
  ORDER: '/fapi/v1/order',
  BATCH_ORDERS: '/fapi/v1/batchOrders',
  KLINE: '/fapi/v1/klines',
  LISTEN_KEY: '/fapi/v1/listenKey',
  ORDERBOOK: '/fapi/v1/depth',
  USER_TRADES: '/fapi/v1/userTrades',
  SET_MARGIN_TYPE: '/fapi/v1/marginType',
  // migrating to v3 requires fake positions as in portfolio margin implementation
  POSITION_RISK: '/fapi/v2/positionRisk',
  TRADES: '/fapi/v1/trades',
  // Algo Order endpoints (for STOP_MARKET, TAKE_PROFIT_MARKET, etc. as of 2025-12-09)
  ALGO_ORDER: '/fapi/v1/algoOrder',
  OPEN_ALGO_ORDERS: '/fapi/v1/openAlgoOrders',
  // Income history (realized PnL, funding fees, commissions)
  INCOME: '/fapi/v1/income',
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

// Income types returned by /fapi/v1/income endpoint
export type BinanceIncomeType =
  | 'TRANSFER'
  | 'WELCOME_BONUS'
  | 'REALIZED_PNL'
  | 'FUNDING_FEE'
  | 'COMMISSION'
  | 'INSURANCE_CLEAR'
  | 'REFERRAL_KICKBACK'
  | 'COMMISSION_REBATE'
  | 'API_REBATE'
  | 'CONTEST_REWARD'
  | 'CROSS_COLLATERAL_TRANSFER'
  | 'OPTIONS_PREMIUM_FEE'
  | 'OPTIONS_SETTLE_PROFIT'
  | 'INTERNAL_TRANSFER'
  | 'AUTO_EXCHANGE'
  | 'DELIVERED_SETTELMENT'
  | 'COIN_SWAP_DEPOSIT'
  | 'COIN_SWAP_WITHDRAW'
  | 'POSITION_LIMIT_INCREASE_FEE';

export type BinanceIncomeRecord = {
  symbol: string;
  incomeType: BinanceIncomeType;
  income: string;
  asset: string;
  info?: string;
  time: number;
  tranId: string;
  tradeId?: string;
};

// Binance-specific income history options (extends the shared type with specific incomeType)
export type BinanceIncomeHistoryOptions = {
  symbol?: string;
  incomeType?: BinanceIncomeType;
  startTime?: number;
  endTime?: number;
  limit?: number; // Max 1000, default 100
};
