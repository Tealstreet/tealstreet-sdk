/**
 * Info API Client
 *
 * Provides methods for retrieving general information about the Lighter exchange,
 * such as withdrawal delays and system parameters.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';
import { LighterApiError } from './http-types';

/**
 * Withdrawal delay information
 */
export interface WithdrawalDelay {
  /** Withdrawal delay in seconds */
  delaySeconds: number;
  /** Withdrawal delay in milliseconds */
  delayMs?: number;
  /** Human-readable description of the delay */
  description?: string;
}

/**
 * Response from the withdrawal delay endpoint
 */
export interface WithdrawalDelayResponse {
  /** Withdrawal delay information */
  withdrawalDelay: WithdrawalDelay;
}

/**
 * Info API client for the Lighter exchange
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const infoApi = new InfoApi(apiClient);
 *
 * // Get withdrawal delay information
 * const delayInfo = await infoApi.getWithdrawalDelay();
 * console.log('Withdrawal delay:', delayInfo.withdrawalDelay.delaySeconds, 'seconds');
 * ```
 */
export class InfoApi {
  constructor(private readonly client: LighterApiClient) {}

  /**
   * Get the current withdrawal delay for the exchange
   *
   * Withdrawal requests must wait for this delay period before they can be processed.
   * This is a security feature to protect against unauthorized withdrawals.
   *
   * @returns Withdrawal delay information
   *
   * @example
   * ```typescript
   * const delayInfo = await infoApi.getWithdrawalDelay();
   * console.log(
   *   'Withdrawals take',
   *   delayInfo.withdrawalDelay.delaySeconds,
   *   'seconds to process'
   * );
   * ```
   */
  public async getWithdrawalDelay(): Promise<WithdrawalDelayResponse> {
    const response = await this.client.get<ApiResponse<WithdrawalDelayResponse>>(
      '/api/v1/withdrawalDelay',
    );

    if (!response.success || !response.data) {
      throw new LighterApiError(
        response.error?.message || 'Failed to get withdrawal delay',
        response.error?.code || 'GET_WITHDRAWAL_DELAY_FAILED',
      );
    }

    return response.data;
  }
}
