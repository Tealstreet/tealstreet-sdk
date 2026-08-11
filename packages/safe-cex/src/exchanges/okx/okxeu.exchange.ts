import type { OkxInstrumentType } from './okx.types';

import { OKXExchange } from './okx.exchange';
import { ENDPOINTS } from './okx.types';

export class OKXEUExchange extends OKXExchange {
  getInstrumentTypes(): OkxInstrumentType[] {
    return ['SWAP', 'FUTURES'];
  }

  isSupportedMarketInstrument(market: Record<string, any>): boolean {
    if (market.instType === 'FUTURES') {
      return market.ruleType === 'xperp' && (market.ctType === 'linear' || market.ctType === 'inverse');
    }

    return super.isSupportedMarketInstrument(market);
  }

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.ACCOUNT_CONFIG);
      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || err?.message };
    }
  };
}
