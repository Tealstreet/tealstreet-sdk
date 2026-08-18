import type { NewsGatewayFetchOptions } from '@tealstreet/mcp-data';

type JsonRecord = Record<string, unknown>;

const DEFAULT_NEWS_GATEWAY_ORIGIN = 'https://news.tealstreet.io';
const NEWS_GATEWAY_FETCH_TIMEOUT_MS = 5_000;

const newsGatewayOrigin = (): string =>
  (
    process.env.TEALSTREET_NEWS_GATEWAY_ORIGIN ||
    process.env.NEXT_PUBLIC_NEWS_GATEWAY_ORIGIN ||
    DEFAULT_NEWS_GATEWAY_ORIGIN
  ).replace(/\/+$/, '');

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const withTimeoutSignal = async (run: (signal: AbortSignal) => Promise<Response>) => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), NEWS_GATEWAY_FETCH_TIMEOUT_MS);
  try {
    return await run(controller.signal);
  } finally {
    clearTimeout(timer);
  }
};

export const newsGatewayJson = async (path: string, options: NewsGatewayFetchOptions = {}): Promise<JsonRecord> => {
  const headers: HeadersInit = {};
  if (options.authorizationToken) headers.authorization = `Bearer ${options.authorizationToken}`;
  const response = await withTimeoutSignal((signal) =>
    fetch(`${newsGatewayOrigin()}${path}`, {
      cache: 'no-store',
      headers,
      signal,
    }),
  );
  const body = await response.json().catch(() => null);
  if (!isRecord(body)) throw new Error('news-gateway returned a non-object response');
  if (!response.ok && typeof body.error !== 'string') {
    throw new Error(`news-gateway returned HTTP ${response.status}`);
  }
  return body;
};
