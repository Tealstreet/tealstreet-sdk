import type { ExchangeName } from '../types';

export interface BybitPublicRateLimitNotice {
  egress: string;
  target: string;
  limit?: string;
  remaining?: string;
  resetTimestamp?: string;
  retryAfter?: string;
}

const publicPaths = new Set(
  [
    'time',
    'kline',
    'instruments-info',
    'tickers',
    'orderbook',
    'recent-trade',
    'open-interest',
    'funding/history',
    'mark-price-kline',
    'index-price-kline',
    'premium-index-price-kline',
  ].map((path) => `/v5/market/${path}`),
);
const queryNames = new Set(['category', 'symbol', 'interval', 'start', 'end', 'limit', 'baseCoin', 'status', 'cursor']);
const headerNames = new Set([
  'accept',
  'accept-encoding',
  'content-type',
  'x-referer',
  'referer',
  'origin',
  'user-agent',
  'sec-fetch-mode',
  'sec-fetch-site',
  'sec-fetch-dest',
]);

/** Inspect only unsigned public responses; never copy a body, URL or arbitrary header into telemetry. */
export const bybitPublicRateLimitNotice = (
  request: { method: string; url: string; headers: Record<string, string>; body?: unknown },
  body: string,
  headers: unknown,
): BybitPublicRateLimitNotice | undefined => {
  if (request.method !== 'GET' || request.body || body.length > 1024 * 1024 || request.url.length > 4096) return;
  try {
    const url = new URL(request.url);
    if (
      url.protocol !== 'https:' ||
      url.hostname !== 'api.bybit.com' ||
      (url.port && url.port !== '443') ||
      url.username ||
      url.password ||
      url.hash ||
      !publicPaths.has(url.pathname)
    )
      return;
    if (Object.keys(request.headers).some((name) => !headerNames.has(name.toLowerCase()))) return;
    const seen = new Set<string>();
    for (const name of url.searchParams.keys()) {
      if (!queryNames.has(name) || seen.has(name)) return;
      seen.add(name);
    }
    if (JSON.parse(body)?.retCode !== 10006) return;
    const normalized: Record<string, string> = Object.fromEntries(
      Object.entries(headers && typeof headers === 'object' ? headers : {})
        .filter((entry): entry is [string, string] => typeof entry[1] === 'string')
        .map(([name, value]) => [name.toLowerCase(), value]),
    );
    const id = (name: string) => (/^[a-z0-9._-]{1,64}$/i.test(normalized[name] ?? '') ? normalized[name] : 'unknown');
    const numeric = (name: string) => (/^\d{1,20}$/.test(normalized[name] ?? '') ? normalized[name] : undefined);
    return {
      egress: id('x-tealstreet-egress'),
      target: id('x-tealstreet-egress-target'),
      limit: numeric('x-bapi-limit'),
      remaining: numeric('x-bapi-limit-status'),
      resetTimestamp: numeric('x-bapi-limit-reset-timestamp'),
      retryAfter: numeric('retry-after'),
    };
  } catch {
    return;
  }
};

export const bybitPublicRateLimitReport = (exchange: ExchangeName, notice: BybitPublicRateLimitNotice) => {
  const message = 'Bybit public REST rate limit (10006)';
  return {
    message,
    sentryData: {
      error: { name: 'BybitPublicRateLimit', message },
      tags: {
        exchange,
        bybit_ret_code: '10006',
        bybit_egress: notice.egress,
        bybit_egress_target: notice.target,
        bybit_limit: notice.limit,
        bybit_limit_remaining: notice.remaining,
        bybit_limit_reset_timestamp: notice.resetTimestamp,
        bybit_retry_after: notice.retryAfter,
      },
    },
  };
};
