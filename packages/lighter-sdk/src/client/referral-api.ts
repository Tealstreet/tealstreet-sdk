/**
 * Referral API Client
 *
 * Provides methods for retrieving referral program information,
 * including points and rewards for account referrals.
 */

import type { LighterApiClient } from './api-client';
import type { ApiResponse } from './http-types';
import { LighterApiError } from './http-types';

/**
 * Referral points information for an account
 */
export interface ReferralPoints {
  /** Account index */
  accountIndex: string; // BigInt as string
  /** Total referral points earned */
  totalPoints: number;
  /** Available points that can be redeemed */
  availablePoints?: number;
  /** Points pending verification */
  pendingPoints?: number;
  /** Number of successful referrals */
  referralCount?: number;
  /** Referral tier or level */
  tier?: string;
  /** Rewards earned from referrals */
  rewards?: {
    /** Total rewards in USD equivalent */
    totalUsd?: string;
    /** Breakdown by token */
    byToken?: Array<{
      /** Token symbol */
      token: string;
      /** Amount */
      amount: string;
    }>;
  };
}

/**
 * Response from the referral points endpoint
 */
export interface ReferralPointsResponse {
  /** Referral points information */
  referralPoints: ReferralPoints;
}

/**
 * Referral API client for the Lighter exchange
 *
 * @example
 * ```typescript
 * const apiClient = new LighterApiClient({ baseUrl: 'https://api.lighter.xyz' });
 * const referralApi = new ReferralApi(apiClient);
 *
 * // Get referral points for account
 * const points = await referralApi.getReferralPoints(
 *   1n,
 *   'auth-token'
 * );
 * console.log('Total points:', points.referralPoints.totalPoints);
 * ```
 */
export class ReferralApi {
  constructor(private readonly client: LighterApiClient) {}

  /**
   * Get referral points information for a specific account
   *
   * @param accountIndex - Account index to query
   * @param authorization - Optional authorization token for authenticated requests
   * @returns Referral points information
   *
   * @example
   * ```typescript
   * const points = await referralApi.getReferralPoints(1n, 'auth-token');
   * console.log('Account', points.referralPoints.accountIndex);
   * console.log('Total points:', points.referralPoints.totalPoints);
   * console.log('Referral count:', points.referralPoints.referralCount);
   * ```
   */
  public async getReferralPoints(
    accountIndex: bigint,
    authorization?: string,
  ): Promise<ReferralPointsResponse> {
    const config = authorization
      ? {
          params: { accountIndex: accountIndex.toString() },
          headers: { Authorization: `Bearer ${authorization}` },
        }
      : {
          params: { accountIndex: accountIndex.toString() },
        };

    const response = await this.client.get<ApiResponse<ReferralPointsResponse>>(
      '/api/v1/referral/points',
      config,
    );

    if (!response.success || !response.data) {
      throw new LighterApiError(
        response.error?.message || 'Failed to get referral points',
        response.error?.code || 'GET_REFERRAL_POINTS_FAILED',
      );
    }

    return response.data;
  }
}
