import { describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

describe('Coinbase account identity audit regression', () => {
  it('uses the selected permitted portfolio UUID instead of a Binance-shaped balance response', async () => {
    const exchange = new CoinbaseExchange(
      'coinbase' as any,
      { key: 'test', secret: 'test', tealstreetTunnel: false } as any,
      new DefaultStore(),
    );
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: { portfolios: [{ name: 'INTX', uuid: 'portfolio-uuid', type: 'INTX', deleted: false }] },
      })
      .mockResolvedValueOnce({ data: { breakdown: {} } });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'portfolio-uuid' });
    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.LIST_PORTFOLIOS);
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.PORTFOLIO_BREAKDOWN, {
      params: { portfolio_uuid: 'portfolio-uuid' },
    });
    expect(exchange.selectedPortfolio?.uuid).toBe('portfolio-uuid');
  });
});
