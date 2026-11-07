import { describe, expect, it } from 'vitest';

import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions } from '../../types';

const mapOrder = (overrides: Record<string, unknown>) => {
  const exchange = new BackpackExchange(
    'backpack',
    { key: 'public-key', secret: 'secret', testnet: false } as ExchangeOptions,
    new DefaultStore(),
  );
  return (exchange as any).mapOrder({
    id: 'order-1',
    symbol: 'BTC_USDC_PERP',
    side: 'Ask',
    orderType: 'Limit',
    price: '100',
    quantity: '1',
    timeInForce: 'GTC',
    selfTradePrevention: 'RejectTaker',
    status: 'New',
    createdAt: 1,
    ...overrides,
  });
};

describe('Backpack reduce-only mapping audit regression', () => {
  it('preserves reduceOnly on ordinary REST limit orders', () => {
    expect(mapOrder({ reduceOnly: true }).reduceOnly).toBe(true);
    expect(mapOrder({ reduceOnly: false }).reduceOnly).toBe(false);
  });

  it('uses the trigger heuristic only when the response omits reduceOnly', () => {
    expect(mapOrder({ orderType: 'Market', triggerPrice: '90', triggerQuantity: '1' }).reduceOnly).toBe(true);
    expect(mapOrder({ orderType: 'Market', triggerPrice: '90', triggerQuantity: '1', reduceOnly: false }).reduceOnly).toBe(
      false,
    );
  });
});
