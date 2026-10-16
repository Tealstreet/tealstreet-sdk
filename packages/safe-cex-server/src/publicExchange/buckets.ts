import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

export const DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE = 'safe-cex';
export const PUBLIC_EXCHANGE_MARKET_CACHE_VERSION = 'v1';
export const UTC_HOUR_MS = 60 * 60 * 1000;

export type UtcHourMarketBucket = {
  bucket: string;
  startMs: number;
  endMs: number;
};

const pad2 = (value: number): string => String(value).padStart(2, '0');

export const getUtcHourMarketBucket = (input: Date | number = Date.now()): UtcHourMarketBucket => {
  const timestamp = input instanceof Date ? input.getTime() : input;
  if (!Number.isFinite(timestamp)) {
    throw new Error('Cannot compute market bucket for a non-finite timestamp');
  }

  const startMs = Math.floor(timestamp / UTC_HOUR_MS) * UTC_HOUR_MS;
  const date = new Date(startMs);
  const bucket = [
    date.getUTCFullYear(),
    pad2(date.getUTCMonth() + 1),
    pad2(date.getUTCDate()),
    pad2(date.getUTCHours()),
  ].join('');

  return {
    bucket,
    startMs,
    endMs: startMs + UTC_HOUR_MS,
  };
};

export const isUtcHourMarketBucket = (bucket: unknown): bucket is string =>
  typeof bucket === 'string' && /^\d{10}$/.test(bucket);

export const getPreviousUtcHourMarketBucket = (input: Date | number = Date.now()): UtcHourMarketBucket =>
  getUtcHourMarketBucket((input instanceof Date ? input.getTime() : input) - UTC_HOUR_MS);

export type MarketCacheKeys = {
  bucketKey: string;
  latestKey: string;
  lockKey: string;
};

export const getMarketCacheKeys = (
  exchange: ExchangeName,
  bucket: string,
  namespace = DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE,
): MarketCacheKeys => {
  if (!isUtcHourMarketBucket(bucket)) {
    throw new Error(`Invalid UTC-hour market bucket: ${String(bucket)}`);
  }

  const prefix = `${namespace}:markets:${PUBLIC_EXCHANGE_MARKET_CACHE_VERSION}:${exchange}`;
  return {
    bucketKey: `${prefix}:${bucket}`,
    latestKey: `${prefix}:latest`,
    lockKey: `${prefix}:lock:${bucket}`,
  };
};
