import type { CrossTradeCopier } from '../../crossTrade/types';
import type { Exchange } from '../../exchanges/base';
import type { Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { createCrossTradePlaceOrder } from '../../crossTrade/runtime/crossTradePlaceOrder';
import { OrderSide } from '../../types';
import { convertFromBaseSizeToNative } from '../../utils/sizing';

const market = {
  symbol: 'DOGEUSDT',
  contractSize: 100,
  precision: { amount: 100, price: 0.00001 },
  limits: { amount: { min: 100, max: 44500000 } },
} as Market;
describe('MEXC copied reductions preserve native quantities', () => {
  it('does not pre-round 130 native contracts using the 100-DOGE base step', () => {
    expect(convertFromBaseSizeToNative(13000, market, 0.1, 'mexc')).toBe(130);
    expect(convertFromBaseSizeToNative(13040, market, 0.1, 'mexc')).toBe(130.4);
  });
  it.each(['Market', 'LimitSlippage', 'Chase'] as const)(
    'refuses off-step or above-limit %s reductions before any side effect',
    async (execution) => {
      const placeOrder = vi.fn();
      const emit = vi.fn();
      const exchange = {
        name: 'mexc',
        placeOrder,
        emitter: { emit },
        store: { activeChasers: [{ id: 'old', symbol: market.symbol, side: OrderSide.Sell, size: 2 }] },
      } as unknown as Exchange;
      const send = createCrossTradePlaceOrder({ getExchange: () => exchange });
      for (const amount of [130.4, 445001]) {
        await expect(
          send({ execution } as CrossTradeCopier, market, 0.1, 'copier', OrderSide.Sell, amount, true),
        ).rejects.toThrow();
      }
      expect(placeOrder).not.toHaveBeenCalled();
      expect(emit).not.toHaveBeenCalledWith('chaser:terminate', expect.anything());
      expect(emit).not.toHaveBeenCalledWith('chaser:start', expect.anything());
      expect(emit).toHaveBeenCalledWith('warning', expect.stringContaining('Cross Trade:'));
    },
  );
  it.each(['Market', 'LimitSlippage'] as const)('preserves the full 130-contract %s close', async (execution) => {
    const placeOrder = vi.fn();
    const exchange = { name: 'mexc', placeOrder, emitter: { emit: vi.fn() } } as unknown as Exchange;
    const send = createCrossTradePlaceOrder({ getExchange: () => exchange });
    await send({ execution } as CrossTradeCopier, market, 0.1, 'copier', OrderSide.Sell, 130, true);
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 130, reduceOnly: true }));
  });
});
