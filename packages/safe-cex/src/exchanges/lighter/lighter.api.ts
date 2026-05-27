import type { ExchangeOptions } from '../../types';

import { LighterApiClient } from '@tealstreet/lighter-sdk';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BASE_URL, RECV_WINDOW } from './lighter.types';
import { toLighterApiKeyIndexString } from './lighter.utils';

/**
 * Create a Lighter SDK API client with enhanced features
 * @param options - Exchange options
 * @param exchange - Exchange instance
 * @returns LighterApiClient instance
 */
export const createLighterSDKClient = (options: ExchangeOptions, exchange: BaseExchange): LighterApiClient => {
  const baseUrl = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

  const client = new LighterApiClient({
    baseUrl,
    timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
    debug: false,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  // Add request interceptor for timestamps and account index
  client.addRequestInterceptor((config: any) => {
    // Skip auth headers for public endpoints
    if (config.url?.includes('/public/') || config.url?.includes('/markets') || config.url?.includes('/tickers')) {
      return config;
    }

    // Add timestamp and account index for authenticated endpoints
    const timestamp = virtualClock.getCurrentTimeMs();
    config.headers = {
      ...config.headers,
      'X-Timestamp': timestamp.toString(),
      'X-Account-Index': toLighterApiKeyIndexString(options.apiKeyIndex),
    };

    return config;
  });

  // Add response interceptor for error handling
  client.addResponseInterceptor((response: any) => {
    if (response.data?.error) {
      const msg = `[${response.data.error.code}] ${response.data.error.message}`;
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }

    if (response.data?.success === false) {
      const msg = response.data.error?.message || 'Request failed';
      exchange.emitAndLogError(msg);
      throw new Error(msg);
    }

    return response;
  });

  // Add error interceptor to emit errors to exchange
  client.addErrorInterceptor((error: any) => {
    const suppressExchangeError = Boolean(error?.config?.__suppressExchangeError);
    if (suppressExchangeError) {
      return error;
    }

    const requestUrl = String(error?.config?.url || '');
    const isSendTxRequest = requestUrl.includes('/api/v1/sendTx');
    // sendTx errors are handled by signer retry/caller flow; avoid duplicate
    // exchange-level errors for transient/recoverable first-attempt failures.
    if (isSendTxRequest) {
      return error;
    }
    if (error.message) {
      exchange.emitAndLogError(error.message);
    }
    return error;
  });

  return client;
};
