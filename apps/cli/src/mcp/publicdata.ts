type JsonRecord = Record<string, unknown>;

const ATHENA_REST = 'https://publicdata-ja.tealstreet.io/publicdata';
const HETZNER_REST = 'https://publicdata-sg.asia.tealstreet.io/publicdata';

const OVERRIDE_REST = process.env.TEALSTREET_PUBLICDATA_REST_URL ?? process.env.NEXT_PUBLIC_PUBLICDATA_REST_URL;

export const PUBLICDATA_REST_URLS: readonly string[] = OVERRIDE_REST
  ? [OVERRIDE_REST, HETZNER_REST]
  : [ATHENA_REST, HETZNER_REST];

const PUBLICDATA_FETCH_TIMEOUT_MS = 5_000;

const withAttemptSignal = async (run: (signal: AbortSignal) => Promise<Response>) => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), PUBLICDATA_FETCH_TIMEOUT_MS);
  try {
    return await run(controller.signal);
  } finally {
    clearTimeout(timer);
  }
};

export async function publicdataFetch(path: string): Promise<Response> {
  let lastErr: unknown = null;
  for (const base of PUBLICDATA_REST_URLS) {
    try {
      const res = await withAttemptSignal((signal) => fetch(`${base}${path}`, { cache: 'no-store', signal }));
      if (res.ok) return res;
      lastErr = new Error(`${base}${path}: HTTP ${res.status}`);
    } catch (err) {
      lastErr = err;
    }
  }
  throw lastErr ?? new Error('publicdataFetch: all endpoints failed');
}

export const publicdataJson = async (path: string): Promise<JsonRecord> => {
  const response = await publicdataFetch(path);
  const body = await response.json();
  if (typeof body !== 'object' || body === null || Array.isArray(body)) {
    throw new Error('publicdata returned a non-object response');
  }
  return body as JsonRecord;
};
