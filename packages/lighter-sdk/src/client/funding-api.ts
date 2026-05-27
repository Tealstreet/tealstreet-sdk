/**
 * Funding API Client
 *
 * Provides methods for retrieving funding rate information for perpetual markets.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';
import { LighterApiError } from './http-types';

/**
 * Funding rate information for a single market
 */
export interface FundingRate {
  /** Market identifier */
  marketId: number;
  /** Market symbol (e.g., "BTC-PERP") */
  symbol?: string;
  /** Current funding rate */
  rate: string;
  /** Next funding timestamp in milliseconds */
  nextFundingTime?: number;
  /** Predicted funding rate for the next period */
  predictedRate?: string;
  /** Current mark price */
  markPrice?: string;
  /** Current index price */
  indexPrice?: string;
  /** Last update timestamp in milliseconds */
  timestamp?: number;
}

/**
 * Response from the funding rates endpoint
 */
export interface FundingRatesResponse {
  /** Array of funding rate information for all markets */
  fundingRates: FundingRate[];
}

/**
 * Funding API client for the Lighter exchange
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const fundingApi = new FundingApi(apiClient);
 *
 * // Get current funding rates for all markets
 * const fundingRates = await fundingApi.getFundingRates();
 * console.log('Funding rates:', fundingRates.fundingRates);
 * ```
 */
export class FundingApi {
  constructor(private readonly client: LighterApiClient) {}

  /**
   * Get current funding rates for all perpetual markets
   *
   * @returns Funding rate information for all markets
   *
   * @example
   * ```typescript
   * const fundingRates = await fundingApi.getFundingRates();
   *
   * // Find funding rate for a specific market
   * const btcFunding = fundingRates.fundingRates.find(
   *   rate => rate.marketId === 1
   * );
   * console.log('BTC funding rate:', btcFunding?.rate);
   * ```
   */
  public async getFundingRates(): Promise<FundingRatesResponse> {
    const response = await this.client.get<ApiResponse<FundingRatesResponse>>(
      '/api/v1/funding-rates',
    );

    if (!response.success || !response.data) {
      throw new LighterApiError(
        response.error?.message || 'Failed to get funding rates',
        response.error?.code || 'GET_FUNDING_RATES_FAILED',
      );
    }

    return response.data;
  }
}
