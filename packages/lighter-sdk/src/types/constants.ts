/**
 * Lighter SDK Constants
 *
 * This file contains all constant values and enums used throughout the Lighter SDK.
 */

/**
 * Transaction type constants for L2 operations
 */
export const TxType = {
  /** Create order transaction */
  L2CreateOrder: 14,
  /** Cancel order transaction */
  L2CancelOrder: 15,
  /** Transfer funds transaction */
  L2Transfer: 16,
  /** Withdraw funds transaction */
  L2Withdraw: 17,
  /** Update API key transaction */
  L2UpdateApiKey: 18,
  /** Liquidation transaction */
  L2Liquidation: 19,
  /** Deleverage transaction */
  L2Deleverage: 20,
  /** Funding fee transaction */
  L2FundingFee: 21,
  /** Create grouped orders transaction */
  L2CreateGroupedOrders: 28,
} as const;

export type TxTypeValue = typeof TxType[keyof typeof TxType];

/**
 * Order types supported by the Lighter protocol
 */
export enum OrderType {
  /** Limit order - order with a specified price */
  Limit = 0,
  /** Market order - order executed at current market price */
  Market = 1,
  /** Stop loss order - triggered when price reaches stop price */
  StopLoss = 2,
  /** Take profit order - triggered when price reaches target price */
  TakeProfit = 3,
  /** Time-weighted average price order */
  TWAP = 4,
}

/**
 * Time in force options for orders
 */
export enum TimeInForce {
  /** Immediate or Cancel - execute immediately or cancel */
  ImmediateOrCancel = 0,
  /** Good till time - valid until specified time */
  GoodTillTime = 1,
  /** Post only - only add liquidity, never take */
  PostOnly = 2,
}

/**
 * Order status enumeration
 */
export enum OrderStatus {
  /** Order is pending execution */
  Pending = 0,
  /** Order is open and active */
  Open = 1,
  /** Order has been partially filled */
  PartiallyFilled = 2,
  /** Order has been completely filled */
  Filled = 3,
  /** Order has been cancelled */
  Cancelled = 4,
  /** Order has been rejected */
  Rejected = 5,
  /** Order has expired */
  Expired = 6,
}

/**
 * Trigger status for conditional orders
 */
export enum TriggerStatus {
  /** Trigger is inactive */
  Inactive = 0,
  /** Trigger is active and monitoring */
  Active = 1,
  /** Trigger has been triggered */
  Triggered = 2,
  /** Trigger has been cancelled */
  Cancelled = 3,
}

/**
 * Margin mode for trading accounts
 */
export enum MarginMode {
  /** Cross margin - use all available margin */
  Cross = 0,
  /** Isolated margin - use only allocated margin */
  Isolated = 1,
}

/**
 * Grouping type for orders
 */
export enum GroupingType {
  /** One order triggers another order */
  OneTriggersTheOther = 1,
  /** One order cancels another order */
  OneCancelsTheOther = 2,
  /** One order triggers an OCO pair */
  OneTriggersAOneCancelsTheOther = 3,
}

/**
 * Side of the order (buy/sell)
 */
export enum OrderSide {
  /** Buy order */
  Buy = 0,
  /** Sell order */
  Sell = 1,
}

/**
 * Maximum account index value
 */
export const MaxAccountIndex = 255;

/**
 * Maximum API key index value
 */
export const MaxApiKeyIndex = 255;

/**
 * Maximum nonce value (uint32 max)
 */
export const MaxNonce = 4294967295;

/**
 * Maximum expiry time value (unix timestamp)
 */
export const MaxExpiry = 4294967295;

/**
 * Minimum order size (base token units)
 */
export const MinOrderSize = BigInt(1);

/**
 * Maximum order size (base token units)
 */
export const MaxOrderSize = BigInt("18446744073709551615"); // uint64 max

/**
 * Precision for price values (decimal places)
 */
export const PricePrecision = 8;

/**
 * Precision for quantity values (decimal places)
 */
export const QuantityPrecision = 8;

/**
 * Default gas limit for transactions
 */
export const DefaultGasLimit = 300000;

/**
 * Default gas price multiplier
 */
export const DefaultGasPriceMultiplier = 1.2;

/**
 * API version
 */
export const ApiVersion = "v1";

/**
 * WebSocket message types
 */
export enum WsMessageType {
  /** Subscribe to channel */
  Subscribe = "subscribe",
  /** Unsubscribe from channel */
  Unsubscribe = "unsubscribe",
  /** Ping message */
  Ping = "ping",
  /** Pong message */
  Pong = "pong",
  /** Data update */
  Update = "update",
  /** Error message */
  Error = "error",
}

/**
 * WebSocket channel types
 */
export enum WsChannel {
  /** Order book updates */
  OrderBook = "orderbook",
  /** Trade updates */
  Trades = "trades",
  /** Ticker updates */
  Ticker = "ticker",
  /** User orders */
  Orders = "orders",
  /** User positions */
  Positions = "positions",
  /** User balances */
  Balances = "balances",
}

/**
 * Result codes for API responses
 */
export enum ResultCode {
  /** Success */
  Success = 0,
  /** Invalid parameters */
  InvalidParams = 1,
  /** Authentication failed */
  AuthFailed = 2,
  /** Insufficient balance */
  InsufficientBalance = 3,
  /** Order not found */
  OrderNotFound = 4,
  /** Market not found */
  MarketNotFound = 5,
  /** Rate limit exceeded */
  RateLimitExceeded = 6,
  /** Internal server error */
  InternalError = 7,
  /** Nonce too low */
  NonceTooLow = 8,
  /** Nonce too high */
  NonceTooHigh = 9,
  /** Signature invalid */
  InvalidSignature = 10,
  /** Transaction expired */
  TxExpired = 11,
}
