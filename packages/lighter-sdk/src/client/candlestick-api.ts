/**
 * Candlestick API Client
 *
 * Provides methods for retrieving candlestick (OHLCV) and funding data from the Lighter API.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';
import { LighterApiError } from './http-types';

/**
 * Single candlestick data point (OHLCV)
 */
export interface Candlestick {
  /** Timestamp in milliseconds */
  timestamp: number;
  /** Opening price */
  open: string;
  /** Highest price */
  high: string;
  /** Lowest price */
  low: string;
  /** Closing price */
  close: string;
  /** Trading volume */
  volume: string;
}

/**
 * Response from the candlesticks endpoint
 */
export interface CandlesticksResponse {
  /** Array of candlestick data points */
  candlesticks: Candlestick[];
  /** Market ID */
  marketId: number;
  /** Resolution (e.g., "1m", "5m", "1h", "1d") */
  resolution: string;
}

/**
 * Single funding data point
 */
export interface Funding {
  /** Timestamp in milliseconds */
  timestamp: number;
  /** Funding rate */
  rate: string;
  /** Mark price at the time of funding */
  markPrice?: string;
  /** Index price at the time of funding */
  indexPrice?: string;
}

/**
 * Response from the fundings endpoint
 */
export interface FundingsResponse {
  /** Array of funding data points */
  fundings: Funding[];
  /** Market ID */
  marketId: number;
  /** Resolution (e.g., "1h", "8h") */
  resolution: string;
}

/**
 * Candlestick API client for the Lighter exchange
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const candlestickApi = new CandlestickApi(apiClient);
 *
 * // Get candlesticks for market 1 with 1-hour resolution
 * const candlesticks = await candlestickApi.getCandlesticks(
 *   1,
 *   '1h',
 *   Date.now() - 24 * 60 * 60 * 1000, // 24 hours ago
 *   Date.now(),
 *   100
 * );
 * ```
 */
export class CandlestickApi {
  constructor(private readonly client: LighterApiClient) {}

  /**
   * Get candlestick (OHLCV) data for a specific market
   *
   * @param marketId - Market identifier
   * @param resolution - Time resolution (e.g., "1m", "5m", "15m", "1h", "4h", "1d")
   * @param startTs - Start timestamp in milliseconds
   * @param endTs - End timestamp in milliseconds
   * @param countBack - Maximum number of candlesticks to return
   * @returns Candlestick data
   *
   * @example
   * ```typescript
   * const candlesticks = await candlestickApi.getCandlesticks(
   *   1,              // Market ID
   *   '1h',           // 1-hour resolution
   *   1640000000000,  // Start timestamp
   *   1640100000000,  // End timestamp
   *   100             // Max 100 candlesticks
   * );
   * console.log('Candlesticks:', candlesticks.candlesticks);
   * ```
   */
  public async getCandlesticks(
    marketId: number,
    resolution: string,
    startTs: number,
    endTs: number,
    countBack: number,
  ): Promise<CandlesticksResponse> {
    const response = await this.client.get<ApiResponse<CandlesticksResponse>>(
      '/api/v1/candlesticks',
      {
        params: {
          marketId,
          resolution,
          startTimestamp: startTs,
          endTimestamp: endTs,
          countBack,
        },
      },
    );

    if (!response.success || !response.data) {
      throw new LighterApiError(
        response.error?.message || 'Failed to get candlesticks',
        response.error?.code || 'GET_CANDLESTICKS_FAILED',
      );
    }

    return response.data;
  }

  /**
   * Get funding rate data for a specific perpetual market
   *
   * @param marketId - Market identifier
   * @param resolution - Time resolution (e.g., "1h", "8h")
   * @param startTs - Start timestamp in milliseconds
   * @param endTs - End timestamp in milliseconds
   * @param countBack - Maximum number of funding data points to return
   * @returns Funding rate data
   *
   * @example
   * ```typescript
   * const fundings = await candlestickApi.getFundings(
   *   1,              // Market ID
   *   '8h',           // 8-hour resolution
   *   1640000000000,  // Start timestamp
   *   1640100000000,  // End timestamp
   *   50              // Max 50 data points
   * );
   * console.log('Funding rates:', fundings.fundings);
   * ```
   */
  public async getFundings(
    marketId: number,
    resolution: string,
    startTs: number,
    endTs: number,
    countBack: number,
  ): Promise<FundingsResponse> {
    const response = await this.client.get<ApiResponse<FundingsResponse>>('/api/v1/fundings', {
      params: {
        marketId,
        resolution,
        startTimestamp: startTs,
        endTimestamp: endTs,
        countBack,
      },
    });

    if (!response.success || !response.data) {
      throw new LighterApiError(
        response.error?.message || 'Failed to get fundings',
        response.error?.code || 'GET_FUNDINGS_FAILED',
      );
    }

    return response.data;
  }
}
