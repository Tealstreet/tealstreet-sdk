import { CoinbaseExchange } from './coinbase.exchange';
import type { LIST_PORTFOLIOS_RESPONSE } from './coinbase.types';
import { ENDPOINTS } from './coinbase.types';

const shouldRethrowPortfolioProbeError = (err: any) => {
  const status = err?.response?.status;
  return status === 429 || status >= 500 || status === undefined;
};

export class CoinbaseSpotExchange extends CoinbaseExchange {
  supportSpot = true;

  getSupportedTypes = async (): Promise<('spot' | 'cfm' | 'intx')[]> => {
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    return ['spot'];
  };

  findSelectedPortfolio = async (): Promise<LIST_PORTFOLIOS_RESPONSE['portfolios'][0]> => {
    let portfolios: LIST_PORTFOLIOS_RESPONSE['portfolios'];
    try {
      const { data } = await this.xhr.get<LIST_PORTFOLIOS_RESPONSE>(ENDPOINTS.LIST_PORTFOLIOS);
      portfolios = data.portfolios;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }

    for (const portfolio of portfolios) {
      if (portfolio.type === 'INTX') continue;

      try {
        await this.xhr.get(ENDPOINTS.PORTFOLIO_BREAKDOWN, {
          params: {
            portfolio_uuid: portfolio.uuid,
          },
        });
        return portfolio;
      } catch (err: any) {
        if (shouldRethrowPortfolioProbeError(err)) {
          this.emitAndLogError(err, err?.response?.data?.message || err?.message);
          throw err;
        }
        // Keep probing; spot accounts must not fall through to CFM detection.
      }
    }

    this.emitAndLogError('No Coinbase spot portfolio with permissions found');
    throw new Error('No Coinbase spot portfolio with permissions found');
  };
}
