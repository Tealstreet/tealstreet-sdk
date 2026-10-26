import type { AxiosError, AxiosRequestConfig } from 'axios';
import type { IAxiosRetryConfig } from 'axios-retry';
import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, ENDPOINTS, PUBLIC_ENDPOINTS, RECV_WINDOW } from './weex.types';

const WEEX_RATE_LIMIT_FALLBACK_MS = 10_000;
const WEEX_RATE_LIMIT_MAX_DELAY_MS = 60_000;
const WEEX_RATE_LIMIT_MAX_RETRIES = 1;
const WEEX_READ_ONLY_POST_ENDPOINTS = new Set([ENDPOINTS.INCOME]);
const WEEX_RATE_LIMIT_INTERVAL_MS: Record<string, number> = {
  S: 1_000,
  M: 60_000,
  H: 3_600_000,
  D: 86_400_000,
};

const getHeader = (headers: unknown, name: string): unknown => {
  if (!headers || typeof headers !== 'object') return undefined;
  if ('get' in headers && typeof headers.get === 'function') return headers.get(name);

  const record = headers as Record<string, unknown>;
  return record[name] ?? record[name.toLowerCase()] ?? record[name.toUpperCase()];
};

const getHeaderEntries = (headers: unknown): Array<[string, unknown]> => {
  if (!headers || typeof headers !== 'object') return [];

  const serialized = 'toJSON' in headers && typeof headers.toJSON === 'function' ? headers.toJSON() : headers;
  return serialized && typeof serialized === 'object' ? Object.entries(serialized as Record<string, unknown>) : [];
};

const clampWeexRateLimitDelay = (delayMs: number): number =>
  Math.min(Math.max(delayMs, 0), WEEX_RATE_LIMIT_MAX_DELAY_MS);

const parseRetryAfterMs = (value: unknown, now = Date.now()): number => {
  if (typeof value !== 'string' && typeof value !== 'number') return 0;

  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds > 0) return seconds * 1_000;

  const timestamp = Date.parse(String(value));
  return Number.isFinite(timestamp) ? Math.max(timestamp - now, 0) : 0;
};

const parseRateLimitIntervalMs = (headerName: string): number => {
  const match = headerName.match(/-(\d+)([SMHD])$/i);
  if (!match) return 0;

  const interval = Number(match[1]);
  const unitMs = WEEX_RATE_LIMIT_INTERVAL_MS[match[2].toUpperCase()];
  return Number.isFinite(interval) && interval > 0 && unitMs ? interval * unitMs : 0;
};

export const getWeexRateLimitRetryDelayMs = (error: Pick<AxiosError, 'response'>): number => {
  const headers = error.response?.headers;
  const retryAfterMs = parseRetryAfterMs(getHeader(headers, 'retry-after'));
  if (retryAfterMs > 0) return clampWeexRateLimitDelay(retryAfterMs);

  const exhaustedIntervals: number[] = [];
  const observedIntervals: number[] = [];
  for (const [rawName, rawValue] of getHeaderEntries(headers)) {
    const name = rawName.toUpperCase();
    const intervalMs = parseRateLimitIntervalMs(name);
    if (!intervalMs) continue;

    if (/^X-(?:REMAINING-WEIGHT|ORDER-REMAINING)-/.test(name) && Number(rawValue) <= 0) {
      exhaustedIntervals.push(intervalMs);
    } else if (/^X-(?:USED-WEIGHT|ORDER-COUNT)-/.test(name)) {
      observedIntervals.push(intervalMs);
    }
  }

  const intervals = exhaustedIntervals.length > 0 ? exhaustedIntervals : observedIntervals;
  if (intervals.length > 0) return clampWeexRateLimitDelay(Math.max(...intervals));

  return WEEX_RATE_LIMIT_FALLBACK_MS;
};

const isWeexReadOnlyRequest = (config?: AxiosRequestConfig): boolean => {
  const method = config?.method?.toUpperCase();
  if (method === 'GET' || method === 'HEAD') return true;
  if (method !== 'POST') return false;

  const path = config?.url?.split('?')[0];
  return !!path && WEEX_READ_ONLY_POST_ENDPOINTS.has(path);
};

export const shouldRetryWeexRequest = (error: AxiosError): boolean => {
  if (!isWeexReadOnlyRequest(error.config)) return false;
  if (isNetworkError(error)) return true;
  if (error.response?.status !== 429) return false;

  const retryCount = error.config?.['axios-retry']?.retryCount ?? 0;
  return retryCount < WEEX_RATE_LIMIT_MAX_RETRIES;
};

export const WEEX_RETRY_CONFIG: IAxiosRetryConfig = {
  retries: 3,
  retryCondition: shouldRetryWeexRequest,
  retryDelay: (_retryCount, error) => (error.response?.status === 429 ? getWeexRateLimitRetryDelayMs(error) : 0),
  shouldResetTimeout: true,
};

export const serializeWeexParams = (params: unknown): string =>
  qs.stringify(params, {
    arrayFormat: 'repeat',
    sort: (a, b) => a.localeCompare(b),
  });

export const signWeexMessage = (secret: string, message: string): string =>
  createHmac('sha256', secret).update(message).digest('base64');

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  if (!options.passphrase && !options.publicDataOnly) {
    throw new Error('WEEX requires a passphrase');
  }

  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: serializeWeexParams,
      },
      headers: {
        'Content-Type': 'application/json',
      },
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, WEEX_RETRY_CONFIG);

  xhr.interceptors.request.use((config) => {
    const url = config.url || '';
    const isPublicEndpoint = PUBLIC_ENDPOINTS.some((endpoint) => url.startsWith(endpoint));
    if (isPublicEndpoint || options.publicDataOnly) {
      return config;
    }

    const nextConfig = { ...config };

    if (options.remoteSigner || !options.key || !options.secret) {
      return nextConfig;
    }

    const method = config.method?.toUpperCase?.() || 'GET';
    const queryString = config.params ? serializeWeexParams(config.params) : '';
    // A replayed config carries the body axios already serialized: sign those bytes.
    const bodyString = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : '';
    const requestPath = `${url}${queryString ? `?${queryString}` : ''}`;
    const timestamp = `${virtualClock.getCurrentTimeMs()}`;
    const signature = signWeexMessage(options.secret, `${timestamp}${method}${requestPath}${bodyString}`);

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-KEY': options.key,
      'ACCESS-PASSPHRASE': options.passphrase || '',
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-SIGN': signature,
      'Content-Type': 'application/json',
    });

    return { ...nextConfig, headers };
  });

  xhr.interceptors.response.use((response) => {
    const code = response.data?.code;
    if (code !== undefined && `${code}` !== '0' && `${code}` !== '200') {
      const msg = response.data?.msg || response.data?.message || `WEEX API error (code ${code})`;
      const err: any = new Error(msg);
      err.response = response;
      throw err;
    }

    return response;
  });

  return xhr;
};
