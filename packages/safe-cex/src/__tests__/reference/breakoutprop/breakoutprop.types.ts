import { OrderSide, OrderStatus, OrderType, PositionSide, Timeframe } from '../../../types';

export const RECV_WINDOW = 20_000;
// export const BASE_URL = 'http://localhost:5050/breakoutprop';
// export const BASE_URL = 'https://app.breakoutprop.com/dxsca-web';
export const BASE_URL = 'https://breakout.prosp.devexperts.com/dxsca-web';

export const ENDPOINTS = {
  LOGIN: '/login',
  USER: '/users/{username}',
  PORTFOLIO: 'accounts/{accountCode}/portfolio',
  INSTRUMENTS: 'instruments/query',
  INSTRUMENT_DETAILS: 'accounts/{accountCode}/instruments/query',
  MARKET_DATA: 'marketdata',

  CANCEL_ORDER: 'accounts/{accountCode}/orders/{orderCode}',
  PLACE_ORDER: 'accounts/{accountCode}/orders',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.PLACE_ORDER,
  ENDPOINTS.CANCEL_ORDER,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.PORTFOLIO,
];

// string candleType
// Possible values:
//
//   m (1 min)
// 5m (5 min)
// 15m (15 minutes)
// 30m (30 minutes)
// h (1 hour)
// 2h (2 hours)
// 4h (4 hours)
// d (Day)
// w (Week)
// mo (Month)
export const INTERVAL: { [key in Timeframe]?: string } = {
  '1m': 'm',
  '5m': '5m',
  '15m': '15m',
  '30m': '30m',
  '1h': 'h',
  '2h': '2h',
  '4h': '4h',
  '1d': 'd',
  '1w': 'w',
};

export const ORDER_TYPE: Record<string, OrderType> = {
  LIMIT: OrderType.Limit,
  MARKET: OrderType.Market,
  STOP: OrderType.StopLoss,
};

export const ORDER_SIDE: Record<string, OrderSide> = {
  BUY: OrderSide.Buy,
  SELL: OrderSide.Sell,
};

export const ORDER_STATUS: Record<string, OrderStatus> = {
  WORKING: OrderStatus.Open,
};

export const POSITION_SIDE: Record<string, PositionSide> = {
  BUY: PositionSide.Long,
  SELL: PositionSide.Short,
};

// {"sessionToken":"<session-token>","timeout":"00:30:00"}%
export type LOGIN_RESPONSE = {
  sessionToken: string;
  timeout: string;
};

// {
//   "userDetails": [
//   {
//     "login": "btg_XXXXXX",
//     "domain": "default",
//     "version": 18758,
//     "fullName": "btg_XXXXXX",
//     "accounts": [
//       {
//         "owner": "btg_XXXXXX@default",
//         "account": "default:00001",
//         "version": 120,
//         "baseCurrency": "USD",
//         "registrationTime": "2024-06-12T11:37:59.548Z",
//         "accountStatus": "FULL_TRADING",
//         "pricingStream": "400000001,2200000001",
//         "positionBased": true
//       }
//     ],
//     "username": "btg_XXXXXX@default"
//   }
// ]
// }

export type USER_RESPONSE = {
  userDetails: {
    login: string;
    domain: string;
    version: number;
    fullName: string;
    accounts: {
      owner: string;
      account: string;
      version: number;
      baseCurrency: string;
      registrationTime: string;
      accountStatus: string;
      pricingStream: string;
      positionBased: boolean;
    }[];
    username: string;
  }[];
};

// {
//   "portfolios": [
//   {
//     "account": "default:00002",
//     "version": 86,
//     "balances": [
//       {
//         "account": "default:00002",
//         "version": 86,
//         "value": 4999.96,
//         "currency": "USD"
//       }
//     ],
//     "positions": [
//       {
//         "account": "default:00002",
//         "version": 85,
//         "positionCode": "76515503",
//         "symbol": "ETHUSD",
//         "quantity": 0.01,
//         "side": "BUY",
//         "quantityNotional": 0.01,
//         "openTime": "2024-12-11T06:00:49.087Z",
//         "openPrice": 3656.01,
//         "lastUpdateTime": "2024-12-11T06:00:49.087Z",
//         "marginRate": 0.2
//       },
//       {
//         "account": "default:00002",
//         "version": 82,
//         "positionCode": "76515542",
//         "symbol": "ETHUSD",
//         "quantity": 0.01,
//         "side": "BUY",
//         "quantityNotional": 0.01,
//         "openTime": "2024-12-11T05:56:26.400Z",
//         "openPrice": 3660.0,
//         "lastUpdateTime": "2024-12-11T05:56:26.400Z",
//         "marginRate": 0.2
//       }
//     ],
//     "orders": [
//       {
//         "account": "default:00002",
//         "version": 73,
//         "orderId": 76515500,
//         "orderCode": "4471153:55639",
//         "clientOrderId": "55639",
//         "actionCode": "4471153:55639",
//         "legCount": 1,
//         "type": "LIMIT",
//         "instrument": "ETHUSD",
//         "status": "WORKING",
//         "finalStatus": false,
//         "legs": [
//           {
//             "instrument": "ETHUSD",
//             "positionEffect": "OPEN",
//             "positionCode": "76515500",
//             "side": "BUY",
//             "price": 3529.47,
//             "legRatio": 1.0,
//             "quantity": 0.01,
//             "filledQuantity": 0.0,
//             "remainingQuantity": 0.01,
//             "averagePrice": 0.0
//           }
//         ],
//         "side": "BUY",
//         "tif": "GTC",
//         "marginRate": 0.2,
//         "issueTime": "2024-12-11T05:55:49.900Z",
//         "transactionTime": "2024-12-11T05:55:49.900Z",
//         "executions": [
//           {
//             "account": "default:00002",
//             "executionCode": "3612101:13706396",
//             "orderCode": "4471153:55639",
//             "updateOrderId": 76515500,
//             "version": 73,
//             "clientOrderId": "55639",
//             "actionCode": "4471153:55639",
//             "status": "WORKING",
//             "finalStatus": false,
//             "filledQuantity": 0.0,
//             "lastQuantity": 0.0,
//             "filledQuantityNotional": 0.0,
//             "lastQuantityNotional": 0.0,
//             "transactionTime": "2024-12-11T05:55:49.900Z",
//             "marginRate": 0.2
//           }
//         ],
//         "cashTransactions": []
//       }
//     ]
//   }
// ]
// }

export type PORTFOLIO_RESPONSE = {
  portfolios: {
    account: string;
    version: number;
    balances: {
      account: string;
      version: number;
      value: number;
      currency: string;
    }[];
    positions: {
      account: string;
      version: number;
      positionCode: string;
      symbol: string;
      quantity: number;
      side: string;
      quantityNotional: number;
      openTime: string;
      openPrice: number;
      lastUpdateTime: string;
      marginRate: number;
    }[];
    orders: {
      account: string;
      version: number;
      orderId: number;
      orderCode: string;
      clientOrderId: string;
      actionCode: string;
      legCount: number;
      type: string;
      instrument: string;
      status: string;
      finalStatus: boolean;
      limitPrice?: number;
      stopPrice?: number;
      legs: {
        instrument: string;
        positionEffect: string;
        positionCode: string;
        side: string;
        price?: number;
        legRatio: number;
        quantity: number;
        filledQuantity: number;
        remainingQuantity: number;
        averagePrice: number;
      }[];
      side: string;
      tif: string;
      marginRate: number;
      issueTime: string;
      transactionTime: string;
      executions: {
        account: string;
        executionCode: string;
        orderCode: string;
        updateOrderId: number;
        version: number;
        clientOrderId: string;
        actionCode: string;
        status: string;
        finalStatus: boolean;
        filledQuantity: number;
        lastQuantity: number;
        filledQuantityNotional: number;
        lastQuantityNotional: number;
        transactionTime: string;
        marginRate: number;
      }[];
      cashTransactions: any[];
    }[];
  }[];
};

// instruments:
//
// [{
//   "type": "FOREX",
//   "symbol": "1000FLOKIUSD",
//   "version": 383,
//   "description": "FLOKI",
//   "priceIncrement": 0.00001,
//   "pipSize": 0.00001,
//   "currency": "USD",
//   "lotSize": 1,
//   "multiplier": 1,
//   "firstCurrency": "FLOKI"
// }]

// {
//   "account": "default:00002",
//   "type": "FOREX",
//   "symbol": "ARUSD",
//   "tradingStatus": "FULL",
//   "minOrderSize": 0.1,
//   "maxOrderSize": {
//   "value": 4000,
//     "limitType": "LOTS"
// },
//   "minOrderSizeIncrement": 0.1,
//   "marginRate": {
//   "rateType": "FLAT",
//     "tiers": [
//     {
//       "endVolume": "Infinity",
//       "value": 0.5
//     }
//   ]
// }
// }

export type INSTRUMENTS_RESPONSE = {
  instruments: {
    type: string;
    symbol: string;
    version: number;
    description: string;
    priceIncrement: number;
    pipSize: number;
    currency: string;
    lotSize: number;
    multiplier: number;
    firstCurrency: string;
  }[];
};

// {
//   "account": "default:00002",
//   "type": "FOREX",
//   "symbol": "ETHUSD",
//   "tradingStatus": "FULL",
//   "minOrderSize": 0.01,
//   "maxOrderSize": {
//   "value": 724,
//     "limitType": "LOTS"
// },
//   "minOrderSizeIncrement": 0.01,
//   "marginRate": {
//   "rateType": "FLAT",
//     "tiers": [
//     {
//       "endVolume": "Infinity",
//       "value": 0.2
//     }
//   ]
// }
// }

export type INSTRUMENT_DETAILS_RESPONSE = {
  instrumentDetails: {
    account: string;
    type: string;
    symbol: string;
    tradingStatus: string;
    minOrderSize: number;
    maxOrderSize: {
      value: number;
      limitType: string;
    };
    minOrderSizeIncrement: number;
    marginRate: {
      rateType: string;
      tiers: {
        endVolume: string;
        value: number;
      }[];
    };
  }[];
};

// [
//   {
//     "symbol": "AAVEUSD",
//     "type": "Quote",
//     "ask": 269.13,
//     "bid": 269.09,
//     "time": "2024-12-11T07:55:11Z"
//   },
//   {
//     "symbol": "ACTUSD",
//     "type": "Quote",
//     "ask": 0.4808,
//     "bid": 0.4806,
//     "time": "2024-12-11T07:55:12Z"
//   },
//   {
//     "symbol": "ADAUSD",
//     "type": "Quote",
//     "ask": 1.0011,
//     "bid": 1.001,
//     "time": "2024-12-11T07:55:11Z"
//   }
// ]

export type TICKER_RESPONSE = {
  events: {
    symbol: string;
    type: string;
    ask: number;
    bid: number;
    time: string;
  }[];
};

// {
//   "events": [
//   {
//     "symbol": "BTCUSD",
//     "type": "Candle",
//     "candleType": "h",
//     "open": 98117.9,
//     "close": 98035.9,
//     "high": 98173.6,
//     "low": 97952,
//     "volume": 922.778,
//     "time": "2024-12-11T09:00:00Z"
//   }
// ]
// }

export type CANDLE_RESPONSE = {
  events: {
    symbol: string;
    type: string;
    candleType: string;
    open: number;
    close: number;
    high: number;
    low: number;
    volume: number;
    time: string;
  }[];
};
