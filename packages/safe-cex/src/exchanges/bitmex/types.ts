import { Timeframe } from '../../types';

export const SupportedTimeframes: { [key in Timeframe]?: string } = {
  '1m': '1',
  '5m': '5',
  '1h': '60',
  '1d': '1D',
};

//
// {
//   "table": "trade",
//   "action": "insert",
//   "data": [
//   {
//     "timestamp": "2024-05-13T07:52:06.893Z",
//     "symbol": "XBTUSDT",
//     "side": "Sell",
//     "size": 46000,
//     "price": 61682.5,
//     "tickDirection": "ZeroPlusTick",
//     "trdMatchID": "00000000-006d-1000-0000-00076413fd51",
//     "grossValue": 2837395000,
//     "homeNotional": 0.046,
//     "foreignNotional": 2837.395,
//     "trdType": "Regular"
//   },
//   {
//     "timestamp": "2024-05-13T07:52:06.893Z",
//     "symbol": "XBTUSDT",
//     "side": "Sell",
//     "size": 1000,
//     "price": 61682.5,
//     "tickDirection": "ZeroPlusTick",
//     "trdMatchID": "00000000-006d-1000-0000-00076413fd54",
//     "grossValue": 61682500,
//     "homeNotional": 0.001,
//     "foreignNotional": 61.6825,
//     "trdType": "Regular"
//   },
//   {
//     "timestamp": "2024-05-13T07:52:06.893Z",
//     "symbol": "XBTUSDT",
//     "side": "Sell",
//     "size": 1000,
//     "price": 61682.5,
//     "tickDirection": "ZeroPlusTick",
//     "trdMatchID": "00000000-006d-1000-0000-00076413fd57",
//     "grossValue": 61682500,
//     "homeNotional": 0.001,
//     "foreignNotional": 61.6825,
//     "trdType": "Regular"
//   }
// ]
// }

export type TRADE_WS_UPDATE = {
  table: 'trade';
  action: 'insert' | 'snapshot';
  data: {
    timestamp: string;
    symbol: string;
    side: string;
    size: number;
    price: number;
    tickDirection: string;
    trdMatchID: string;
    grossValue: number;
    homeNotional: number;
    foreignNotional: number;
    trdType: string;
  }[];
};

// {
//   table: 'order',
//   action: 'insert',
//   data: [
//     {
//       orderID: '0f7406f5-a218-47a9-9f00-aba4c752a81e',
//       account: 2169568,
//       symbol: 'XBTUSD',
//       side: 'Buy',
//       orderQty: 300,
//       price: 60000.0,
//       currency: 'USD',
//       settlCurrency: 'XBt',
//       ordType: 'Limit',
//       timeInForce: 'GoodTillCancel',
//       ordStatus: 'New',
//       workingIndicator: true,
//       leavesQty: 300,
//       cumQty: 0,
//       text: 'Submission from www.bitmex.com',
//       transactTime: '2024-03-03T20:50:45.019Z',
//       timestamp: '2024-03-03T20:50:45.019Z',
//     },
//   ],
// };
// };

export type BITMEX_ORDER = {
  orderID: string;
  account: number;
  symbol: string;
  side: string;
  orderQty: number;
  price?: number;
  avgPx?: number;
  currency: string;
  settlCurrency: string;
  ordType: string;
  timeInForce: string;
  ordStatus: string;
  workingIndicator: boolean;
  leavesQty: number;
  cumQty: number;
  maxSlippagePct?: number | string;
  text: string;
  transactTime: string;
  timestamp: string;
};

export type ORDER_WS_UPDATE = {
  table: 'order';
  action: 'insert' | 'partial' | 'update' | 'delete';
  data: BITMEX_ORDER[];
};

// {
//   "account": 2232361,
//   "symbol": "XRPUSDT",
//   "currency": "USDt",
//   "rebalancedPnl": -1023452,
//   "openOrderBuyQty": 0,
//   "openOrderBuyCost": 0,
//   "currentQty": 366000,
//   "currentCost": 1912058000,
//   "unrealisedCost": 1912058000,
//   "markPrice": 0.52315,
//   "markValue": 1914729000,
//   "riskValue": 1914729000,
//   "homeNotional": 3660,
//   "foreignNotional": -1914.729,
//   "posCost": 1912058000,
//   "posComm": 1303399,
//   "posMargin": 20615185,
//   "posMaint": 20615185,
//   "initMargin": 0,
//   "maintMargin": 20615185,
//   "unrealisedPnl": 2671000,
//   "unrealisedPnlPcnt": 0.0014,
//   "unrealisedRoePcnt": 0.0698,
//   "avgCostPrice": 0.5224203,
//   "avgEntryPrice": 0.5224203,
//   "liquidationPrice": 0.4278,
//   "bankruptPrice": 0.4226,
//   "timestamp": "2024-05-18T07:14:35.961Z"
// }

export type BITMEX_POSITION = {
  account: number;
  symbol: string;
  currency: string;
  rebalancedPnl: number;
  openOrderBuyQty: number;
  openOrderBuyCost: number;
  currentQty: number;
  currentCost: number;
  unrealisedCost: number;
  markPrice: number;
  markValue: number;
  riskValue: number;
  homeNotional: number;
  foreignNotional: number;
  posCost: number;
  posComm: number;
  posMargin: number;
  posMaint: number;
  initMargin: number;
  maintMargin: number;
  realisedPnl?: number;
  realizedPnl?: number;
  unrealisedPnl: number;
  unrealisedPnlPcnt: number;
  unrealisedRoePcnt: number;
  avgCostPrice: number;
  avgEntryPrice: number;
  liquidationPrice: number;
  bankruptPrice: number;
  timestamp: string;
};

export type POSITION_WS_UPDATE = {
  table: 'position';
  action: 'insert' | 'partial' | 'update' | 'delete';
  data: BITMEX_POSITION[];
};

export type BITMEX_EXECUTION = {
  execID: string;
  orderID: string;
  symbol: string;
  side?: string;
  execType?: string;
  ordStatus?: string;
  lastQty?: number;
  lastPx?: number;
  price?: number;
  execComm?: number;
  execCommCcy?: string;
  currency?: string;
  settlCurrency?: string;
  maxSlippagePct?: number | string;
  realisedPnl?: number;
  realisedPnlCcy?: string;
  realizedPnl?: number;
  realizedPnlCcy?: string;
  closedPnl?: number;
  transactTime?: string;
  timestamp?: string;
};

export type BITMEX_MARKET = {
  symbol: string;
  rootSymbol: string;
  state: string;
  typ: string;
  listing: string;
  front: string;
  underlying: string;
  quoteCurrency: string;
  underlyingSymbol: string;
  reference: string;
  referenceSymbol: string;
  maxOrderQty: number;
  maxPrice: number;
  lotSize: number;
  tickSize: number;
  multiplier: number;
  settlCurrency: string;
  quoteToSettleMultiplier: number;
  isQuanto: boolean;
  isInverse: boolean;
  initMargin: number;
  maintMargin: number;
  riskLimit: number;
  riskStep: number;
  taxed: boolean;
  deleverage: boolean;
  makerFee: number;
  takerFee: number;
  settlementFee: number;
  fundingBaseSymbol: string;
  fundingQuoteSymbol: string;
  fundingPremiumSymbol: string;
  fundingTimestamp: string;
  fundingInterval: string;
  fundingRate: number;
  indicativeFundingRate: number;
  prevClosePrice: number;
  limitDownPrice: number;
  limitUpPrice: number;
  prevTotalVolume: number;
  totalVolume: number;
  volume: number;
  volume24h: number;
  prevTotalTurnover: number;
  totalTurnover: number;
  turnover: number;
  turnover24h: number;
  homeNotional24h: number;
  foreignNotional24h: number;
  prevPrice24h: number;
  vwap: number;
  highPrice: number;
  lowPrice: number;
  lastPrice: number;
  lastPriceProtected: number;
  lastTickDirection: string;
  lastChangePcnt: number;
  bidPrice: number;
  midPrice: number;
  askPrice: number;
  impactBidPrice: number;
  impactMidPrice: number;
  impactAskPrice: number;
  hasLiquidity: boolean;
  openInterest: number;
  openValue: number;
  fairMethod: string;
  fairBasisRate: number;
  fairBasis: number;
  fairPrice: number;
  markMethod: string;
  markPrice: number;
  indicativeSettlePrice: number;
  instantPnl: boolean;
  timestamp: string;
  minTick: number;
  capped: boolean;
};
