import type { Market } from '@tealstreet/safe-cex/src/types';

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import { prepareOrder } from './prepareOrder';

const perp = {
  symbol: 'BTCUSDT',
  precision: { price: 0.1, amount: 0.001 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
} as unknown as Market;

const option = {
  symbol: 'BTC-25DEC26-150000-C-USDT',
  precision: { price: 5, amount: 0.01 },
  limits: { amount: { min: 0.01, max: 1000 }, leverage: { min: 1, max: 1 }, price: { min: 10 } },
  option: {
    underlying: 'BTC',
    expiry: 1798185600000,
    strike: 150000,
    right: 'call',
    style: 'european',
    settlement: 'cash',
    premiumAsset: 'USDT',
    settleAsset: 'USDT',
  },
} as unknown as Market;

const limit = (symbol: string, extra: object) => ({
  symbol,
  side: OrderSide.Buy,
  type: OrderType.Limit,
  amount: 0.01,
  ...extra,
});

describe('prepareOrder on options', () => {
  it('refuses a limit price that rounds below the venue minimum', () => {
    expect(() => prepareOrder(limit(option.symbol, { price: 5 }), { market: option, exchangeName: 'bybit' })).toThrow(
      'below the minimum 10',
    );
  });

  it('passes an implied volatility through on an option market', () => {
    const out = prepareOrder(limit(option.symbol, { price: 10, impliedVolatility: 0.45 }), {
      market: option,
      exchangeName: 'bybit',
    });
    expect(out.impliedVolatility).toBe(0.45);
  });

  it('refuses an implied volatility on a market that is not an option', () => {
    expect(() =>
      prepareOrder(limit(perp.symbol, { price: 80000, impliedVolatility: 0.45 }), {
        market: perp,
        exchangeName: 'bybit',
      }),
    ).toThrow('impliedVolatility applies only to option markets');
  });
});
