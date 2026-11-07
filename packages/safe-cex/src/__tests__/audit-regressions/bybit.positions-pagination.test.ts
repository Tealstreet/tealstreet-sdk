import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';

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

  it('treats Bybit leverage-not-modified as a successful no-op', async () => {
    const store = new DefaultStore();
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
    const refreshedPosition = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      entryPrice: 0,
      notional: 0,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      contracts: 0,
      liquidationPrice: 0,
      positionMode: PositionMode.OneWay,
      marginType: MarginType.Cross,
      info: {},
      lastUpdate: 1,
    };
    store.update({
      markets: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
          info: {},
        },
      ],
      positions: [{ ...refreshedPosition, leverage: 5 }],
    });
    exchange.xhr = {
      post: vi.fn().mockResolvedValue({
        data: { retCode: 110043, retMsg: 'leverage not modified', result: {}, retExtInfo: {} },
      }),
    } as any;
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([refreshedPosition]);

    await expect(exchange.setLeverage('BTCUSDT', 10)).resolves.toBeUndefined();

    expect(exchange.xhr.post).toHaveBeenCalledWith(ENDPOINTS.SET_LEVERAGE, {
      category: 'linear',
      symbol: 'BTCUSDT',
      buyLeverage: '10',
      sellLeverage: '10',
    });
    expect(exchange.fetchPositions).toHaveBeenCalledWith(['BTCUSDT']);
    expect(store.positions[0].leverage).toBe(10);
  });
});
