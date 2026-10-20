import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '../../../types';

export const RECV_WINDOW = 20_000;
export const BASE_URL = {
  livenet: 'https://api.coinbase.com',
  testnet: 'https://api.coinbase.com',
};

export const BASE_WS_URL = {
  public: {
    livenet: 'wss://advanced-trade-ws.coinbase.com',
    testnet: 'wss://advanced-trade-ws.coinbase.com',
  },
  private: {
    livenet: 'wss://advanced-trade-ws-user.coinbase.com',
    testnet: 'wss://advanced-trade-ws-user.coinbase.com',
  },
};

export const ENDPOINTS = {
  MARKETS: '/api/v3/brokerage/products',
  LIST_ACCOUNTS: '/api/v3/brokerage/accounts',
  LIST_PORTFOLIOS: '/api/v3/brokerage/portfolios',
  LIST_POSITIONS_INTX: '/api/v3/brokerage/intx/positions/{portfolio_uuid}',
  LIST_POSITIONS_CFM: '/api/v3/brokerage/cfm/positions',
  KLINE: '/api/v3/brokerage/products/{product_id}/candles',
  ORDER_BOOK: '/api/v3/brokerage/product_book',
  LIST_ORDERS: '/api/v3/brokerage/orders/historical/batch',
  INTX_INSTRUMENTS: 'https://api.international.coinbase.com/api/v1/instruments/',
  BALANCE: '/api/v3/brokerage/intx/portfolio/{portfolio_uuid}',
  BALANCE_CFM: '/api/v3/brokerage/cfm/balance_summary',
  PORTFOLIO_BREAKDOWN: '/api/v3/brokerage/portfolios/{portfolio_uuid}',
  ORDER: '/api/v3/brokerage/orders',
  EDIT_ORDER: '/api/v3/brokerage/orders/edit',
  CANCEL_ORDER_BATCH: '/api/v3/brokerage/orders/batch_cancel',

  // TODO: remove copied binance endpoints
  // LEVERAGE_BRACKET: '/fapi/v1/leverageBracket',
  // TICKERS_24H: '/fapi/v1/ticker/24hr',
  // TICKERS_BOOK: '/fapi/v1/ticker/bookTicker',
  // TICKERS_PRICE: '/fapi/v1/premiumIndex',
  // HEDGE_MODE: '/fapi/v1/positionSide/dual',
  // SET_LEVERAGE: '/fapi/v1/leverage',
  // OPEN_ORDERS: '/fapi/v1/openOrders',
  // CANCEL_SYMBOL_ORDERS: '/fapi/v1/allOpenOrders',
  // BATCH_ORDERS: '/fapi/v1/batchOrders',
  // LISTEN_KEY: '/fapi/v1/listenKey',
  // ORDERBOOK: '/fapi/v1/depth',
  // USER_TRADES: '/fapi/v1/userTrades',
};

export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.ORDER,
  ENDPOINTS.EDIT_ORDER,
  ENDPOINTS.CANCEL_ORDER_BATCH,
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.LIST_PORTFOLIOS,
  ENDPOINTS.LIST_POSITIONS_INTX,
  ENDPOINTS.LIST_POSITIONS_CFM,
  ENDPOINTS.LIST_ORDERS,
  ENDPOINTS.BALANCE,
  ENDPOINTS.BALANCE_CFM,
  ENDPOINTS.PORTFOLIO_BREAKDOWN,
];

export const PUBLIC_ENDPOINTS = [
  // ENDPOINTS.MARKETS,
  // ENDPOINTS.TICKERS_24H,
  // ENDPOINTS.TICKERS_BOOK,
  // ENDPOINTS.TICKERS_PRICE,
  // ENDPOINTS.KLINE,
  // ENDPOINTS.ORDERBOOK,
  ENDPOINTS.INTX_INSTRUMENTS,
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

export type MARKET_RESPONSE = {
  num_products: number;
  products: {
    quote_name: string;
    base_name: string;
    cancel_only: boolean;
    volume_percentage_change_24h: string;
    mid_market_price: string;
    base_display_symbol: string;
    quote_display_symbol: string;
    price_percentage_change_24h: string;
    quote_increment: string;
    post_only: boolean;
    limit_only: boolean;
    base_max_size: string;
    auction_mode: boolean;
    price: string;
    product_id: string;
    alias: string;
    quote_currency_id: string;
    new: boolean;
    trading_disabled: boolean;
    fcm_trading_session_details: { open_time: string; close_time: string; is_session_open: string };
    alias_to: string[];
    is_disabled: boolean;
    base_currency_id: string;
    price_increment: string;
    watched: boolean;
    quote_min_size: string;
    product_type: string;
    volume_24h: string;
    future_product_details: {
      venue: string;
      contract_expiry_type: string;
      contract_expiry: string;
      contract_code: string;
      contract_expiry_timezone: string;
      contract_root_unit: string;
      group_short_description: string;
      contract_size: string;
      perpetual_details: { funding_rate: string; open_interest: string; funding_time: string };
      group_description: string;
      risk_managed_by: string;
      contract_display_name: string;
    };
    base_increment: string;
    base_min_size: string;
    view_only: boolean;
    quote_max_size: string;
    status: string;
  }[];
};

export type CANDLES_RESPONSE = {
  candles: {
    start: string;
    low: string;
    high: string;
    open: string;
    close: string;
    volume: string;
  }[];
};

export type LIST_ACCOUNTS_RESPONSE = {
  uuid: string;
  name: string;
  currency: string;
  available_balance: {
    value: string;
    currency: string;
  };
  default: boolean;
  active: boolean;
  created_at: string;
  updated_at: string;
  deleted_at: string;
  type: string;
  ready: boolean;
  hold: {
    value: string;
    currency: string;
  };
}[];

export type LIST_PORTFOLIOS_RESPONSE = {
  portfolios: {
    name: string;
    uuid: string;
    type: string;
    deleted: boolean;
  }[];
};

export type LIST_POSITIONS_INTX_RESPONSE = {
  positions: {
    product_id: string;
    product_uuid: string;
    symbol: string;
    entry_vwap: {
      value: string;
      currency: string;
    };
    vwap: {
      value: string;
      currency: string;
    };
    position_side: 'POSITION_SIDE_LONG' | 'POSITION_SIDE_SHORT';
    net_size: string;
    buy_order_size: string;
    sell_order_size: string;
    im_contribution: string;
    unrealized_pnl: {
      value: string;
      currency: string;
    };
    mark_price: {
      value: string;
      currency: string;
    };
    liquidation_price: {
      value: string;
      currency: string;
    };
    leverage: string;
    im_notional: {
      value: string;
      currency: string;
    };
    mm_notional: {
      value: string;
      currency: string;
    };
    position_notional: {
      value: string;
      currency: string;
    };
    margin_type: string;
    liquidation_buffer: string;
    liquidation_percentage: string;
    portfolio_summary: {
      portfolio_uuid: string;
      collateral: string;
      position_notional: string;
      open_position_notional: string;
      pending_fees: string;
      borrow: string;
      accrued_interest: string;
      rolling_debt: string;
      portfolio_initial_margin: string;
      portfolio_im_notional: {
        value: string;
        currency: string;
      };
      portfolio_maintenance_margin: string;
      portfolio_mm_notional: {
        value: string;
        currency: string;
      };
      liquidation_percentage: string;
      liquidation_buffer: string;
      margin_type: string;
      margin_flags: string;
      liquidation_status: string;
      unrealized_pnl: {
        value: string;
        currency: string;
      };
      buying_power: {
        value: string;
        currency: string;
      };
      total_balance: {
        value: string;
        currency: string;
      };
      max_withdrawal: {
        value: string;
        currency: string;
      };
    };
  }[];
};

// [
//   {
//     "product_id": "ET-30AUG24-CDE",
//     "expiration_time": "2024-08-30T15:00:00Z",
//     "side": "LONG",
//     "number_of_contracts": "2",
//     "current_price": "2591",
//     "avg_entry_price": "2583.9999999999995",
//     "unrealized_pnl": "1.400000000000091",
//     "daily_realized_pnl": "0"
//   }
// ]

export type LIST_POSITIONS_CFM_RESPONSE = {
  positions: {
    product_id: string;
    expiration_time: string;
    side: 'LONG' | 'SHORT';
    number_of_contracts: string;
    current_price: string;
    avg_entry_price: string;
    unrealized_pnl: string;
    daily_realized_pnl: string;
  }[];
};

export type CANDLE_EVENT = {
  type: 'snapshot' | 'update';
  candles: {
    start: string;
    high: string;
    low: string;
    open: string;
    close: string;
    volume: string;
    product_id: string;
  }[];
};

export type TRADE_EVENT = {
  type: 'snapshot' | 'update';
  trades: {
    product_id: string;
    trade_id: string;
    price: string;
    size: string;
    time: string;
    side: 'BUY' | 'SELL';
  }[];
};

export type TICKER_EVENT = {
  type: 'snapshot' | 'update';
  tickers: {
    type: 'ticker';
    product_id: string;
    price: string;
    volume_24_h: string;
    low_24_h: string;
    high_24_h: string;
    low_52_w: string;
    high_52_w: string;
    price_percent_chg_24_h: string;
  }[];
};

export type LEVEL2_EVENT = {
  type: 'snapshot' | 'update';
  product_id: string;
  updates: {
    side: 'bid' | 'offer';
    event_time: string;
    price_level: string;
    new_quantity: string;
  }[];
};

export type INTX_INSTRUMENTS_RESPONSE = {
  instrument_id: string;
  instrument_uuid: string;
  symbol: string;
  type: string;
  base_asset_id: string;
  base_asset_uuid: string;
  base_asset_name: string;
  quote_asset_id: string;
  quote_asset_uuid: string;
  quote_asset_name: string;
  base_increment: string;
  quote_increment: string;
  price_band_percent: number;
  market_order_percent: number;
  qty_24hr: string;
  notional_24hr: string;
  avg_daily_qty: string;
  avg_daily_notional: string;
  previous_day_qty: string;
  open_interest: string;
  position_limit_qty: string;
  position_limit_adq_pct: number;
  replacement_cost: string;
  base_imf: number;
  min_notional_value: string;
  funding_interval: string;
  trading_state: string;
  quote: {
    best_bid_price: string;
    best_bid_size: string;
    best_ask_price: string;
    best_ask_size: string;
    trade_price: string;
    trade_qty: string;
    index_price: string;
    mark_price: string;
    settlement_price: string;
    limit_up: string;
    limit_down: string;
    predicted_funding: string;
    timestamp: string;
  };
}[];

export type BALANCE_RESPONSE = {
  summary: {
    portfolio_uuid: string;
    collateral: string;
    position_notional: string;
    open_position_notional: string;
    pending_fees: string;
    borrow: string;
    accrued_interest: string;
    rolling_debt: string;
    portfolio_initial_margin: string;
    portfolio_im_notional: {
      value: string;
      currency: string;
    };
    portfolio_maintenance_margin: string;
    portfolio_mm_notional: {
      value: string;
      currency: string;
    };
    liquidation_percentage: string;
    liquidation_buffer: string;
    margin_type: string;
    margin_flags: string;
    liquidation_status: string;
    unrealized_pnl: {
      value: string;
      currency: string;
    };
    buying_power: {
      value: string;
      currency: string;
    };
    total_balance: {
      value: string;
      currency: string;
    };
    max_withdrawal: {
      value: string;
      currency: string;
    };
  };
};

export type ORDER_EVENT = {
  type: 'snapshot' | 'update';
  orders: {
    order_id: string;
    client_order_id: string;
    cumulative_quantity: string;
    leaves_quantity: string;
    avg_price: string;
    limit_price: string;
    stop_price: string;
    total_fees: string;
    status: 'PENDING' | 'OPEN' | 'FILLED' | 'CANCELLED' | 'EXPIRED' | 'FAILED';
    product_id: string;
    creation_time: string;
    order_side: 'BUY' | 'SELL';
    order_type: 'Limit' | 'Market' | 'Stop Limit';
    cancel_reason: string;
    reject_reason: string;
  }[];
};

export type LIST_ORDERS_RESPONSE = {
  orders: {
    order_id: string;
    client_order_id: string;
    product_id: string;
    side: 'BUY' | 'SELL';
    status:
      | 'PENDING'
      | 'OPEN'
      | 'FILLED'
      | 'CANCELLED'
      | 'EXPIRED'
      | 'FAILED'
      | 'UNKNOWN_ORDER_STATUS'
      | 'QUEUED'
      | 'CANCEL_QUEUED'
      | 'EDIT_QUEUED';
    order_type:
      | 'UNKNOWN_ORDER_TYPE'
      | 'MARKET'
      | 'LIMIT'
      | 'STOP'
      | 'STOP_LIMIT'
      | 'BRACKET'
      | 'TWAP'
      | 'ROLL_OPEN'
      | 'ROLL_CLOSE'
      | 'LIQUIDATION'
      | 'SCALED';
    created_time: string;
    filled_size?: string;
    average_filled_price?: string;
    total_fees?: string;
    order_configuration?: {
      market_market_ioc?: {
        base_size?: string;
        quote_size?: string;
      };
      market_market_fok?: {
        base_size?: string;
        quote_size?: string;
      };
      sor_limit_ioc?: {
        base_size?: string;
        quote_size?: string;
        limit_price?: string;
      };
      limit_limit_gtc?: {
        base_size?: string;
        quote_size?: string;
        limit_price?: string;
        post_only?: boolean;
      };
      limit_limit_gtd?: {
        base_size?: string;
        quote_size?: string;
        limit_price?: string;
      };
      limit_limit_fok?: {
        base_size?: string;
        quote_size?: string;
        limit_price?: string;
      };
      stop_limit_stop_limit_gtc?: {
        base_size?: string;
        limit_price?: string;
        stop_price?: string;
      };
      stop_limit_stop_limit_gtd?: {
        base_size?: string;
        limit_price?: string;
        stop_price?: string;
      };
    };
  }[];
  has_next: boolean;
  cursor?: string;
};

export type STOP_DIRECTION = 'STOP_DIRECTION_STOP_UP' | 'STOP_DIRECTION_STOP_DOWN';
type MARGIN_TYPE = 'ISOLATED' | 'CROSS';

type ORDER_REQUEST_CONFIG = {
  market_market_ioc?: {
    quote_size?: string;
    base_size: string;
  };
  sor_limit_ioc?: {
    base_size: string;
    limit_price: string;
  };
  limit_limit_gtc?: {
    base_size: string;
    limit_price: string;
    post_only?: boolean;
  };
  limit_limit_gtd?: {
    base_size: string;
    limit_price: string;
    end_time: string; // RFC3339 Timestamp
    post_only?: boolean;
  };
  stop_limit_stop_limit_gtc?: {
    base_size: string;
    limit_price: string;
    stop_price: string;
    stop_direction: STOP_DIRECTION;
  };
  stop_limit_stop_limit_gtd?: {
    base_size: string;
    limit_price: string;
    stop_price: string;
    end_time: string; // RFC3339 Timestamp
    stop_direction: STOP_DIRECTION;
  };
  trigger_bracket_gtc?: {
    base_size: string;
    limit_price: string;
    stop_trigger_price: string;
  };
  trigger_bracket_gtd?: {
    base_size: string;
    limit_price: string;
    stop_trigger_price: string;
    end_time: string; // RFC3339 Timestamp
  };
};

export type PLACE_ORDER_REQUEST = {
  client_order_id: string;
  product_id: string;
  side: 'BUY' | 'SELL';
  order_configuration: ORDER_REQUEST_CONFIG;
  self_trade_prevention_id?: string;
  leverage?: string;
  margin_type?: MARGIN_TYPE;
  retail_portfolio_id?: string;
};

export type PLACE_ORDER_RESPONSE = {
  success: boolean;
  success_response?: {
    order_id: string;
  };
  failure_reason?: string;
  order_id?: string;
  error_response?: {
    error?: string;
    message?: string;
    error_details?: string;
    preview_failure_reason?: string;
    new_order_failure_reason?: string;
  };
  order_configuration?: ORDER_REQUEST_CONFIG;
};

export type EDIT_ORDER_REQUEST = {
  order_id: string;
  price: string;
  size: string;
};

export type EDIT_ORDER_RESPONSE = {
  success: boolean;
  errors?: {
    edit_failure_reason?: string;
    preview_failure_reason?: string;
  }[];
};

// {
//   "balance_summary": {
//   "futures_buying_power": {
//     "value": "0.38",
//       "currency": "USD"
//   },
//   "total_usd_balance": {
//     "value": "0.38",
//       "currency": "USD"
//   },
//   "cbi_usd_balance": {
//     "value": "0.38",
//       "currency": "USD"
//   },
//   "cfm_usd_balance": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "total_open_orders_hold_amount": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "unrealized_pnl": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "daily_realized_pnl": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "initial_margin": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "available_margin": {
//     "value": "0.38",
//       "currency": "USD"
//   },
//   "liquidation_threshold": {
//     "value": "0",
//       "currency": "USD"
//   },
//   "liquidation_buffer_amount": {
//     "value": "0.38",
//       "currency": "USD"
//   },
//   "liquidation_buffer_percentage": "1000",
//     "intraday_margin_window_measure": {
//     "margin_window_type": "FCM_MARGIN_WINDOW_TYPE_UNSPECIFIED",
//       "margin_level": "MARGIN_LEVEL_TYPE_UNSPECIFIED",
//       "initial_margin": "",
//       "maintenance_margin": "",
//       "liquidation_buffer": "",
//       "total_hold": "",
//       "futures_buying_power": ""
//   },
//   "overnight_margin_window_measure": {
//     "margin_window_type": "FCM_MARGIN_WINDOW_TYPE_UNSPECIFIED",
//       "margin_level": "MARGIN_LEVEL_TYPE_UNSPECIFIED",
//       "initial_margin": "",
//       "maintenance_margin": "",
//       "liquidation_buffer": "",
//       "total_hold": "",
//       "futures_buying_power": ""
//   }
// }
// }

export type BALANCE_RESPONSE_CFM = {
  balance_summary: {
    futures_buying_power: {
      value: string;
      currency: string;
    };
    total_usd_balance: {
      value: string;
      currency: string;
    };
    cbi_usd_balance: {
      value: string;
      currency: string;
    };
    cfm_usd_balance: {
      value: string;
      currency: string;
    };
    total_open_orders_hold_amount: {
      value: string;
      currency: string;
    };
    unrealized_pnl: {
      value: string;
      currency: string;
    };
    daily_realized_pnl: {
      value: string;
      currency: string;
    };
    initial_margin: {
      value: string;
      currency: string;
    };
    available_margin: {
      value: string;
      currency: string;
    };
    liquidation_threshold: {
      value: string;
      currency: string;
    };
    liquidation_buffer_amount: {
      value: string;
      currency: string;
    };
    liquidation_buffer_percentage: string;
    intraday_margin_window_measure: {
      margin_window_type: string;
      margin_level: string;
      initial_margin: string;
      maintenance_margin: string;
      liquidation_buffer: string;
      total_hold: string;
      futures_buying_power: string;
    };
    overnight_margin_window_measure: {
      margin_window_type: string;
      margin_level: string;
      initial_margin: string;
      maintenance_margin: string;
      liquidation_buffer: string;
      total_hold: string;
      futures_buying_power: string;
    };
  };
};

export type PORTFOLIO_BREAKDOWN_RESPONSE = {
  breakdown: {
    portfolio: {
      name: string;
      uuid: string;
      type: string;
      deleted: boolean;
    };
    portfolio_balances: {
      total_balance: {
        value: string;
        currency: string;
      };
      total_futures_balance: {
        value: string;
        currency: string;
      };
      total_cash_equivalent_balance: {
        value: string;
        currency: string;
      };
      total_crypto_balance: {
        value: string;
        currency: string;
      };
      futures_unrealized_pnl: {
        value: string;
        currency: string;
      };
      perp_unrealized_pnl: {
        value: string;
        currency: string;
      };
    };
    spot_positions: {
      asset: string;
      account_uuid: string;
      total_balance_fiat: number;
      total_balance_crypto: number;
      available_to_trade_fiat: number;
      allocation: number;
      cost_basis: {
        value: string;
        currency: string;
      };
      asset_img_url: string;
      is_cash: boolean;
      average_entry_price: string;
      asset_uuid: string;
      available_to_trade_crypto: number;
      unrealized_pnl: number;
      available_to_transfer_fiat: number;
      available_to_transfer_crypto: number;
      asset_color: string;
    }[];
    perp_positions: {
      product_id: string;
      contract_size: string;
      side: 'FUTURES_POSITION_SIDE_LONG' | 'FUTURES_POSITION_SIDE_SHORT';
      amount: string;
      avg_entry_price: string;
      current_price: string;
      unrealized_pnl: string;
      expiry: string;
      underlying_asset: string;
      asset_img_url: string;
      product_name: string;
      venue: string;
      notional_value: string;
      asset_color: string;
    }[];
    futures_positions: {
      product_id: string;
      contract_size: string;
      side: 'FUTURES_POSITION_SIDE_LONG' | 'FUTURES_POSITION_SIDE_SHORT';
      amount: string;
      avg_entry_price: string;
      current_price: string;
      unrealized_pnl: string;
      expiry: string;
      underlying_asset: string;
      asset_img_url: string;
      product_name: string;
      venue: string;
      notional_value: string;
      asset_color: string;
    }[];
  };
};
