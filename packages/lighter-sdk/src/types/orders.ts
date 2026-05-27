/**
 * Order-related types and interfaces for the Lighter SDK
 */

import { OrderType, TimeInForce, OrderStatus, TriggerStatus, OrderSide } from './constants';

/**
 * Order information interface
 */
export interface OrderInfo {
  /** Unique order ID */
  orderId: string;
  /** Client-provided order ID */
  clientOrderId?: string;
  /** Market ID */
  marketId: string;
  /** Market symbol */
  symbol: string;
  /** Account address */
  account: string;
  /** Order type */
  type: OrderType;
  /** Order side (buy/sell) */
  side: OrderSide;
  /** Order price (for limit orders) */
  price: string;
  /** Order quantity */
  quantity: string;
  /** Filled quantity */
  filledQuantity: string;
  /** Remaining quantity */
  remainingQuantity: string;
  /** Average fill price */
  averagePrice: string;
  /** Order status */
  status: OrderStatus;
  /** Time in force */
  timeInForce: TimeInForce;
  /** Order expiry timestamp */
  expiry: number;
  /** Stop price (for stop orders) */
  stopPrice?: string;
  /** Trigger status (for conditional orders) */
  triggerStatus?: TriggerStatus;
  /** Reduce only flag */
  reduceOnly: boolean;
  /** Post only flag */
  postOnly: boolean;
  /** Fee paid */
  fee: string;
  /** Fee token */
  feeToken: string;
  /** Order creation timestamp */
  createdAt: number;
  /** Order update timestamp */
  updatedAt: number;
  /** Transaction hash */
  txHash?: string;
  /** Nonce used for this order */
  nonce: number;
  /** Account index */
  accountIndex: number;
  /** API key index */
  apiKeyIndex: number;
}

/**
 * Fill information for an order
 */
export interface Fill {
  /** Fill ID */
  fillId: string;
  /** Order ID */
  orderId: string;
  /** Trade ID */
  tradeId: string;
  /** Market symbol */
  symbol: string;
  /** Fill price */
  price: string;
  /** Fill quantity */
  quantity: string;
  /** Quote quantity */
  quoteQuantity: string;
  /** Fee paid */
  fee: string;
  /** Fee token */
  feeToken: string;
  /** Is maker */
  isMaker: boolean;
  /** Order side */
  side: OrderSide;
  /** Fill timestamp */
  timestamp: number;
}

/**
 * Parameters for creating a limit order
 */
export interface CreateLimitOrderParams {
  /** Market symbol */
  symbol: string;
  /** Order side */
  side: OrderSide;
  /** Order price */
  price: string;
  /** Order quantity */
  quantity: string;
  /** Time in force */
  timeInForce?: TimeInForce;
  /** Post only flag */
  postOnly?: boolean;
  /** Reduce only flag */
  reduceOnly?: boolean;
  /** Client order ID */
  clientOrderId?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
  /** Order expiry timestamp */
  expiry?: number;
}

/**
 * Parameters for creating a market order
 */
export interface CreateMarketOrderParams {
  /** Market symbol */
  symbol: string;
  /** Order side */
  side: OrderSide;
  /** Order quantity */
  quantity: string;
  /** Reduce only flag */
  reduceOnly?: boolean;
  /** Client order ID */
  clientOrderId?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Parameters for creating a stop loss order
 */
export interface CreateStopLossOrderParams {
  /** Market symbol */
  symbol: string;
  /** Order side */
  side: OrderSide;
  /** Stop price */
  stopPrice: string;
  /** Order quantity */
  quantity: string;
  /** Limit price (optional, for stop-limit) */
  price?: string;
  /** Reduce only flag */
  reduceOnly?: boolean;
  /** Client order ID */
  clientOrderId?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
  /** Order expiry timestamp */
  expiry?: number;
}

/**
 * Parameters for creating a take profit order
 */
export interface CreateTakeProfitOrderParams {
  /** Market symbol */
  symbol: string;
  /** Order side */
  side: OrderSide;
  /** Take profit price */
  takeProfitPrice: string;
  /** Order quantity */
  quantity: string;
  /** Limit price (optional, for take-profit-limit) */
  price?: string;
  /** Reduce only flag */
  reduceOnly?: boolean;
  /** Client order ID */
  clientOrderId?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
  /** Order expiry timestamp */
  expiry?: number;
}

/**
 * Parameters for canceling an order
 */
export interface CancelOrderParams {
  /** Order ID to cancel */
  orderId?: string;
  /** Client order ID to cancel */
  clientOrderId?: string;
  /** Market symbol */
  symbol: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Parameters for canceling all orders
 */
export interface CancelAllOrdersParams {
  /** Market symbol (optional, if not provided cancels all orders for all markets) */
  symbol?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Parameters for querying orders
 */
export interface QueryOrdersParams {
  /** Market symbol */
  symbol?: string;
  /** Order status filter */
  status?: OrderStatus;
  /** Order side filter */
  side?: OrderSide;
  /** Order type filter */
  type?: OrderType;
  /** Start time */
  startTime?: number;
  /** End time */
  endTime?: number;
  /** Page number */
  page?: number;
  /** Page size */
  pageSize?: number;
}

/**
 * Parameters for querying fills
 */
export interface QueryFillsParams {
  /** Market symbol */
  symbol?: string;
  /** Order ID filter */
  orderId?: string;
  /** Start time */
  startTime?: number;
  /** End time */
  endTime?: number;
  /** Page number */
  page?: number;
  /** Page size */
  pageSize?: number;
}

/**
 * Order modification parameters
 */
export interface ModifyOrderParams {
  /** Order ID to modify */
  orderId: string;
  /** New price (optional) */
  price?: string;
  /** New quantity (optional) */
  quantity?: string;
  /** New expiry (optional) */
  expiry?: number;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Batch order creation parameters
 */
export interface BatchCreateOrdersParams {
  /** List of orders to create */
  orders: (CreateLimitOrderParams | CreateMarketOrderParams)[];
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Batch order cancellation parameters
 */
export interface BatchCancelOrdersParams {
  /** List of order IDs to cancel */
  orderIds?: string[];
  /** List of client order IDs to cancel */
  clientOrderIds?: string[];
  /** Market symbol */
  symbol?: string;
  /** Account index */
  accountIndex?: number;
  /** API key index */
  apiKeyIndex?: number;
}

/**
 * Order statistics
 */
export interface OrderStats {
  /** Total orders count */
  totalOrders: number;
  /** Active orders count */
  activeOrders: number;
  /** Filled orders count */
  filledOrders: number;
  /** Cancelled orders count */
  cancelledOrders: number;
  /** Total volume traded */
  totalVolume: string;
  /** Total fees paid */
  totalFees: string;
  /** Average fill rate */
  averageFillRate: number;
}
