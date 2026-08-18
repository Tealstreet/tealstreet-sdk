import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '@tealstreet/safe-cex/src/exchanges/bitget/bitget.exchange';
import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import type { Market } from '@tealstreet/safe-cex/src/types';

import { admitTwapEngine } from './admit';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
} as Market;

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { BTCUSDT: market },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

describe('admitTwapEngine', () => {
  it('selects confirmed for eligible Bybit USDT-linear and legacy when asked', () => {
    const exchange = new BybitExchange('bybit', { key: 'k', secret: 's' }, store);
    expect(admitTwapEngine({ exchange, market, explicitLegacy: false })).toEqual({
      engine: 'confirmed',
      capability: 'bybit:usdt-linear-perp:twap-market',
    });
    expect(admitTwapEngine({ exchange, market, explicitLegacy: true })).toEqual({
      engine: 'legacy',
      reason: 'explicit-legacy',
    });
  });

  it('derives the key from adapter family and market metadata, not name or symbol spelling', () => {
    const renamed = { ...market, id: 'linear-42', symbol: 'BTC-PERP' } as Market;
    const renamedStore = {
      subscribe: vi.fn(),
      markets: [renamed],
      marketsBySymbol: { [renamed.symbol]: renamed },
      tickersBySymbol: {},
      orders: [],
      positions: [],
    } as never;
    const exchange = new BybitExchange('bybit', { key: 'k', secret: 's' }, renamedStore);
    expect(admitTwapEngine({ exchange, market: renamed, explicitLegacy: false })).toEqual({
      engine: 'confirmed',
      capability: 'bybit:usdt-linear-perp:twap-market',
    });

    const misnamed = new BybitExchange('blofin', { key: 'k', secret: 's' }, renamedStore);
    expect(admitTwapEngine({ exchange: misnamed, market: renamed, explicitLegacy: false })).toMatchObject({
      engine: 'error',
      capability: 'bybit:usdt-linear-perp:twap-market',
    });
  });

  it('errors on listed Bitget without a proven capability instead of falling back', () => {
    const exchange = new BitgetExchange('bitget', { key: 'k', secret: 's', passphrase: 'p' }, store);
    const admission = admitTwapEngine({ exchange, market, explicitLegacy: false });
    expect(admission.engine).toBe('error');
    if (admission.engine === 'error') {
      expect(admission.message).toMatch(/legacy/);
    }
  });

  it('keeps noneligible markets on existing legacy behavior', () => {
    const exchange = new BybitExchange('bybit', { key: 'k', secret: 's' }, store);
    expect(
      admitTwapEngine({
        exchange,
        market: { ...market, isSpot: true } as Market,
        explicitLegacy: false,
      }),
    ).toEqual({ engine: 'legacy', reason: 'not-listed' });
    expect(
      admitTwapEngine({
        exchange,
        market: { ...market, quote: 'USDC' } as Market,
        explicitLegacy: false,
      }),
    ).toEqual({ engine: 'legacy', reason: 'not-listed' });
  });
});
