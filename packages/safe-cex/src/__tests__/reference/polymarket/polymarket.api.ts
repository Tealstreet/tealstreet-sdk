/**
 * Polymarket REST API Client
 *
 * Handles all HTTP communication with the Polymarket CLOB API.
 * L1 methods are public (no auth), L2 methods require API credentials.
 *
 * Reference: https://docs.polymarket.com/
 */

import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../../types';
import type { BaseExchange } from '../../../exchanges/base';
import type {
  GammaMarketInfo,
  PolymarketAPIOrder,
  PolymarketBalanceAllowance,
  PolymarketClobMarket,
  PolymarketCredentials,
  PolymarketDataPosition,
  PolymarketOrderBook,
  PolymarketOrderResponse,
  PolymarketPrice,
  PolymarketSignedOrder,
  PolymarketTrade,
} from './polymarket.types';

import { orderToJsonV2, OrderType as PolymarketOrderType } from '@polymarket/clob-client-v2';
import retry from 'axios-retry';
import qs from 'qs';

import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { LogSeverity } from '../../../types';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import { ENDPOINTS, POLYMARKET_CONFIG, RATE_LIMITS } from './polymarket.constants';
import { buildL2Headers } from './polymarket.signing';

const INITIAL_CURSOR = 'MA==';
const END_CURSOR = 'LTE=';

/** Rate limiter state */
interface RateLimiterState {
  lastRequestTime: number;
  requestCount: number;
}

/**
 * Create the Polymarket API client
 */
export function createPolymarketAPI(
  options: ExchangeOptions,
  exchange: BaseExchange,
  getCredentials: () => PolymarketCredentials | null,
  getWalletAddress: () => string | null,
): AxiosInstance {
  const baseURL = POLYMARKET_CONFIG.REST_BASE;

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? 30000,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'comma' }),
      },
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    },
    options,
    exchange,
  );

  // Retry on network errors; order books and midpoints are POSTed reads.
  retry(xhr, {
    retries: 3,
    retryCondition: networkErrorOnRead(readsAndPosted(ENDPOINTS.ORDER_BOOKS, ENDPOINTS.MIDPOINTS)),
    retryDelay: (retryCount) => retryCount * 1000,
  });

  // Rate limiting state
  const rateLimiter: RateLimiterState = {
    lastRequestTime: 0,
    requestCount: 0,
  };

  // Request interceptor for authentication and rate limiting
  xhr.interceptors.request.use(async (config: InternalAxiosRequestConfig) => {
    const url = config.url || '';

    // Public endpoints (L1) - no auth required
    const publicEndpoints = [
      ENDPOINTS.MARKETS,
      ENDPOINTS.SIMPLIFIED_MARKETS,
      ENDPOINTS.ORDER_BOOK,
      ENDPOINTS.ORDER_BOOKS,
      ENDPOINTS.PRICE,
      ENDPOINTS.PRICES,
      ENDPOINTS.MIDPOINT,
      ENDPOINTS.MIDPOINTS,
      ENDPOINTS.SPREAD,
      ENDPOINTS.SPREADS,
      ENDPOINTS.LAST_TRADE_PRICE,
      ENDPOINTS.PRICE_HISTORY,
      ENDPOINTS.TIME,
      ENDPOINTS.DERIVE_API_KEY,
    ];

    const isPublic = publicEndpoints.some((endpoint) => url.startsWith(endpoint));

    // Apply rate limiting
    const now = Date.now();
    const rateLimit = isPublic ? RATE_LIMITS.PUBLIC_RPS : RATE_LIMITS.PRIVATE_RPS;
    const minInterval = 1000 / rateLimit;

    if (now - rateLimiter.lastRequestTime < minInterval) {
      const delay = minInterval - (now - rateLimiter.lastRequestTime);
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
    rateLimiter.lastRequestTime = Date.now();

    // Add auth headers for private endpoints
    if (!isPublic) {
      const credentials = getCredentials();
      if (!credentials) {
        exchange.log('[Polymarket API] No credentials available for private endpoint', LogSeverity.Error);
        throw new Error('Polymarket credentials not initialized');
      }

      const walletAddress = getWalletAddress();
      if (!walletAddress) {
        throw new Error('Polymarket wallet address not initialized');
      }

      const method = config.method?.toUpperCase() || 'GET';
      const path = config.url || '';
      const body =
        config.data === undefined
          ? undefined
          : typeof config.data === 'string'
            ? config.data
            : JSON.stringify(config.data);

      const headers = buildL2Headers(credentials, walletAddress, method, path, body);

      Object.entries(headers).forEach(([key, value]) => {
        config.headers.set(key, value);
      });

      exchange.log(`[Polymarket API] Added L2 headers for ${method} ${path}`, LogSeverity.Debug);
    }

    return config;
  });

  // Response interceptor for error handling
  xhr.interceptors.response.use(
    (response) => {
      // Check for error in response body (some APIs return 200 with error)
      if (response.data?.error) {
        const errorMessage = response.data.message || response.data.error;
        const error: any = new Error(`Polymarket Error: ${errorMessage}`);
        error.isAxiosError = true;
        error.response = response;
        error.config = response.config;
        return Promise.reject(error);
      }
      return response;
    },
    (error) => {
      // Enhanced error handling
      if (error.response?.data) {
        const data = error.response.data;
        const errorMessage = data.message || data.error || 'Unknown error';
        exchange.log(`[Polymarket API] Error: ${errorMessage}`, LogSeverity.Error);

        // Create enhanced error with Polymarket details
        const enhancedError = Object.create(error);
        Object.defineProperty(enhancedError, 'message', {
          value: `Polymarket Error: ${errorMessage}`,
          writable: false,
          enumerable: true,
        });
        return Promise.reject(enhancedError);
      }
      return Promise.reject(error);
    },
  );

  return xhr;
}

/**
 * Polymarket API wrapper class with typed methods
 */
export class PolymarketAPI {
  private xhr: AxiosInstance;
  private gammaXhr: AxiosInstance;
  private dataXhr: AxiosInstance;
  private exchange: BaseExchange;

  constructor(
    options: ExchangeOptions,
    exchange: BaseExchange,
    private getCredentials: () => PolymarketCredentials | null,
    getWalletAddress: () => string | null,
  ) {
    this.exchange = exchange;
    this.xhr = createPolymarketAPI(options, exchange, getCredentials, getWalletAddress);

    // Separate client for Gamma API (market metadata)
    this.gammaXhr = safeCexAxiosCreate(
      {
        baseURL: POLYMARKET_CONFIG.GAMMA_API,
        timeout: 30000,
        paramsSerializer: {
          // Gamma accepts repeated slug= values; slug[]= is silently ignored.
          serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
        },
        headers: {
          'Content-Type': 'application/json',
          Accept: 'application/json',
        },
      },
      options,
      exchange,
    );

    retry(this.gammaXhr, {
      retries: 3,
      retryCondition: networkErrorOnRead(),
      retryDelay: (retryCount) => retryCount * 1000,
    });

    this.dataXhr = safeCexAxiosCreate(
      {
        baseURL: POLYMARKET_CONFIG.DATA_API,
        timeout: 30000,
        headers: { Accept: 'application/json' },
      },
      options,
      exchange,
    );

    retry(this.dataXhr, {
      retries: 3,
      retryCondition: networkErrorOnRead(),
      retryDelay: (retryCount) => retryCount * 1000,
    });
  }

  // ==================== L1 Public Methods ====================

  /**
   * Get all CLOB markets
   */
  async getMarkets(): Promise<PolymarketClobMarket[]> {
    const { data } = await this.xhr.get<PolymarketClobMarket[]>(ENDPOINTS.MARKETS);
    return data;
  }

  /** Get the CLOB server timestamp in Unix seconds. */
  async getServerTime(): Promise<number> {
    const { data } = await this.xhr.get<number>(ENDPOINTS.TIME);
    return data;
  }

  /**
   * Get simplified markets from CLOB
   */
  async getSimplifiedMarkets(): Promise<any[]> {
    const { data } = await this.xhr.get(ENDPOINTS.SIMPLIFIED_MARKETS);
    return data;
  }

  /**
   * Get order book for a token
   */
  async getOrderBook(tokenId: string): Promise<PolymarketOrderBook> {
    const { data } = await this.xhr.get<PolymarketOrderBook>(ENDPOINTS.ORDER_BOOK, {
      params: { token_id: tokenId },
    });
    return data;
  }

  /**
   * Get order books for multiple tokens
   */
  async getOrderBooks(tokenIds: string[]): Promise<PolymarketOrderBook[]> {
    const { data } = await this.xhr.post<PolymarketOrderBook[]>(
      ENDPOINTS.ORDER_BOOKS,
      tokenIds.map((tokenId) => ({ token_id: tokenId })),
    );
    return data;
  }

  /**
   * Get midpoint price for a token
   */
  async getMidpoint(tokenId: string): Promise<number> {
    const { data } = await this.xhr.get<{ mid: string }>(ENDPOINTS.MIDPOINT, {
      params: { token_id: tokenId },
    });
    return parseFloat(data.mid);
  }

  /**
   * Get midpoints for multiple tokens
   */
  async getMidpoints(tokenIds: string[]): Promise<Record<string, number>> {
    const { data } = await this.xhr.post<Record<string, string>>(
      ENDPOINTS.MIDPOINTS,
      tokenIds.map((tokenId) => ({ token_id: tokenId })),
    );
    return Object.fromEntries(Object.entries(data).map(([id, price]) => [id, parseFloat(price)]));
  }

  /**
   * Get price for a token and side
   */
  async getPrice(tokenId: string, side: 'BUY' | 'SELL'): Promise<number> {
    const { data } = await this.xhr.get<{ price: string }>(ENDPOINTS.PRICE, {
      params: { token_id: tokenId, side },
    });
    return parseFloat(data.price);
  }

  /**
   * Get prices for multiple tokens
   */
  async getPrices(tokenIds: string[]): Promise<PolymarketPrice[]> {
    const { data } = await this.xhr.get<PolymarketPrice[]>(ENDPOINTS.PRICES, {
      params: { token_ids: tokenIds.join(',') },
    });
    return data;
  }

  /**
   * Get last trade price for a token
   */
  async getLastTradePrice(tokenId: string): Promise<number> {
    const { data } = await this.xhr.get<{ price: string }>(ENDPOINTS.LAST_TRADE_PRICE, {
      params: { token_id: tokenId },
    });
    return parseFloat(data.price);
  }

  /**
   * Get price history for a token
   */
  async getPriceHistory(
    tokenId: string,
    startTs?: number,
    endTs?: number,
    interval?: string,
    fidelity?: number,
  ): Promise<Array<{ t: number; p: string }>> {
    const { data } = await this.xhr.get(ENDPOINTS.PRICE_HISTORY, {
      params: {
        market: tokenId,
        startTs,
        endTs,
        interval,
        fidelity,
      },
    });
    return data.history || [];
  }

  /**
   * Get spread for a token
   */
  async getSpread(tokenId: string): Promise<{ bid: string; ask: string; spread: string }> {
    const { data } = await this.xhr.get(ENDPOINTS.SPREAD, {
      params: { token_id: tokenId },
    });
    return data;
  }

  // ==================== L2 Private Methods ====================

  /**
   * Place a single order
   */
  async postOrder(
    order: PolymarketSignedOrder,
    orderType: 'GTC' | 'FOK' | 'GTD' | 'FAK',
  ): Promise<PolymarketOrderResponse> {
    const credentials = this.getCredentials();
    if (!credentials) {
      throw new Error('Credentials not initialized');
    }

    const payload = orderToJsonV2(order as any, credentials.apiKey, orderType as PolymarketOrderType);
    const { data } = await this.xhr.post<PolymarketOrderResponse>(ENDPOINTS.ORDER, payload);

    return data;
  }

  /**
   * Place multiple orders (batch)
   */
  async postOrders(
    orders: Array<{ order: PolymarketSignedOrder; orderType: string }>,
  ): Promise<PolymarketOrderResponse[]> {
    const credentials = this.getCredentials();
    if (!credentials) {
      throw new Error('Credentials not initialized');
    }

    const { data } = await this.xhr.post<PolymarketOrderResponse[]>(ENDPOINTS.ORDERS, {
      orders: orders.map((o) => ({
        order: o.order,
        owner: credentials.apiKey,
        orderType: o.orderType,
      })),
    });

    return data;
  }

  /**
   * Get open orders
   */
  async getOpenOrders(market?: string): Promise<PolymarketAPIOrder[]> {
    const orders: PolymarketAPIOrder[] = [];
    let nextCursor = INITIAL_CURSOR;

    while (nextCursor !== END_CURSOR) {
      const params: Record<string, string> = { next_cursor: nextCursor };
      if (market) params.market = market;
      const { data } = await this.xhr.get<{ data: PolymarketAPIOrder[]; next_cursor: string } | PolymarketAPIOrder[]>(
        ENDPOINTS.OPEN_ORDERS,
        { params },
      );

      if (Array.isArray(data)) return data;
      orders.push(...(data.data || []));
      nextCursor = data.next_cursor || END_CURSOR;
    }

    return orders;
  }

  /**
   * Get a specific order by ID
   */
  async getOrder(orderId: string): Promise<PolymarketAPIOrder> {
    const { data } = await this.xhr.get<PolymarketAPIOrder>(`${ENDPOINTS.GET_ORDER}${orderId}`);
    return data;
  }

  /**
   * Cancel a single order
   */
  async cancelOrder(orderId: string): Promise<void> {
    await this.xhr.delete(ENDPOINTS.CANCEL, {
      data: { orderID: orderId },
    });
  }

  /**
   * Cancel multiple orders
   */
  async cancelOrders(orderIds: string[]): Promise<void> {
    await this.xhr.delete(ENDPOINTS.CANCEL_ORDERS, {
      data: orderIds,
    });
  }

  /**
   * Cancel all orders for a market
   */
  async cancelMarketOrders(market: string): Promise<void> {
    await this.xhr.delete(ENDPOINTS.CANCEL_MARKET_ORDERS, {
      data: { market },
    });
  }

  /**
   * Cancel all open orders
   */
  async cancelAllOrders(): Promise<void> {
    await this.xhr.delete(ENDPOINTS.CANCEL_ALL);
  }

  /**
   * Get user trades
   */
  async getTrades(market?: string, limit?: number): Promise<PolymarketTrade[]> {
    const trades: PolymarketTrade[] = [];
    let nextCursor = INITIAL_CURSOR;

    while (nextCursor !== END_CURSOR) {
      const params: Record<string, any> = { next_cursor: nextCursor };
      if (market) params.market = market;
      if (limit) params.limit = limit;
      const { data } = await this.xhr.get<{ data: PolymarketTrade[]; next_cursor: string } | PolymarketTrade[]>(
        ENDPOINTS.TRADES,
        { params },
      );

      if (Array.isArray(data)) return data;
      trades.push(...(data.data || []));
      nextCursor = data.next_cursor || END_CURSOR;
      if (limit && trades.length >= limit) return trades.slice(0, limit);
    }

    return trades;
  }

  /**
   * Get balance and allowance
   */
  async getBalanceAllowance(signatureType: number): Promise<PolymarketBalanceAllowance> {
    const { data } = await this.xhr.get<PolymarketBalanceAllowance>(ENDPOINTS.BALANCE_ALLOWANCE, {
      params: { asset_type: 'COLLATERAL', signature_type: signatureType },
    });
    return data;
  }

  // ==================== Auth Methods ====================

  /**
   * Derive API credentials from wallet signature
   */
  async deriveApiKey(
    address: string,
    nonce: number,
    signature: string,
    timestamp: number,
  ): Promise<PolymarketCredentials> {
    const { data } = await this.xhr.get<{
      apiKey: string;
      secret: string;
      passphrase: string;
    }>(ENDPOINTS.DERIVE_API_KEY, {
      headers: {
        POLY_ADDRESS: address,
        POLY_SIGNATURE: signature,
        POLY_TIMESTAMP: timestamp.toString(),
        POLY_NONCE: nonce.toString(),
      },
    });

    return data;
  }

  /**
   * Delete an API key
   */
  async deleteApiKey(): Promise<void> {
    await this.xhr.delete(ENDPOINTS.DELETE_API_KEY);
  }

  // ==================== Gamma API Methods ====================

  /**
   * Get all markets from Gamma API (includes metadata)
   */
  async getGammaMarkets(params?: {
    active?: boolean;
    closed?: boolean;
    archived?: boolean;
    slug?: string[];
    limit?: number;
    offset?: number;
  }): Promise<GammaMarketInfo[]> {
    const { limit: requestedLimit = 100, offset, ...filters } = params ?? {};
    const slugs = filters.slug;
    if (slugs !== undefined && (!Array.isArray(slugs) || slugs.length === 0 || slugs.length > 100 ||
        slugs.some((slug) => typeof slug !== 'string' || !slug.trim()) || new Set(slugs).size !== slugs.length)) {
      throw new Error('Polymarket Gamma slug filter must contain 1-100 unique nonempty strings');
    }
    if (!Number.isInteger(requestedLimit) || requestedLimit <= 0) {
      throw new Error('Polymarket Gamma limit must be a positive integer');
    }
    if (offset !== undefined && offset !== 0) {
      throw new Error('Polymarket Gamma keyset pagination does not support a nonzero offset');
    }
    // Offset pagination is capped at 10,000; keyset cursors enumerate the whole
    // universe. Its effective page limit is 100, even if callers request more.
    const limit = Math.min(requestedLimit, 100);
    const markets: GammaMarketInfo[] = [];
    const cursors = new Set<string>();
    let cursor: string | undefined;
    while (true) {
      const { data } = await this.gammaXhr.get<{ markets: GammaMarketInfo[]; next_cursor?: string }>('/markets/keyset', {
        params: { ...filters, limit, ...(cursor !== undefined ? { after_cursor: cursor } : {}) },
      });
      if (!data || !Array.isArray(data.markets) || data.markets.length > limit || data.markets.some((market) => !market || typeof market !== 'object' || Array.isArray(market))) {
        throw new Error('Invalid Polymarket Gamma keyset market page');
      }
      const next = data.next_cursor;
      if (next === undefined) {
        if (data.markets.length === limit) throw new Error('Polymarket Gamma full keyset page is missing its cursor');
        markets.push(...data.markets);
        return markets;
      }
      if (typeof next !== 'string' || !next.trim() || cursors.has(next)) {
        throw new Error('Invalid or repeated Polymarket Gamma keyset cursor');
      }
      cursors.add(next);
      markets.push(...data.markets);
      cursor = next;
    }
  }

  /** Get complete current holdings from the Data API. */
  async getPositions(user: string): Promise<PolymarketDataPosition[]> {
    const limit = 500;
    let offset = 0;
    const positions: PolymarketDataPosition[] = [];

    while (true) {
      const { data } = await this.dataXhr.get<PolymarketDataPosition[]>(ENDPOINTS.POSITIONS, {
        params: { user, limit, offset, sizeThreshold: 0 },
      });
      positions.push(...data);
      if (data.length < limit) return positions;
      offset += limit;
    }
  }

  /**
   * Get a specific market from Gamma API
   */
  async getGammaMarket(conditionId: string): Promise<GammaMarketInfo> {
    const { data } = await this.gammaXhr.get<GammaMarketInfo>(`/markets/${conditionId}`);
    return data;
  }

  /**
   * Get market by slug from Gamma API
   */
  async getGammaMarketBySlug(slug: string): Promise<GammaMarketInfo> {
    const { data } = await this.gammaXhr.get<GammaMarketInfo>(`/markets/slug/${slug}`);
    return data;
  }

  /**
   * Search markets from Gamma API
   */
  async searchGammaMarkets(query: string): Promise<GammaMarketInfo[]> {
    const { data } = await this.gammaXhr.get<GammaMarketInfo[]>('/markets', {
      params: { search: query },
    });
    return data;
  }
}
