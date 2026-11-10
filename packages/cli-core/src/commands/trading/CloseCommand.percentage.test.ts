import { describe, expect, it, vi } from 'vitest';
import { OrderSide, PositionSide } from '@tealstreet/safe-cex/src/types';
import { CloseCommand } from './CloseCommand';

const fixture = (side: PositionSide, unknown = false) => {
  const placeOrder = vi.fn(async () => ['close']);
  const market = {
    id: 'DOGE_USDT',
    symbol: 'DOGEUSDT',
    base: 'DOGE',
    quote: 'USDT',
    contractSize: 100,
    precision: { amount: 100, price: 0.00001 },
    limits: { amount: { min: 100, max: 44500000 } },
  };
  const position = { symbol: market.symbol, side, contracts: 26100, extra: { contractsNative: unknown ? 260 : 261 } };
  const exchange = {
    name: 'mexc',
    placeOrder,
    store: { markets: [market], positions: [position], orders: [], tickers: [{ symbol: market.symbol, last: 0.1 }] },
    emitter: { emit: vi.fn() },
  };
  return {
    placeOrder,
    run: async (size: string) => {
      const result = await new CloseCommand().execute({
        raw: `close ${size}`,
        args: { _: ['close', side, size] },
        symbol: market.symbol,
        accountName: 'mexc',
        exchange,
        adapter: { printSuccess: vi.fn(), printError: vi.fn() },
      } as never);
      if (!result.success) throw new Error(result.error);
    },
  };
};

describe.each([PositionSide.Long, PositionSide.Short])('CLI percentage close policy (%s)', (side) => {
  it.each([
    ['100%', 261],
    ['50%', 130],
  ] as const)('%s sends exactly %i native contracts', async (size, amount) => {
    const { run, placeOrder } = fixture(side);
    await run(size);
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        amount,
        reduceOnly: true,
        side: side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
      }),
    );
  });
  it('0.1% visibly refuses size 0.261 floored to zero at step 1', async () => {
    const { run, placeOrder } = fixture(side);
    await expect(run('0.1%')).rejects.toThrow(/0\.261.*floors to 0.*step 1/);
    expect(placeOrder).not.toHaveBeenCalled();
  });
  it('explicit 130.5 native contracts still refuse', async () => {
    const { run, placeOrder } = fixture(side);
    // At 0.1 USDT/DOGE and 100 DOGE/contract, $1305 is exactly 130.5 native contracts.
    await expect(run('$1305')).rejects.toThrow(/130\.5.*lot step 1/);
    expect(placeOrder).not.toHaveBeenCalled();
  });
  it.each(['50%', '100%'])('%s visibly refuses unknown native size', async (size) => {
    const { run, placeOrder } = fixture(side, true);
    await expect(run(size)).rejects.toThrow(/unknown/i);
    expect(placeOrder).not.toHaveBeenCalled();
  });
});
