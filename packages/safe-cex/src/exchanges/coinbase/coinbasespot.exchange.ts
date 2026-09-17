import { CoinbaseExchange } from './coinbase.exchange';
import type { LIST_PORTFOLIOS_RESPONSE } from './coinbase.types';
import { ENDPOINTS } from './coinbase.types';

export class CoinbaseSpotExchange extends CoinbaseExchange {
  supportSpot = true;

  getSupportedTypes = async (): Promise<('spot' | 'cfm' | 'intx')[]> => {
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    return ['spot'];
  };

  findSelectedPortfolio = async (): Promise<LIST_PORTFOLIOS_RESPONSE['portfolios'][0]> => {
    const portfolios = await this.fetchPortfolios();
    for (const portfolio of portfolios) {
      if (portfolio.type === 'INTX') continue;

      try {
        await this.xhr.get(ENDPOINTS.PORTFOLIO_BREAKDOWN, {
          params: {
            portfolio_uuid: portfolio.uuid,
          },
        });
        return portfolio;
      } catch {
        // Keep probing; spot accounts must not fall through to CFM detection.
      }
    }

    this.emitAndLogError('No Coinbase spot portfolio with permissions found');
    throw new Error('No Coinbase spot portfolio with permissions found');
  };
}
