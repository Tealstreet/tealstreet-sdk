// Helper to get private exchange proxy URL whenever the account/runtime has
// requested Tealstreet IP routing. Browser runtimes need this for CORS; headless
// clients need it for exchange API-key IP allowlists. Hostnames are centralized
// in transportResolution.ts (whitelisted private pool — stays on AWS).

import { PRIVATE_PROXY_ASIA, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from './transportResolution';

export const getPrivateProxy = (url: string | undefined, requiresCors = false, forceJa = false): string => {
  if (!requiresCors) {
    return '';
  }

  // Production browser environment
  if (url?.includes('grvt')) {
    return PRIVATE_PROXY_GRVT_TINY;
  }

  if (url?.match(/binance|okx|okex|bitget/) || forceJa) {
    return PRIVATE_PROXY_JP;
  }

  return PRIVATE_PROXY_ASIA;
};
