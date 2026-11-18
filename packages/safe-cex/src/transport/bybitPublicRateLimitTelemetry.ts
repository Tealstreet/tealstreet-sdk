import type { HttpPacket, HttpResponse } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName } from '../types';

export interface BybitPublicRateLimitNotice {
  retCode?: 10006 | 10018 | 429;
  httpStatus?: 429;
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

type PublicRequest = Pick<HttpPacket, 'method' | 'url' | 'headers'> & { body?: unknown };
const maxBodyBytes = 1024 * 1024;
const responseHeaderNames = new Set([
  'x-tealstreet-egress',
  'x-tealstreet-egress-target',
  'x-bapi-limit',
  'x-bapi-limit-status',
  'x-bapi-limit-reset-timestamp',
  'retry-after',
]);

const isPublicRequest = (request: PublicRequest): boolean => {
  if (request.method !== 'GET' || request.body || request.url.length > 4096) return false;
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
      return false;
    if (Object.keys(request.headers).some((name) => !headerNames.has(name.toLowerCase()))) return false;
    const seen = new Set<string>();
    for (const name of url.searchParams.keys()) {
      if (!queryNames.has(name) || seen.has(name)) return false;
      seen.add(name);
    }
    return true;
  } catch {
    return false;
  }
};

/** Inspect only unsigned public responses; never copy a body, URL or arbitrary header into telemetry. */
export const bybitPublicRateLimitNotice = (
  request: PublicRequest,
  body: string,
  headers: unknown,
  status = 200,
): BybitPublicRateLimitNotice | undefined => {
  if (!isPublicRequest(request)) return;
  try {
    let retCode: BybitPublicRateLimitNotice['retCode'];
    if (status !== 429) {
      if (body.length > maxBodyBytes) return;
      // https://bybit-exchange.github.io/docs/v5/error (10018 is the legacy IP limit).
      const code = JSON.parse(body)?.retCode;
      if (code !== 10006 && code !== 10018 && code !== 429) return;
      retCode = code;
    }
    const normalized: Record<string, string> = Object.fromEntries(
      Object.entries(headers && typeof headers === 'object' ? headers : {})
        .filter((entry): entry is [string, string] => typeof entry[1] === 'string')
        .map(([name, value]) => [name.toLowerCase(), value]),
    );
    const id = (name: string) => (/^[a-z0-9._-]{1,64}$/i.test(normalized[name] ?? '') ? normalized[name] : 'unknown');
    const numeric = (name: string) => (/^\d{1,20}$/.test(normalized[name] ?? '') ? normalized[name] : undefined);
    return {
      retCode,
      httpStatus: status === 429 ? 429 : undefined,
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

/** Observe the host's response without consuming ahead, changing chunks, or delaying dispatch. */
export const observeBybitPublicRateLimit = (
  request: PublicRequest,
  response: HttpResponse,
  report: ((notice: BybitPublicRateLimitNotice) => void) | undefined,
): HttpResponse => {
  if (!report || !isPublicRequest(request)) return response;
  const reportBody = (body: string) => {
    try {
      const headers: Record<string, string> = {};
      const lines = (response.headers?.length ?? 0) <= 64 * 1024 ? (response.headers ?? '') : '';
      for (const line of lines.split('\n')) {
        const colon = line.indexOf(':');
        if (colon < 0) continue;
        const name = line.slice(0, colon).trim().toLowerCase();
        if (!responseHeaderNames.has(name)) continue;
        // Duplicate headers are ambiguous; an empty value is rejected by the notice validator.
        headers[name] = name in headers ? '' : line.slice(colon + 1).trim();
      }
      const notice = bybitPublicRateLimitNotice(request, body, headers, response.status);
      if (notice) report(notice);
    } catch {
      /* Telemetry cannot fail a request or its stream. */
    }
  };
  // Status alone is authoritative; do not consume or wrap a 429 body, or report it twice.
  if (response.status === 429) {
    reportBody('');
    return response;
  }
  if (response.body instanceof Uint8Array) {
    if (response.body.byteLength <= maxBodyBytes) reportBody(new TextDecoder().decode(response.body));
    return response;
  }
  const source = response.body;
  return {
    ...response,
    body: (async function* () {
      const decoder = new TextDecoder();
      let bytes = 0;
      let body = '';
      for await (const chunk of source) {
        bytes += chunk.byteLength;
        body = bytes <= maxBodyBytes ? body + decoder.decode(chunk, { stream: true }) : '';
        yield chunk;
      }
      if (bytes <= maxBodyBytes) reportBody(body + decoder.decode());
    })(),
  };
};

export const bybitPublicRateLimitReport = (exchange: ExchangeName, notice: BybitPublicRateLimitNotice) => {
  const message = 'Bybit public REST rate limit';
  return {
    message,
    sentryData: {
      error: { name: 'BybitPublicRateLimit', message },
      tags: {
        exchange,
        bybit_ret_code: notice.retCode?.toString(),
        bybit_http_status: notice.httpStatus?.toString(),
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
