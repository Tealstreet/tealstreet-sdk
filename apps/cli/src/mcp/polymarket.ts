import type { PolymarketFetchResult } from '@tealstreet/mcp-data';

const GAMMA_API = 'https://gamma-api.polymarket.com';
const CLOB_API = 'https://clob.polymarket.com';
// Positions live on a THIRD base, beside Gamma and the CLOB.
const DATA_API = 'https://data-api.polymarket.com';
const POLYMARKET_FETCH_TIMEOUT_MS = 10_000;

const configuredOrigin = process.env.TEALSTREET_POLYMARKET_API_ORIGIN ?? process.env.NEXT_PUBLIC_POLY_API_ORIGIN;
const POLYMARKET_PROXY_ORIGIN = configuredOrigin?.replace(/\/+$/, '');
const POLYMARKET_WORKER_ORIGIN_HEADER = process.env.TEALSTREET_POLYMARKET_ORIGIN_HEADER ?? 'https://v3.tealstreet.io';

type FetchInitForPath = { url: string; init?: RequestInit };

const withAttemptSignal = async (run: (signal: AbortSignal) => Promise<Response>) => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), POLYMARKET_FETCH_TIMEOUT_MS);
  try {
    return await run(controller.signal);
  } finally {
    clearTimeout(timer);
  }
};

const directFetchForPath = (path: string): FetchInitForPath => {
  const url = new URL(path, 'https://mcp.local');
  const search = url.searchParams.toString();
  if (url.pathname === '/v1/events') return { url: `${GAMMA_API}/events?${search}` };
  if (url.pathname === '/v1/search') return { url: `${GAMMA_API}/public-search?${search}` };
  if (url.pathname === '/v1/book') return { url: `${CLOB_API}/book?${search}` };
  if (url.pathname === '/v1/positions') return { url: `${DATA_API}/positions?${search}` };
  if (url.pathname === '/v1/price-history') return { url: `${CLOB_API}/prices-history?${search}` };
  if (url.pathname === '/v1/midpoints') {
    const tokenIds = (url.searchParams.get('token_ids') ?? '')
      .split(',')
      .map((tokenId) => tokenId.trim())
      .filter(Boolean);
    return {
      url: `${CLOB_API}/midpoints`,
      init: {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(tokenIds.map((tokenId) => ({ token_id: tokenId }))),
      },
    };
  }
  throw new Error('unknown Polymarket public route');
};

const requestForPath = (path: string): FetchInitForPath => {
  if (!POLYMARKET_PROXY_ORIGIN) return directFetchForPath(path);
  return {
    url: `${POLYMARKET_PROXY_ORIGIN}${path}`,
    init: { headers: { Origin: POLYMARKET_WORKER_ORIGIN_HEADER } },
  };
};

export const polymarketJson = async (path: string): Promise<PolymarketFetchResult> => {
  const request = requestForPath(path);
  const res = await withAttemptSignal((signal) =>
    fetch(request.url, {
      ...request.init,
      cache: 'no-store',
      signal,
    }),
  );
  if (!res.ok) throw new Error(`${request.url}: HTTP ${res.status}`);
  return {
    body: await res.json(),
    receivedAt: new Date().toISOString(),
    cacheControl: res.headers.get('cache-control') ?? undefined,
    cacheStatus: res.headers.get('x-poly-cache') ?? undefined,
  };
};
