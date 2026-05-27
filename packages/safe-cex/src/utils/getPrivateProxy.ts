// Helper to get CORS proxy URL for browser runtimes.
// Includes both main thread (window) and WebWorker (self) contexts.
// Pure server-side Node.js should return empty string.

export const getPrivateProxy = (url: string | undefined, requiresCors = false, forceJa = false): string => {
  const isWebWorker = typeof self !== 'undefined' && typeof window === 'undefined';
  const isBrowserMainThread = typeof window !== 'undefined';
  const isBrowserRuntime = isBrowserMainThread || isWebWorker;

  // Server-side: no CORS needed.
  if (!isBrowserRuntime) {
    return '';
  }

  if (!requiresCors) {
    return '';
  }

  // Production browser environment
  if (url?.includes('grvt')) {
    return 'https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy';
  }

  if (url?.match(/binance|okx|okex|bitget/) || forceJa) {
    return 'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy';
  }

  return 'https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy';
};
