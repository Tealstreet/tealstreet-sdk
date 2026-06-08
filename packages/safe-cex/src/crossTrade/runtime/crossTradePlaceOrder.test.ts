import type { Exchange } from '../../exchanges/base';
import type { Market } from '../../types';
import type { CrossTradeCopier } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { createCrossTradePlaceOrder } from './crossTradePlaceOrder';

const makeExchange = (activeChasers: any[]): Exchange => {
  const emit = vi.fn();
  return {
    emitter: { emit } as any,
    placeOrder: vi.fn(),
    store: { activeChasers } as any,
    name: 'bybit' as any,
  } as unknown as Exchange;
};

const makeMarket = (symbol: string): Market =>
  ({
    symbol,
    base: 'BTC',
    quote: 'USDT',
    precision: { price: 0.5, amount: 0.001 },
    limits: { amount: { min: 0.001 }, leverage: { min: 1, max: 100 } },
  }) as unknown as Market;

const makeCopier = (execution: CrossTradeCopier['execution']): CrossTradeCopier =>
  ({
    enabled: true,
    account: 'copier-account',
    weight: 1,
    weightMode: 'Multiplier',
    execution,
    symbolMappings: {},
  }) as unknown as CrossTradeCopier;

describe('crossTradePlaceOrder — Chase mode dedup scope', () => {
  it('does not start a new chaser when the copier already has one with matching size', async () => {
    const copierExchange = makeExchange([
      { id: 'existing', symbol: 'BTCUSDT', side: OrderSide.Buy, size: 0.5, isStopped: false },
    ]);

    const placeOrder = createCrossTradePlaceOrder({
      getExchange: () => copierExchange,
    });

    await placeOrder(makeCopier('Chase'), makeMarket('BTCUSDT'), 65000, 'copier-account', OrderSide.Buy, 0.5, false);

    expect(copierExchange.emitter.emit).not.toHaveBeenCalledWith('chaser:start', expect.anything());
  });

  it('terminates and restarts when the copier has a same-symbol/side chaser at a different size', async () => {
    const copierExchange = makeExchange([
      { id: 'existing', symbol: 'BTCUSDT', side: OrderSide.Buy, size: 0.3, isStopped: false },
    ]);

    const placeOrder = createCrossTradePlaceOrder({
      getExchange: () => copierExchange,
    });

    await placeOrder(makeCopier('Chase'), makeMarket('BTCUSDT'), 65000, 'copier-account', OrderSide.Buy, 0.5, false);

    expect(copierExchange.emitter.emit).toHaveBeenCalledWith('chaser:terminate', 'existing');
    expect(copierExchange.emitter.emit).toHaveBeenCalledWith(
      'chaser:start',
      expect.objectContaining({ symbol: 'BTCUSDT', size: 0.5, side: OrderSide.Buy }),
    );
  });

  it('starts a fresh chaser when the copier has no matching active chaser, regardless of state on other exchanges', async () => {
    // Master / other accounts having matching chasers must NOT suppress the
    // copier's chaser — the dedup scan is scoped to the copier's own
    // exchange. This is the regression test for the sub-agent finding on
    // PR #5984.
    const copierExchange = makeExchange([]); // no active chasers on copier

    const placeOrder = createCrossTradePlaceOrder({
      getExchange: (account) => {
        if (account === 'copier-account') return copierExchange;
        return undefined;
      },
    });

    await placeOrder(makeCopier('Chase'), makeMarket('BTCUSDT'), 65000, 'copier-account', OrderSide.Buy, 0.5, false);

    expect(copierExchange.emitter.emit).toHaveBeenCalledWith(
      'chaser:start',
      expect.objectContaining({ symbol: 'BTCUSDT', size: 0.5, side: OrderSide.Buy }),
    );
  });

  it('emits nothing when amount is 0', async () => {
    const copierExchange = makeExchange([]);

    const placeOrder = createCrossTradePlaceOrder({
      getExchange: () => copierExchange,
    });

    await placeOrder(makeCopier('Chase'), makeMarket('BTCUSDT'), 65000, 'copier-account', OrderSide.Buy, 0, false);

    expect(copierExchange.emitter.emit).not.toHaveBeenCalled();
  });
});
