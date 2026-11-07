import type { Market } from '../../../types';
import type { AccountMarketOverlay } from '../../../exchanges/base';
import type { OkxInstrumentType } from '../../../exchanges/okx/okx.types';

import { OKXExchange } from './okx.exchange';
import { ENDPOINTS } from '../../../exchanges/okx/okx.types';

export class OKXEUExchange extends OKXExchange {
  getInstrumentTypes(): OkxInstrumentType[] {
    return ['SWAP', 'FUTURES'];
  }

  protected getMarketInstrumentsEndpoint(): string {
    return this.isPublicDataOnly ? ENDPOINTS.MARKETS : ENDPOINTS.ACCOUNT_INSTRUMENTS;
  }

  // ACCOUNT_INSTRUMENTS lists only what this EEA account may trade; the shared
  // keyless exchange can only list the public set, so narrow it to this account's.
  protected async fetchAccountMarketOverlay(): Promise<AccountMarketOverlay | null> {
    const accountMarkets = await this.fetchMarkets();
    if (accountMarkets.length === 0) return null;
    const byId = new Map(accountMarkets.map((m) => [m.id, m]));
    return (markets: Market[]) =>
      markets.flatMap((m) => {
        const own = byId.get(m.id);
        return own ? [{ ...m, limits: own.limits }] : [];
      });
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
