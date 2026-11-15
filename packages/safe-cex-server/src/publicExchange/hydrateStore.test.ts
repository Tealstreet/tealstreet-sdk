import { describe, expect, it } from 'vitest';

import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import type { Market } from '@tealstreet/safe-cex/src/types';

import { hydrateMarketStore } from './hydrateStore';

const market = (overrides: Partial<Market> = {}): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { price: 0.1, amount: 0.001 },
    limits: { amount: { min: 0, max: 0 }, leverage: { min: 1, max: 1 } },
    ...overrides,
  }) satisfies Market;

describe('hydrateMarketStore', () => {
  it('hydrates markets and marks markets loaded on a live safe-cex store', () => {
    const store = new DefaultStore();
    const markets = [market()];

    hydrateMarketStore(store, markets);

    expect(store.markets).toBe(markets);
    expect(store.marketsBySymbol.BTCUSDT).toBe(markets[0]);
    expect(store.loaded.markets).toBe(true);
  });
});
