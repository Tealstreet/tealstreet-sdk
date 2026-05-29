// Helper to get private exchange proxy URL whenever the account/runtime has
// requested Tealstreet IP routing. Browser runtimes need this for CORS; headless
// clients need it for exchange API-key IP allowlists.

export const getPrivateProxy = (url: string | undefined, requiresCors = false, forceJa = false): string => {
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
