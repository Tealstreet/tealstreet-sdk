type JsonRecord = Record<string, unknown>;

const DEFAULT_EXCHANGE_PROXY_ORIGIN = 'https://data.tealstreet.io/candles';
const EXCHANGE_PROXY_FETCH_TIMEOUT_MS = 5_000;

const exchangeProxyOrigin = (): string =>
  (
    process.env.TEALSTREET_EXCHANGE_PROXY_ORIGIN ||
    process.env.TEALSTREET_CANDLE_SERVER_URL ||
    process.env.NEXT_PUBLIC_CANDLE_SERVER_URL ||
    DEFAULT_EXCHANGE_PROXY_ORIGIN
  ).replace(/\/+$/, '');

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const withTimeoutSignal = async (run: (signal: AbortSignal) => Promise<Response>) => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), EXCHANGE_PROXY_FETCH_TIMEOUT_MS);
  try {
    return await run(controller.signal);
  } finally {
    clearTimeout(timer);
  }
};

export const exchangeProxyJson = async (path: string): Promise<JsonRecord> => {
  const response = await withTimeoutSignal((signal) =>
    fetch(`${exchangeProxyOrigin()}${path}`, {
      cache: 'no-store',
      signal,
    }),
  );
  const body = await response.json();
  if (!isRecord(body)) throw new Error('exchange proxy returned a non-object response');
  if (!response.ok && typeof body.error !== 'string') {
    throw new Error(`exchange proxy returned HTTP ${response.status}`);
  }
  return body;
};
