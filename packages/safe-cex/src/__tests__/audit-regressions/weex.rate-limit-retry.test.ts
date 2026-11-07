import type { AxiosError } from 'axios';

import { describe, expect, it } from 'vitest';

import { getWeexRateLimitRetryDelayMs, shouldRetryWeexRequest, WEEX_RETRY_CONFIG } from '../reference/weex/weex.api';
import { ENDPOINTS } from '../reference/weex/weex.types';

const createError = ({
  method = 'get',
  url = ENDPOINTS.POSITIONS,
  status,
  headers = {},
  retryCount = 0,
  code,
}: {
  method?: string;
  url?: string;
  status?: number;
  headers?: Record<string, string>;
  retryCount?: number;
  code?: string;
}): AxiosError =>
  ({
    code,
    config: {
      method,
      url,
      'axios-retry': { retryCount },
    },
    response: status === undefined ? undefined : { status, headers },
  }) as AxiosError;

describe('audit regression: WEEX rate-limit retries', () => {
  it.each([
    ['get', ENDPOINTS.POSITIONS],
    ['head', ENDPOINTS.EXCHANGE_INFO],
    ['post', ENDPOINTS.INCOME],
  ])('retries one 429 for the read-only %s %s request', (method, url) => {
    expect(shouldRetryWeexRequest(createError({ method, url, status: 429 }))).toBe(true);
    expect(shouldRetryWeexRequest(createError({ method, url, status: 429, retryCount: 1 }))).toBe(false);
  });

  it.each([
    ['post', ENDPOINTS.ORDER],
    ['post', ENDPOINTS.BATCH_ORDERS],
    ['post', ENDPOINTS.ALGO_ORDER],
    ['post', ENDPOINTS.MODIFY_TP_SL_ORDER],
    ['post', ENDPOINTS.SET_LEVERAGE],
    ['delete', ENDPOINTS.BATCH_ORDERS],
    ['delete', ENDPOINTS.CANCEL_ALL_OPEN_ORDERS],
  ])('never retries the mutation %s %s after a 429 or network failure', (method, url) => {
    expect(shouldRetryWeexRequest(createError({ method, url, status: 429 }))).toBe(false);
    expect(shouldRetryWeexRequest(createError({ method, url, code: 'ECONNRESET' }))).toBe(false);
  });

  it('retains bounded network retries for reads only', () => {
    expect(WEEX_RETRY_CONFIG.retries).toBe(3);
    expect(shouldRetryWeexRequest(createError({ code: 'ECONNRESET' }))).toBe(true);
    expect(shouldRetryWeexRequest(createError({ status: 503 }))).toBe(false);
  });

  it('prefers Retry-After and caps an excessive server delay', () => {
    expect(
      getWeexRateLimitRetryDelayMs(
        createError({ status: 429, headers: { 'retry-after': '2', 'x-used-weight-1m': '100' } }),
      ),
    ).toBe(2_000);
    expect(getWeexRateLimitRetryDelayMs(createError({ status: 429, headers: { 'retry-after': '300' } }))).toBe(60_000);
  });

  it('uses exhausted remaining windows before observed counter windows', () => {
    expect(
      getWeexRateLimitRetryDelayMs(
        createError({
          status: 429,
          headers: {
            'x-remaining-weight-10s': '0',
            'x-used-weight-1m': '100',
          },
        }),
      ),
    ).toBe(10_000);
  });

  it('falls back to documented counter windows and the ten-second ban', () => {
    expect(
      getWeexRateLimitRetryDelayMs(
        createError({ status: 429, headers: { 'x-used-weight-10s': '2000', 'x-used-weight-1m': '2000' } }),
      ),
    ).toBe(60_000);
    expect(getWeexRateLimitRetryDelayMs(createError({ status: 429 }))).toBe(10_000);
  });
});
