import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../types';

export const BROKER_ID = 'Tealstreet';

export type PhemexApiResponse<T extends Record<string, any>> = T & {
  code: number;
  msg: string;
};

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://api.phemex.com',
  testnet: 'https://testnet-api.phemex.com',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://ws.phemex.com',
    testnet: 'wss://testnet-api.phemex.com/ws',
  },
  private: {
    livenet: 'wss://ws.phemex.com',
    testnet: 'wss://testnet-api.phemex.com/ws',
  },
};

export const ENDPOINTS = {
  MARKETS: '/public/products',
  BATCH_ORDERS: '/g-orders', // used only for cancellation
  CANCEL_ORDER: '/g-orders/cancel', // used only for cancellation
  TICKERS: '/md/v3/ticker/24hr/all',
  ACCOUNT: '/g-accounts/accountPositions',
  ORDER: '/g-orders/create', // must be PUT
  USER_TRADES: '/exchange/order/v2/tradingList',
  USER_TRADES_LEGACY: '/exchange/order/trade',
  KLINE: '/exchange/public/md/v2/kline/list',
  ORDERBOOK: '/md/v2/fullbook',
  SET_LEVERAGE: '/g-positions/leverage', // must be PUT
  SET_LEVERAGE_CONTRACT: '/positions/leverage', // must be PUT
  POSITIONS: '/g-accounts/accountPositions',
  SET_POSITION_MODE: '/g-positions/switch-pos-mode-sync', // must be PUT
};

export const GRAFANA_ENDPOINTS_PERIODIC = [ENDPOINTS.POSITIONS, ENDPOINTS.ACCOUNT];

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.BATCH_ORDERS,
  ENDPOINTS.CANCEL_ORDER,
  ENDPOINTS.ORDER,
  ENDPOINTS.SET_LEVERAGE,
  ENDPOINTS.SET_LEVERAGE_CONTRACT,
  ENDPOINTS.SET_POSITION_MODE,
];

export const PUBLIC_ENDPOINTS = [ENDPOINTS.MARKETS, ENDPOINTS.TICKERS, ENDPOINTS.KLINE, ENDPOINTS.ORDERBOOK];

// Order status includes New, PartiallyFilled, Filled, Canceled, Rejected, Triggered, Untriggered;
// Open order status includes New, PartiallyFilled, Untriggered;

export const ORDER_STATUS: Record<string, OrderStatus> = {
  new: OrderStatus.Open,
  untriggered: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
  partiallyfilled: OrderStatus.Open,
  triggered: OrderStatus.Closed,
  filled: OrderStatus.Closed,
  rejected: OrderStatus.Canceled,
  canceled: OrderStatus.Canceled,
};

export const ORDER_TYPE: Record<OrderType, string> = {
  [OrderType.Market]: 'Market',
  [OrderType.Limit]: 'Limit',
  [OrderType.Conditional]: 'Stop',
  [OrderType.StopLoss]: 'Stop',
  [OrderType.TakeProfit]: 'MarketIfTouched',
  [OrderType.TrailingStopLoss]: 'MarketIfTouched',
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
  GoodTillCancel: OrderTimeInForce.GoodTillCancel,
  ImmediateOrCancel: OrderTimeInForce.ImmediateOrCancel,
  FillOrKill: OrderTimeInForce.FillOrKill,
  PostOnly: OrderTimeInForce.PostOnly,
};

export type POSITIONS_RESPONSE = {
  account: {
    userID: number;
    accountId: number;
    currency: string;
    accountBalanceRv: string;
    totalUsedBalanceRv: string;
    bonusBalanceRv: string;
    status: number;
  };
  positions: {
    userID: number;
    accountID: number;
    symbol: string;
    currency: string;
    side: string;
    positionStatus: string;
    crossMargin: boolean;
    leverageRr: string;
    initMarginReqRr: string;
    maintMarginReqRr: string;
    riskLimitRv: string;
    size: string;
    valueRv: string;
    avgEntryPriceRp: string;
    avgEntryPrice: string;
    posCostRv: string;
    assignedPosBalanceRv: string;
    bankruptCommRv: string;
    bankruptPriceRp: string;
    positionMarginRv: string;
    liquidationPriceRp: string;
    deleveragePercentileRr: string;
    buyValueToCostRr: string;
    sellValueToCostRr: string;
    markPriceRp: string;
    estimatedOrdLossRv: string;
    usedBalanceRv: string;
    cumClosedPnlRv: string;
    cumFundingFeeRv: string;
    cumTransactFeeRv: string;
    transactTimeNs: number;
    takerFeeRateRr: string;
    makerFeeRateRr: string;
    term: number;
    lastTermEndTimeNs: number;
    lastFundingTimeNs: number;
    curTermRealisedPnlRv: string;
    execSeq: number;
    posSide: string;
    posMode: string;
    buyLeavesValueRv: string;
    sellLeavesValueRv: string;
  }[];
};
