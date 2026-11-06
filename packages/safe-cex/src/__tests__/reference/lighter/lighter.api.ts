import type { ExchangeOptions } from '../../../types';

import { LighterApiClient } from '@tealstreet/lighter-sdk';
import { BaseExchange } from '../../../exchanges/base';
import { getLighterApiBaseUrl, RECV_WINDOW } from '../../../exchanges/lighter/lighter.types';

const UNSUPPORTED_LIGHTER_REST_HEADERS = ['x-timestamp', 'x-account-index'];

export const stripUnsupportedLighterRestHeaders = (headers: unknown): void => {
  if (!headers || typeof headers !== 'object') {
    return;
  }

  const headerRecord = headers as Record<string, unknown> & { delete?: (name: string) => void };

  for (const unsupportedHeader of UNSUPPORTED_LIGHTER_REST_HEADERS) {
    if (typeof headerRecord.delete === 'function') {
      headerRecord.delete(unsupportedHeader);
    }

    for (const headerName of Object.keys(headerRecord)) {
      if (headerName.toLowerCase() === unsupportedHeader) {
        delete headerRecord[headerName];
      }
    }
  }
};

/**
 * Create a Lighter SDK API client with enhanced features
 * @param options - Exchange options
 * @param exchange - Exchange instance
 * @returns LighterApiClient instance
 */
export const createLighterSDKClient = (options: ExchangeOptions, exchange: BaseExchange): LighterApiClient => {
  const baseUrl = getLighterApiBaseUrl({
    exchangeName: exchange.name,
    testnet: options.testnet,
  });

  const client = new LighterApiClient({
    baseUrl,
    timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
    debug: false,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  // Lighter auth is carried by signed tx payloads or explicit `auth`/`authorization`
  // fields. Legacy X-* headers trigger browser CORS preflights through the REST proxy.
  client.addRequestInterceptor((config: any) => {
    stripUnsupportedLighterRestHeaders(config.headers);
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
