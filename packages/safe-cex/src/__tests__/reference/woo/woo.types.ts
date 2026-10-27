import { OrderSide, OrderStopTriggerType, OrderType } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'ab82cb09-cfec-4473-80a3-b740779d0644'; // tealstreet
export const TESTNET_BROKER_ID = 'e708a644-9ce5-46c6-b50c-74b15f62d8ca'; // safe-cex

export const BASE_URL = {
  livenet: 'https://api.woox.io',
  testnet: 'https://api.staging.woox.io',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://wss.woox.io/ws/stream/',
    testnet: 'wss://wss.staging.woox.io/ws/stream/',
  },
  private: {
    livenet: 'wss://wss.woox.io/v2/ws/private/stream/',
    testnet: 'wss://wss.staging.woox.io/v2/ws/private/stream/',
  },
};

export const ENDPOINTS = {
  // v3
  ACCOUNT: '/v3/accountinfo',
  BALANCE: '/v3/balances',
  POSITIONS: '/v3/positions',
  POSITION: '/v3/position/{symbol}',
  TRANSACTION_HISTORY: '/v3/trade/transactionHistory',
  ALGO_ORDERS: '/v3/algo/orders',
  UPDATE_ORDER: '/v3/order',
  ALGO_ORDER: '/v3/algo/order',
  CANCEL_ORDERS: '/v3/orders/pending',
  CANCEL_ALGO_ORDERS: '/v3/algo/orders/pending',
  CANCEL_SYMBOL_ORDERS: '/v1/orders',
  // v1
  MARKETS: '/v1/public/info',
  TICKERS: '/v1/public/futures',
  FUNDING_RATES: '/v1/public/funding_rates',
  CLIENT_TRADES: '/v1/client/trades',
  HIST_TRADES: '/v1/client/hist_trades',
  ORDERS: '/v1/orders',
  KLINE: '/v1/kline',
  KLINE_HISTORY: 'https://api-pub.woox.io/v1/hist/kline',
  CANCEL_ORDER: '/v1/order',
  PLACE_ORDER: '/v1/order',
  LEVERAGE: '/v1/client/leverage',
  ORDERBOOK: '/v1/public/orderbook',
  POSITION_MODE: '/v1/client/position_mode',
  FUTURES_LEVERAGE: '/v1/client/futures_leverage',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ALGO_ORDER,
  ENDPOINTS.POSITION,
  ENDPOINTS.UPDATE_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_ALGO_ORDERS,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.PLACE_ORDER,
  ENDPOINTS.LEVERAGE,
  ENDPOINTS.POSITION_MODE,
  ENDPOINTS.FUTURES_LEVERAGE,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.BALANCE,
  ENDPOINTS.POSITIONS,
  ENDPOINTS.ALGO_ORDERS,
];

export const ORDER_TYPE: Record<string, OrderType> = {
  POST_ONLY: OrderType.Limit,
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  ASK: OrderType.Market,
  BID: OrderType.Market,
  TAKE_PROFIT: OrderType.TakeProfit,
  STOP_LOSS: OrderType.StopLoss,
  TRAILING_STOP: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  MARK_PRICE: OrderStopTriggerType.MarkPrice,
  MARKET_PRICE: OrderStopTriggerType.LastPrice,
};

export type ACCOUNT_RESPONSE = {
  applicationId: string;
  account: string;
  alias: string;
  accountMode: string;
  positionMode: string;
  leverage: number;
  takerFeeRate: number;
  makerFeeRate: number;
  interestRate: number;
  futuresTakerFeeRate: number;
  futuresMakerFeeRate: number;
  otpauth: boolean;
  marginRatio: number;
  openMarginRatio: number;
  initialMarginRatio: number;
  maintenanceMarginRatio: number;
  totalCollateral: number;
  freeCollateral: number;
  totalAccountValue: number;
  totalVaultValue: number;
  totalStakingValue: number;
  referrerID: string;
  accountType: string;
  totalLaunchpadVaultValue: number;
  totalEarnValue: number;
};

export type PLACE_ORDER_RESPONSE = {
  success: boolean;
  timestamp: string;
  order_id: number;
  order_type: string;
  order_price: number;
  order_quantity: number;
  order_amount: number;
  client_order_id: number;
};

export type PLACE_ALGO_ORDER_RESPONSE = {
  success: boolean;
  timestamp: number;
  data: {
    rows: Array<{ orderId: number | string }>;
  };
};

// {
//   "success": true,
//   "meta": {
//   "total": 1,
//     "records_per_page": 100,
//     "current_page": 1
// },
//   "rows": [
//   {
//     "symbol": "PERP_BTC_USDT",
//     "status": "NEW",
//     "side": "BUY",
//     "created_time": "1718174226.839",
//     "updated_time": "1718174226.839",
//     "order_id": 19004494279,
//     "order_tag": "default",
//     "price": 64533,
//     "type": "LIMIT",
//     "quantity": 0.00001,
//     "amount": null,
//     "visible": 0.00001,
//     "executed": 0,
//     "total_fee": 0,
//     "fee_asset": "USDT",
//     "total_rebate": 0,
//     "rebate_asset": "USDT",
//     "client_order_id": null,
//     "reduce_only": false,
//     "realized_pnl": null,
//     "average_executed_price": null,
//     "position_side": "BOTH"
//   }
// ]
// }

export type WOOX_ORDER = {
  symbol: string;
  status: string;
  side: string;
  created_time: string;
  updated_time: string;
  order_id: number;
  order_tag: string;
  price: number;
  type: string;
  quantity: number;
  amount: number;
  visible: number;
  executed: number;
  total_fee: number;
  fee_asset: string;
  total_rebate: number;
  rebate_asset: string;
  client_order_id: number;
  reduce_only: boolean;
  realized_pnl: number;
  average_executed_price: number;
  position_side: string;
};

export type FETCH_ORDER_RESPONSE = {
  success: boolean;
  meta: {
    total: number;
    records_per_page: number;
    current_page: number;
  };
  rows: WOOX_ORDER[];
};

// // cross margin, one way mode
// {
//   "success": true,
//   "data": {
//   "symbol": "PERP_BTC_USDT",
//     "margin_mode": "CROSS",
//     "position_mode": "ONE_WAY",
//     "details": [
//     {
//       "position_side": "BOTH",
//       "leverage": "10"
//     }
//   ]
// },
//   "timestamp": 1696663264324
// }
//
// // cross margin, hedge mode
// {
//   "success": true,
//   "data": {
//   "symbol": "PERP_BTC_USDT",
//     "margin_mode": "CROSS",
//     "position_mode": "HEDGE_MODE",
//     "details": [
//     {
//       "position_side": "LONG",
//       "leverage": "10"
//     },
//     {
//       "position_side": "SHORT",
//       "leverage": "10"
//     },
//   ]
// },
//   "timestamp": 1696663264324
// }
//
// // isolated margin, one way mode
// {
//   "success": true,
//   "data": {
//   "symbol": "PERP_BTC_USDT",
//     "margin_mode": "ISOLATED",
//     "position_mode": "ONE_WAY",
//     "details": [
//     {
//       "position_side": "BOTH",
//       "leverage": "10"
//     }
//   ]
// },
//   "timestamp": 1696663264324
// }
//
// // isolated margin, hedge mode
// {
//   "success": true,
//   "data": {
//   "symbol": "PERP_BTC_USDT",
//     "margin_mode": "ISOLATED",
//     "position_mode": "HEDGE_MODE",
//     "details": [
//     {
//       "position_side": "LONG",
//       "leverage": "10"
//     },
//     {
//       "position_side": "SHORT",
//       "leverage": "20"
//     },
//   ]
// },
//   "timestamp": 1696663264324
// }

export type GET_FUTURES_LEVERAGE_RESPONSE = {
  success: boolean;
  data: {
    symbol: string;
    margin_mode: string;
    default_margin_mode?: string;
    position_mode: string;
    details: {
      position_side: string;
      leverage: string;
    }[];
  };
  timestamp: number;
};
// {
//   "success": true,
//   "data": {
//   "rows": [
//     {
//       "open": 66166.23,
//       "close": 66124.56,
//       "low": 66038.06,
//       "high": 66176.97,
//       "volume": 23.45528526,
//       "amount": 1550436.21725288,
//       "symbol": "SPOT_BTC_USDT",
//       "type": "1m",
//       "start_timestamp": 1636388220000, // Unix epoch time in milliseconds
//       "end_timestamp": 1636388280000
//     },
//     {
//       "open": 66145.13,
//       "close": 66166.24,
//       "low": 66124.62,
//       "high": 66178.60,
//       "volume": 15.50705000,
//       "amount": 1025863.18892610,
//       "symbol": "SPOT_BTC_USDT",
//       "type": "1m",
//       "start_timestamp": 1636388160000,
//       "end_timestamp": 1636388220000
//     },
//     // ...skip
//   ],
//     "meta":{
//     "total":67377,
//       "records_per_page":100,
//       "current_page":1
//   }
// },
//   "timestamp": 1636388280000
// }

export type GET_KLINE_RESPONSE = {
  success: boolean;
  data: {
    rows: {
      open: number;
      close: number;
      low: number;
      high: number;
      volume: number;
      amount: number;
      symbol: string;
      type: string;
      start_timestamp: number; // Unix epoch time in milliseconds
      end_timestamp: number;
    }[];
    meta: {
      total: number;
      records_per_page: number;
      current_page: number;
    };
  };
  timestamp: number;
};
