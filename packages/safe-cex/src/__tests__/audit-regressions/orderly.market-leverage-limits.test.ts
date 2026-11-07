import { describe, expect, it, vi } from 'vitest';

import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: Orderly market leverage limits', () => {
  it('derives each symbol cap from the documented base_imr field', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: {
            rows: [
              {
                symbol: 'PERP_BTC_USDC',
                base_imr: 0.04,
                base_tick: 0.001,
                quote_tick: 0.1,
                base_min: 0.001,
                base_max: 100,
              },
              {
                symbol: 'PERP_NEAR_USDC',
                base_imr: 0.08,
                base_tick: 0.1,
                quote_tick: 0.001,
                base_min: 0.1,
                base_max: 10_000,
              },
            ],
          },
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets.map((market) => [market.symbol, market.limits.leverage])).toEqual([
      ['BTCUSDC', { min: 1, max: 25 }],
      ['NEARUSDC', { min: 1, max: 13 }],
    ]);
  });

  it('retains the legacy field and universal fallback for older or incomplete rows', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: {
            rows: [
              { symbol: 'PERP_ETH_USDC', max_leverage: 50 },
              { symbol: 'PERP_SOL_USDC' },
            ],
          },
        },
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets.map((market) => market.limits.leverage)).toEqual([
      { min: 1, max: 50 },
      { min: 1, max: 100 },
    ]);
  });
});
