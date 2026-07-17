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
      vi.spyOn(exchange, 'mapPosition').mockImplementation((position: any) => ({
        ...position,
        id: position.symbol,
      }));

      await expect(exchange.fetchPositionsHelper(undefined, category, settleCoin)).resolves.toEqual([
        { id: 'FIRST', symbol: 'FIRST' },
        { id: 'SECOND', symbol: 'SECOND' },
      ]);
      expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.POSITIONS, {
        params: { category, settleCoin, limit: 200, cursor: '' },
      });
      expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.POSITIONS, {
        params: { category, settleCoin, limit: 200, cursor: 'page-2' },
      });
    }
  });

  it('keeps updated leverage when a later page returns a duplicate placeholder row', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore());
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol: 'ARBUSDT',
                leverage: '16.5',
                avgPrice: '0',
                positionValue: '',
                unrealisedPnl: '',
                curRealisedPnl: '0',
                liqPrice: '',
                positionIM: '',
                size: '0',
                positionIdx: 0,
                seq: 318875949721,
                updatedTime: '1784321341079',
                side: '',
                positionStatus: 'Normal',
              },
            ],
            nextPageCursor: 'ARBUSDT%2C1784321341079%2C0',
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol: 'ARBUSDT',
                leverage: '10',
                avgPrice: '',
                positionValue: '',
                unrealisedPnl: '',
                curRealisedPnl: '',
                liqPrice: '',
                positionIM: '',
                size: '0',
                positionIdx: 0,
                seq: -1,
                updatedTime: '',
                side: '',
                positionStatus: '',
              },
            ],
            nextPageCursor: '',
          },
        },
      });
    exchange.xhr = { get } as any;

    const positions = await exchange.fetchPositionsHelper(['ARBUSDT']);

    expect(positions).toHaveLength(1);
    expect(positions[0]).toMatchObject({
      symbol: 'ARBUSDT',
      leverage: 16.5,
      contracts: 0,
    });
    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.POSITIONS, {
      params: { symbol: 'ARBUSDT', category: 'linear', limit: 200, cursor: '' },
    });
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.POSITIONS, {
      params: {
        symbol: 'ARBUSDT',
        category: 'linear',
        limit: 200,
        cursor: 'ARBUSDT%2C1784321341079%2C0',
      },
    });
  });
});
