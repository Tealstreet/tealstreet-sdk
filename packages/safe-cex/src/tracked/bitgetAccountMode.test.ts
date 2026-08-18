import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import type { Market } from '../types';

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { [market.symbol]: market },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

class ProbeableBitgetExchange extends BitgetExchange {
  async applyAccountModeProbe(data: unknown): Promise<void> {
    vi.spyOn(this.xhr, 'get').mockResolvedValue({ data });
    await this._detectAccountType();
  }
}

describe('Bitget confirmed account-mode admission', () => {
  it('recognizes a 200 response body with code 40084 as proven Classic V2', async () => {
    const exchange = new ProbeableBitgetExchange(
      'bitget',
      { key: 'key', secret: 'secret', passphrase: 'passphrase' },
      store,
    );

    await exchange.applyAccountModeProbe({ code: '40084', msg: 'Classic Account mode' });

    expect(exchange.isClassicV2Account).toBe(true);
  });

  it('does not call an unrecognized response code Classic', async () => {
    const exchange = new ProbeableBitgetExchange(
      'bitget',
      { key: 'key', secret: 'secret', passphrase: 'passphrase' },
      store,
    );

    await exchange.applyAccountModeProbe({ code: '50000', msg: 'Unknown' });

    expect(exchange.isClassicV2Account).toBe(false);
  });
});
