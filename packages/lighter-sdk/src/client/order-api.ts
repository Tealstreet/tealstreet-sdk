/**
 * Order API Client for Lighter SDK
 *
 * Provides methods for querying orders, order books, trades, and exchange statistics.
 */

import type { AxiosRequestConfig } from 'axios';

import { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';
import { OrderStatus } from '../types/constants';
import { Trade, OrderBookLevel } from '../types/common';

type LighterRequestConfig = AxiosRequestConfig & { __suppressExchangeError?: boolean };

/**
 * Order with market details from API
 */
export interface ApiOrder {
  /** Order ID as string */
  orderId: string;
  /** Market ID as number */
  marketId: number;
  /** Market symbol */
  symbol: string;
  /** Account index as number */
  accountIndex: number;
  /** Price as string */
  price: string;
  /** Quantity as string */
  quantity: string;
  /** Filled quantity as string */
  filledQuantity: string;
  /** Remaining quantity as string */
  remainingQuantity: string;
  /** Is buy order */
  isBuy: boolean;
  /** Post only flag */
  postOnly: boolean;
  /** Reduce only flag */
  reduceOnly: boolean;
  /** Order creation timestamp */
  createdAt: number;
  /** Order update timestamp */
  updatedAt: number;
  /** Order status */
  status: OrderStatus;
}

/**
 * Response for account orders
 */
export interface AccountOrdersResponse {
  /** List of orders */
  orders: ApiOrder[];
  /** Total count of orders */
  total?: number;
  /** Next cursor for pagination */
  nextCursor?: string;
}

/**
 * Response for order books
 */
export interface OrderBooksResponse {
  /** List of order books by market */
  orderBooks: Array<{
    /** Market ID */
    marketId: number;
    /** Market symbol */
    symbol: string;
    /** Best bid price */
    bestBid: string;
    /** Best ask price */
    bestAsk: string;
    /** Bid levels */
    bids: OrderBookLevel[];
    /** Ask levels */
    asks: OrderBookLevel[];
    /** Last update timestamp */
    timestamp: number;
  }>;
}

/**
 * Response for order book orders
 */
export interface OrderBookOrdersResponse {
  /** Market ID */
  marketId: number;
  /** Bid orders */
  bids: Array<{
    /** Price level */
    price: string;
    /** Total quantity at this level */
    quantity: string;
    /** Number of orders at this level */
    orderCount: number;
  }>;
  /** Ask orders */
  asks: Array<{
    /** Price level */
    price: string;
    /** Total quantity at this level */
    quantity: string;
    /** Number of orders at this level */
    orderCount: number;
  }>;
  /** Timestamp */
  timestamp: number;
}

/**
 * Response for order book details
 */
export interface OrderBookDetailsResponse {
  /** Order book details by market */
  details: Array<{
    /** Market ID */
    marketId: number;
    /** Market symbol */
    symbol: string;
    /** Total bid volume */
    totalBidVolume: string;
    /** Total ask volume */
    totalAskVolume: string;
    /** Number of bid orders */
    bidOrderCount: number;
    /** Number of ask orders */
    askOrderCount: number;
    /** Mid price */
    midPrice: string;
    /** Spread */
    spread: string;
    /** Spread percentage */
    spreadPercentage: string;
    /** Last update timestamp */
    lastUpdate: number;
  }>;
}

/**
 * Trade information from API
 */
export interface ApiTrade extends Trade {
  /** Trade ID */
  tradeId: string;
  /** Market ID */
  marketId: number;
  /** Market symbol */
  symbol: string;
  /** Trade price */
  price: string;
  /** Trade quantity */
  quantity: string;
  /** Quote quantity */
  quoteQuantity: string;
  /** Trade side */
  side: 'buy' | 'sell';
  /** Is buyer maker */
  isBuyerMaker: boolean;
  /** Trade timestamp */
  timestamp: number;
  /** Maker account index */
  makerAccountIndex?: number;
  /** Taker account index */
  takerAccountIndex?: number;
}

/**
 * Response for trades
 */
export interface TradesResponse {
  /** List of trades */
  trades: ApiTrade[];
  /** Total count of trades */
  total?: number;
  /** Next cursor for pagination */
  nextCursor?: string;
}

/**
 * Response for recent trades
 */
export interface RecentTradesResponse {
  /** Market ID */
  marketId: number;
  /** Recent trades */
  trades: ApiTrade[];
}

/**
 * Exchange statistics
 */
export interface ExchangeStats {
  /** Total trading volume (24h) */
  volume24h: string;
  /** Total number of trades (24h) */
  trades24h: number;
  /** Total number of markets */
  totalMarkets: number;
  /** Total number of active orders */
  activeOrders: number;
  /** Total number of accounts */
  totalAccounts: number;
  /** Total value locked */
  tvl: string;
  /** Statistics by market */
  marketStats: Array<{
    /** Market ID */
    marketId: number;
    /** Market symbol */
    symbol: string;
    /** 24h volume */
    volume24h: string;
    /** 24h trades */
    trades24h: number;
    /** Price change (24h) */
    priceChange24h: string;
    /** Price change percentage (24h) */
    priceChangePercent24h: string;
    /** High price (24h) */
    high24h: string;
    /** Low price (24h) */
    low24h: string;
    /** Last price */
    lastPrice: string;
  }>;
  /** Statistics timestamp */
  timestamp: number;
}

/**
 * Response for exchange statistics
 */
export interface ExchangeStatsResponse {
  /** Exchange statistics */
  stats: ExchangeStats;
}

/**
 * Options for getting inactive orders
 */
export interface GetInactiveOrdersOptions {
  /** Market ID filter */
  marketId?: number;
  /** Maximum number of orders to return (1-100) */
  limit?: number;
  /** Filter for ask orders (true) or bid orders (false) */
  askFilter?: boolean;
  /** Filter by timestamp range [start, end] */
  betweenTimestamps?: [number, number];
  /** Pagination cursor */
  cursor?: string;
  /** Sort by field */
  sortBy?: 'createdAt' | 'updatedAt' | 'price';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
}

/**
 * Options for getting trades
 */
export interface GetTradesOptions {
  /** Market ID filter */
  marketId?: number;
  /** Account index filter */
  accountIndex?: bigint;
  /** Maximum number of trades to return (1-100) */
  limit?: number;
  /** Start time filter (unix timestamp) */
  startTime?: number;
  /** End time filter (unix timestamp) */
  endTime?: number;
  /** Pagination cursor */
  cursor?: string;
  /** Sort by field */
  sortBy?: 'timestamp' | 'price';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
}

/**
 * Order API Client
 *
 * Provides methods for querying orders, order books, trades, and exchange statistics.
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const orderApi = new OrderApi(apiClient);
 *
 * // Get active orders for an account
 * const activeOrders = await orderApi.getAccountActiveOrders(1n, 0);
 *
 * // Get order books
 * const orderBooks = await orderApi.getOrderBooks();
 *
 * // Get recent trades
 * const recentTrades = await orderApi.getRecentTrades(0, 50);
 * ```
 */
export class OrderApi {
  private readonly client: LighterApiClient;

  /**
   * Creates a new OrderApi instance
   *
   * @param client - LighterApiClient instance to use for HTTP requests
   */
  constructor(client: LighterApiClient) {
    this.client = client;
  }

  /**
   * Get active orders for an account
   *
   * Retrieves all active (open) orders for a specific account. Can optionally filter by market ID.
   *
   * @param accountIndex - Account index to query
   * @param marketId - Optional market ID to filter orders
   * @returns List of active orders
   *
   * @example
   * ```typescript
   * // Get all active orders for account
   * const orders = await orderApi.getAccountActiveOrders(1n);
   *
   * // Get active orders for specific market
   * const marketOrders = await orderApi.getAccountActiveOrders(1n, 0);
   * ```
   */
  public async getAccountActiveOrders(
    accountIndex: bigint,
    marketId?: number,
  ): Promise<AccountOrdersResponse> {
    const params: Record<string, string> = {
      account_index: accountIndex.toString(),
    };

    if (marketId !== undefined) {
      params.market_id = marketId.toString();
    }

    const response = await this.client.get<ApiResponse<AccountOrdersResponse>>(
      '/api/v1/accountActiveOrders',
      { params },
    );

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get active orders');
    }

    return response.data;
  }

  /**
   * Get inactive orders for an account
   *
   * Retrieves historical (filled, cancelled, expired) orders for a specific account.
   * Supports filtering by market, status, time range, and pagination.
   *
   * @param accountIndex - Account index to query
   * @param options - Optional filtering and pagination options
   * @returns List of inactive orders with pagination info
   *
   * @example
   * ```typescript
   * // Get recent inactive orders
   * const orders = await orderApi.getAccountInactiveOrders(1n, { limit: 50 });
   *
   * // Get inactive orders for specific market within time range
   * const marketOrders = await orderApi.getAccountInactiveOrders(1n, {
   *   marketId: 0,
   *   betweenTimestamps: [startTime, endTime],
   *   limit: 100
   * });
   * ```
   */
  public async getAccountInactiveOrders(
    accountIndex: bigint,
    options?: GetInactiveOrdersOptions,
  ): Promise<AccountOrdersResponse> {
    const params: Record<string, string> = {
      account_index: accountIndex.toString(),
      limit: (options?.limit || 50).toString(),
    };

    if (options?.marketId !== undefined) {
      params.market_id = options.marketId.toString();
    }

    if (options?.askFilter !== undefined) {
      params.ask_filter = options.askFilter.toString();
    }

    if (options?.betweenTimestamps) {
      params.start_time = options.betweenTimestamps[0].toString();
      params.end_time = options.betweenTimestamps[1].toString();
    }

    if (options?.cursor) {
      params.cursor = options.cursor;
    }

    if (options?.sortBy) {
      params.sort_by = options.sortBy;
    }

    if (options?.sortOrder) {
      params.sort_order = options.sortOrder;
    }

    const response = await this.client.get<ApiResponse<AccountOrdersResponse>>(
      '/api/v1/accountInactiveOrders',
      { params },
    );

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get inactive orders');
    }

    return response.data;
  }

  /**
   * Get order books for all markets or a specific market
   *
   * Retrieves current order book snapshots with bid/ask levels.
   *
   * @param marketId - Optional market ID to get specific order book
   * @returns Order books with bid/ask levels
   *
   * @example
   * ```typescript
   * // Get all order books
   * const allOrderBooks = await orderApi.getOrderBooks();
   *
   * // Get order book for specific market
   * const marketOrderBook = await orderApi.getOrderBooks(0);
   * ```
   */
  public async getOrderBooks(marketId?: number): Promise<OrderBooksResponse> {
    const params: Record<string, string> = {};

    if (marketId !== undefined) {
      params.market_id = marketId.toString();
    }

    const response = await this.client.get<ApiResponse<OrderBooksResponse>>(
      '/api/v1/order-books',
      { params },
    );

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get order books');
    }

    return response.data;
  }

  /**
   * Get order book orders for a specific market
   *
   * Retrieves detailed order information for each price level in the order book.
   *
   * @param marketId - Market ID to query
   * @param limit - Maximum number of price levels per side (default: 50, max: 100)
   * @returns Order book orders with aggregated price levels
   *
   * @example
   * ```typescript
   * // Get top 20 levels of order book
   * const orderBookOrders = await orderApi.getOrderBookOrders(0, 20);
   * ```
   */
  public async getOrderBookOrders(
    marketId: number,
    limit?: number,
    requestConfig?: LighterRequestConfig,
  ): Promise<OrderBookOrdersResponse> {
    const params: Record<string, string> = {
      limit: (limit || 50).toString(),
    };

    const response = await this.client.get<any>(
      '/api/v1/orderBookOrders',
      {
        ...requestConfig,
        params: { ...params, ...(requestConfig?.params || {}), market_id: marketId.toString() },
      } as AxiosRequestConfig,
    );

    if (response?.success && response.data) {
      return response.data;
    }

    if (response?.code === 200 && (Array.isArray(response.bids) || Array.isArray(response.asks))) {
      return {
        marketId,
        bids: response.bids || [],
        asks: response.asks || [],
        timestamp: response.timestamp || Date.now(),
      };
    }

    throw new Error(response?.error?.message || response?.message || 'Failed to get order book orders');
  }

  /**
   * Get order book details for markets
   *
   * Retrieves statistical information about order books including volume, spread, and order counts.
   *
   * @param marketId - Optional market ID to get specific market details
   * @returns Order book details with statistics
   *
   * @example
   * ```typescript
   * // Get details for all markets
   * const allDetails = await orderApi.getOrderBookDetails();
   *
   * // Get details for specific market
   * const marketDetails = await orderApi.getOrderBookDetails(0);
   * ```
   */
  public async getOrderBookDetails(marketId?: number): Promise<OrderBookDetailsResponse> {
    const params: Record<string, string> = {};

    if (marketId !== undefined) {
      params.market_id = marketId.toString();
    }

    const response = await this.client.get<ApiResponse<OrderBookDetailsResponse>>(
      '/api/v1/order-books/details',
      { params },
    );

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get order book details');
    }

    return response.data;
  }

  /**
   * Get recent trades for a market
   *
   * Retrieves the most recent trades executed on a specific market.
   *
   * @param marketId - Market ID to query
   * @param limit - Maximum number of trades to return (default: 50, max: 100)
   * @returns Recent trades for the market
   *
   * @example
   * ```typescript
   * // Get last 50 trades
   * const recentTrades = await orderApi.getRecentTrades(0);
   *
   * // Get last 100 trades
   * const moreTrades = await orderApi.getRecentTrades(0, 100);
   * ```
   */
  public async getRecentTrades(marketId: number, limit?: number): Promise<RecentTradesResponse> {
    const params: Record<string, string> = {
      limit: (limit || 50).toString(),
    };

    const response = await this.client.get<ApiResponse<RecentTradesResponse>>(
      `/api/v1/trades/recent/${marketId}`,
      { params },
    );

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get recent trades');
    }

    return response.data;
  }

  /**
   * Get trades with filtering options
   *
   * Retrieves trades with advanced filtering by market, account, time range, and pagination.
   *
   * @param options - Optional filtering and pagination options
   * @returns Filtered trades with pagination info
   *
   * @example
   * ```typescript
   * // Get trades for specific market
   * const marketTrades = await orderApi.getTrades({ marketId: 0, limit: 100 });
   *
   * // Get trades for specific account within time range
   * const accountTrades = await orderApi.getTrades({
   *   accountIndex: 1n,
   *   startTime: Date.now() - 86400000, // Last 24h
   *   endTime: Date.now(),
   *   limit: 50
   * });
   * ```
   */
  public async getTrades(options?: GetTradesOptions): Promise<TradesResponse> {
    const params: Record<string, string> = {
      limit: (options?.limit || 50).toString(),
    };

    if (options?.marketId !== undefined) {
      params.market_id = options.marketId.toString();
    }

    if (options?.accountIndex !== undefined) {
      params.account_index = options.accountIndex.toString();
    }

    if (options?.startTime !== undefined) {
      params.start_time = options.startTime.toString();
    }

    if (options?.endTime !== undefined) {
      params.end_time = options.endTime.toString();
    }

    if (options?.cursor) {
      params.cursor = options.cursor;
    }

    if (options?.sortBy) {
      params.sort_by = options.sortBy;
    }

    if (options?.sortOrder) {
      params.sort_order = options.sortOrder;
    }

    const response = await this.client.get<ApiResponse<TradesResponse>>('/api/v1/trades', {
      params,
    });

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get trades');
    }

    return response.data;
  }

  /**
   * Get exchange-wide statistics
   *
   * Retrieves overall exchange statistics including trading volume, number of trades,
   * active orders, and per-market statistics.
   *
   * @returns Exchange statistics
   *
   * @example
   * ```typescript
   * const stats = await orderApi.getExchangeStats();
   * console.log('24h Volume:', stats.stats.volume24h);
   * console.log('Total Markets:', stats.stats.totalMarkets);
   * ```
   */
  public async getExchangeStats(): Promise<ExchangeStatsResponse> {
    const response = await this.client.get<ApiResponse<ExchangeStatsResponse>>('/api/v1/stats');

    if (!response.success || !response.data) {
      throw new Error(response.error?.message || 'Failed to get exchange stats');
    }

    return response.data;
  }
}
