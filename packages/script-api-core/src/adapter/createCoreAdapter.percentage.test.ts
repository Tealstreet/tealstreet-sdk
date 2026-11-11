import { describe, expect, it, vi } from 'vitest';
import { OrderSide, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import { createCoreAdapter } from './createCoreAdapter';

const fixture = (side: PositionSide, unknown = false) => {
  const placeOrder = vi.fn(async () => ['close']);
  const updateOrder = vi.fn();
  const market = {
    id: 'DOGE_USDT',
    symbol: 'DOGEUSDT',
    contractSize: 100,
    precision: { amount: 100, price: 0.00001 },
    limits: { amount: { min: 100, max: 44500000 } },
  };
  const store = {
    markets: [market],
    positions: [{ symbol: market.symbol, side, contracts: 26100, extra: { contractsNative: unknown ? 260 : 261 } }],
    orders: [{ id: 'close', symbol: market.symbol, type: OrderType.Limit, reduceOnly: true }],
    tickers: [],
  };
  const writes: unknown[] = [];
  const adapter = createCoreAdapter({
    getExchange: () => ({ name: 'mexc', placeOrder, updateOrder, store }),
    getStore: () => store,
    onWrite: (event: unknown) => writes.push(event),
  } as never);
  return {
    placeOrder,
    updateOrder,
    writes,
    run: (opts: { percent?: number; amount?: number }) =>
      adapter.closePosition({ symbol: market.symbol, side, ...opts } as never, 'mexc'),
    amend: () => adapter.updateOrder('close', { update: { amount: 130.5 } }, 'mexc'),
  };
};

describe.each([PositionSide.Long, PositionSide.Short])('CoreAdapter percentage close policy (%s)', (side) => {
  it.each([
    [100, 261],
    [50, 130],
  ])('%i percent sends exactly %i native contracts', async (percent, amount) => {
    const { run, placeOrder } = fixture(side);
    await run({ percent });
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        amount,
        reduceOnly: true,
        side: side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
      }),
    );
  });
  it('0.1% visibly refuses size 0.261 floored to zero at step 1', async () => {
    const { run, placeOrder, writes } = fixture(side);
    await expect(run({ percent: 0.1 })).rejects.toThrow(/0\.261.*floors to 0.*step 1/);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(writes.at(-1)).toMatchObject({ ok: false, error: expect.stringMatching(/0\.261.*step 1/) });
  });
  it('explicit 130.5 native contracts still refuse', async () => {
    const { run, placeOrder } = fixture(side);
    await expect(run({ amount: 130.5 })).rejects.toThrow(/130\.5.*lot step 1/);
    expect(placeOrder).not.toHaveBeenCalled();
  });
  it.each([50, 100])('%i percent visibly refuses unknown native size', async (percent) => {
    const { run, placeOrder } = fixture(side, true);
    await expect(run({ percent })).rejects.toThrow(/unknown/i);
    expect(placeOrder).not.toHaveBeenCalled();
  });
  it('an explicit 130.5 amend still refuses', async () => {
    const { amend, updateOrder } = fixture(side);
    await expect(amend()).rejects.toThrow(/130\.5.*lot step 1/);
    expect(updateOrder).not.toHaveBeenCalled();
  });
});

it('refuses ambiguous explicit amount plus percentage before sending', async () => {
  const { run, placeOrder } = fixture(PositionSide.Long);
  await expect(run({ percent: 50, amount: 130 })).rejects.toThrow(/only one.*amount.*percent/);
  expect(placeOrder).not.toHaveBeenCalled();
});
