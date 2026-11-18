import { describe, expect, it } from 'vitest';

import { BYBIT_BRAZIL_SITE_ID } from '../exchanges/bybit/bybit.site';
import { FrameStore } from '../store/frameStore';
import { nativeOrder } from './cutover/nativeOrder';
import { BybitExchange } from './reference/bybit/bybit.exchange';

/**
 * Brazil accounts write order rows locally after a REST write. An inverse row holds `remaining` in the coin and
 * the USD contracts in `extra.remainingNative`, as `bybitinverse` does; an amend that wrote USD into `remaining`
 * made the chart draw contracts x price ("2,600,192 USD" for a $1,000 ETHUSD leg).
 */
const create = () => {
  const store = new FrameStore();
  const exchange = new BybitExchange(
    'bybit',
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
      bybitSiteId: BYBIT_BRAZIL_SITE_ID,
      enableInverseMarkets: true,
      tuning: { consoleLogging: false },
    } as any,
    store,
  );
  return { exchange: exchange as any, store };
};

const inverseOrder = nativeOrder({
  id: 'oid-1',
  symbol: 'ETHUSD',
  price: 2600,
  amount: 1000,
  filled: 400,
  remaining: 600 / 2600,
  extra: { remainingNative: 600 },
});

describe('Bybit Brazil local order rows for inverse amends', () => {
  it('keeps remaining in the coin and the USD contracts native after a size amend', () => {
    const { exchange, store } = create();
    exchange.applyLocalOrderUpdateAfterRestWrite(inverseOrder, { amount: 1500 });

    const updated = store.orders[0];
    expect(updated.extra?.remainingNative).toBe(1100);
    expect(updated.remaining).toBeCloseTo(1100 / 2600, 8);
  });

  it('re-prices remaining in the coin after a price-only amend', () => {
    const { exchange, store } = create();
    exchange.applyLocalOrderUpdateAfterRestWrite(inverseOrder, { price: 2400 });

    const updated = store.orders[0];
    expect(updated.extra?.remainingNative).toBe(600);
    expect(updated.remaining).toBeCloseTo(600 / 2400, 8);
  });

  it('leaves a linear amend as it was: remaining from the new size, no extra added', () => {
    const { exchange, store } = create();
    const linear = nativeOrder({ id: 'oid-2', symbol: 'ETHUSDT', price: 2600, amount: 1, filled: 0.4, remaining: 0.6 });

    exchange.applyLocalOrderUpdateAfterRestWrite(linear, { amount: 1.5 });
    const sized = store.orders[0];
    exchange.applyLocalOrderUpdateAfterRestWrite(linear, { price: 2400 });
    const repriced = store.orders[0];

    expect(sized.remaining).toBeCloseTo(1.1, 8);
    expect(sized.extra).toBeUndefined();
    expect(repriced.remaining).toBe(0.6);
    expect(repriced.extra).toBeUndefined();
  });
});
