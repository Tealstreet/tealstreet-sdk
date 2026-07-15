import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { BybitInverseExchange } from '../../exchanges/bybit/bybitinverse.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { DefaultStore } from '../../store/store.base';

describe('Bybit position pagination', () => {
  it('follows nextPageCursor for linear and inverse account snapshots', async () => {
    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore()),
        category: 'linear',
        settleCoin: 'USDT',
      },
      {
        exchange: new BybitInverseExchange(
          'bybitinverse',
          { key: 'key', secret: 'secret' },
          new DefaultStore(),
        ),
        category: 'inverse',
        settleCoin: undefined,
      },
    ];

    for (const { exchange, category, settleCoin } of variants) {
      const get = vi
        .fn()
        .mockResolvedValueOnce({
          data: { retMsg: 'OK', result: { list: [{ symbol: 'FIRST' }], nextPageCursor: 'page-2' } },
        })
        .mockResolvedValueOnce({
          data: { retMsg: 'OK', result: { list: [{ symbol: 'SECOND' }], nextPageCursor: '' } },
        });
      exchange.xhr = { get } as any;
      vi.spyOn(exchange, 'mapPosition').mockImplementation((position: any) => position);

      await expect(exchange.fetchPositionsHelper(undefined, category, settleCoin)).resolves.toEqual([
        { symbol: 'FIRST' },
        { symbol: 'SECOND' },
      ]);
      expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.POSITIONS, {
        params: { category, settleCoin, limit: 200, cursor: '' },
      });
      expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.POSITIONS, {
        params: { category, settleCoin, limit: 200, cursor: 'page-2' },
      });
    }
  });
});
