import {
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionSide,
  Timeframe,
} from '../../types';

export const RECV_WINDOW = 20_000;
export const BROKER_ID = 'Tealstreet';

export const BASE_URL = {
  livenet: 'https://api.bybit.com',
  testnet: 'https://api-testnet.bybit.com',
};

export const BASE_WS_URL = {
  public: {
    linear: {
      livenet: 'wss://stream.bybit.com/v5/public/linear',
      testnet: 'wss://stream-testnet.bybit.com/v5/public/linear',
    },
    inverse: {
      livenet: 'wss://stream.bybit.com/v5/public/inverse',
      testnet: 'wss://stream-testnet.bybit.com/v5/public/inverse',
    },
  },
  private: {
    livenet: 'wss://stream.bybit.com/v5/private',
    testnet: 'wss://stream-testnet.bybit.com/v5/private',
  },
};

export const DEMO_URL = {
  livenet: 'https://api-demo.bybit.com',
  testnet: 'https://api-testnet.bybit.com',
};

export const DEMO_WS_URL = {
  private: {
    livenet: 'wss://stream-demo.bybit.com/v5/private',
    testnet: 'wss://stream-testnet.bybit.com/v5/private',
  },
};

export const ENDPOINTS = {
  ACCOUNT_MARGIN: '/v5/account/info',
  ACCOUNT: '/v5/user/query-api',
  BALANCE: '/v5/account/wallet-balance',
  FEE_RATE: '/v5/account/fee-rate',
  UNFILLED_ORDERS: '/v5/order/realtime',
  TICKERS: '/v5/market/tickers',
  MARKETS: '/v5/market/instruments-info',
  CANCEL_ORDER: '/v5/order/cancel',
  CANCEL_ORDERS: '/v5/order/cancel-batch',
  CANCEL_SYMBOL_ORDERS: '/v5/order/cancel-all',
  POSITIONS: '/v5/position/list',
  KLINE: '/v5/market/kline',
  SET_LEVERAGE: '/v5/position/set-leverage',
  SET_TRADING_STOP: '/v5/position/trading-stop',
  CREATE_ORDER: '/v5/order/create',
  CREATE_BATCH_ORDER: '/v5/order/create-batch',
  REPLACE_ORDER: '/v5/order/amend',
  SET_POSITION_MODE: '/v5/position/switch-mode',
  SET_MARGIN_MODE: '/v5/account/set-margin-mode',
  USER_ORDERS: '/v5/order/history',
  USER_TRADES: '/v5/execution/list',
  USER_CLOSED_PNL: '/v5/position/closed-pnl',
  // Transaction log (realized PnL, funding fees, etc.)
  TRANSACTION_LOG: '/v5/account/transaction-log',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.CREATE_ORDER,
  ENDPOINTS.CREATE_BATCH_ORDER,
  ENDPOINTS.REPLACE_ORDER,
  ENDPOINTS.SET_POSITION_MODE,
  ENDPOINTS.SET_MARGIN_MODE,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.CANCEL_ORDERS,
  ENDPOINTS.CANCEL_SYMBOL_ORDERS,
  ENDPOINTS.SET_TRADING_STOP,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.POSITIONS,
  ENDPOINTS.UNFILLED_ORDERS,
  ENDPOINTS.FEE_RATE,
  ENDPOINTS.ACCOUNT,
  ENDPOINTS.ACCOUNT_MARGIN,
  ENDPOINTS.BALANCE,
];

export const INTERVAL: Record<Timeframe, string> = {
  '1m': '1',
  '3m': '3',
  '5m': '5',
  '15m': '15',
  '30m': '30',
  '1h': '60',
  '2h': '120',
  '4h': '240',
  '6h': '360',
  '12h': '720',
  '1d': 'D',
  '1w': 'W',
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  Created: OrderStatus.Open,
  New: OrderStatus.Open,
  Active: OrderStatus.Open,
  Untriggered: OrderStatus.Open,
  PartiallyFilled: OrderStatus.Open,
  Rejected: OrderStatus.Closed,
  Filled: OrderStatus.Closed,
  Deactivated: OrderStatus.Closed,
  Triggered: OrderStatus.Closed,
  PendingCancel: OrderStatus.Canceled,
  Cancelled: OrderStatus.Canceled,
};

export const ORDER_TYPE: Record<string, OrderType> = {
  Limit: OrderType.Limit,
  Market: OrderType.Market,
  Stop: OrderType.StopLoss,
  StopLoss: OrderType.StopLoss,
  TakeProfit: OrderType.TakeProfit,
  PartialTakeProfit: OrderType.TakeProfit,
  PartialStopLoss: OrderType.StopLoss,
  TrailingStop: OrderType.TrailingStopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  Buy: OrderSide.Buy,
  Sell: OrderSide.Sell,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  Buy: PositionSide.Long,
  Sell: PositionSide.Short,
};

export const ORDER_TIME_IN_FORCE: Record<string, OrderTimeInForce> = {
  GTC: OrderTimeInForce.GoodTillCancel,
  IOC: OrderTimeInForce.ImmediateOrCancel,
  FOK: OrderTimeInForce.FillOrKill,
  PostOnly: OrderTimeInForce.PostOnly,
  // RPI is a maker-only order that matches retail flow, so PostOnly is the
  // closest internal display/runtime contract.
  RPI: OrderTimeInForce.PostOnly,
};

// Keep placement serialization explicit: inbound RPI orders display as
// PostOnly, but Tealstreet's own PostOnly orders must still be sent as
// `PostOnly`, never as the market-maker-only `RPI` value.
export const ORDER_TIME_IN_FORCE_OUTBOUND: Record<OrderTimeInForce, string> = {
  [OrderTimeInForce.GoodTillCancel]: 'GTC',
  [OrderTimeInForce.ImmediateOrCancel]: 'IOC',
  [OrderTimeInForce.FillOrKill]: 'FOK',
  [OrderTimeInForce.PostOnly]: 'PostOnly',
};

export const ORDER_TRIGGER_TYPE: Record<string, OrderStopTriggerType> = {
  IndexPrice: OrderStopTriggerType.IndexPrice,
  MarkPrice: OrderStopTriggerType.MarkPrice,
  LastPrice: OrderStopTriggerType.LastPrice,
};

export interface BybitApiKeyInfo {
  id: string;
  note: string;
  apiKey: string;
  readOnly: number;
  secret: string;
  permissions: {
    ContractTrade: string[];
    Spot: string[];
    Wallet: string[];
    Options: string[];
    Derivatives: string[];
    CopyTrading: string[];
    BlockTrade: string[];
    Exchange: string[];
    NFT: string[];
  };
  ips: string[];
  type: number;
  deadlineDay: number;
  expiredAt: string;
  createdAt: string;
  unified: number;
  uta: number;
  userID: number;
  inviterID: number;
  vipLevel: string;
  mktMakerLevel: string;
  affiliateID: number;
  rsaPublicKey: string;
  isMaster: boolean;
  parentUid: string;
}

// Transaction log types for /v5/account/transaction-log endpoint
// See: https://bybit-exchange.github.io/docs/v5/account/transaction-log
export type BybitTransactionType =
  | 'TRADE'
  | 'SETTLEMENT'
  | 'DELIVERY'
  | 'TRANSFER'
  | 'AIRDROP'
  | 'LIQUIDATION'
  | 'ADL'
  | 'BLOCK_TRADE'
  | 'BONUS'
  | 'FEE_REFUND'
  | 'INTEREST'
  | 'CURRENCY_BUY'
  | 'CURRENCY_SELL'
  | 'AUTO_REPAYMENT'
  | 'AUTO_BORROW'
  | 'MANUAL_REPAYMENT'
  | 'SPOT_REPAY_COLLATERAL'
  | 'CUSTODY_NETWORK'
  | 'CUSTODY_SETTLE'
  | 'CUSTODY'
  | 'CUSTODY_AUTO'
  | 'CUSTODY_MANUAL'
  | 'PREMARKET_TRANSFER_IN'
  | 'PREMARKET_TRANSFER_OUT'
  | 'PREMARKET_DELIVERY'
  | 'PREMARKET_ROLLBACK'
  | 'NONE_C2C_TRANSFER';

export interface BybitTransactionRecord {
  symbol: string;
  category: string;
  side: string;
  transactionTime: string;
  type: BybitTransactionType;
  qty: string;
  size: string;
  currency: string;
  tradePrice: string;
  funding: string;
  fee: string;
  cashFlow: string;
  change: string;
  cashBalance: string;
  feeRate: string;
  bonusChange: string;
  tradeId: string;
  orderId: string;
  orderLinkId: string;
}

export interface BybitTransactionHistoryOptions {
  accountType?: 'UNIFIED' | 'CONTRACT';
  category?: 'linear' | 'inverse' | 'option' | 'spot';
  currency?: string;
  baseCoin?: string;
  type?: BybitTransactionType;
  startTime?: number;
  endTime?: number;
  limit?: number;
  cursor?: string;
}
