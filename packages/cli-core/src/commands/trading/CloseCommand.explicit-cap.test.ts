import { describe, expect, it, vi } from 'vitest';
import { OrderSide, PositionSide } from '@tealstreet/safe-cex/src/types';
import { CloseCommand } from './CloseCommand';

const fixture = (side: PositionSide, global: boolean) => {
  const placeOrder = vi.fn(async () => ['close']);
  const printError = vi.fn();
  const market = {
    id: 'DOGE_USDT',
    symbol: 'DOGEUSDT',
    base: 'DOGE',
    quote: 'USDT',
    contractSize: 100,
    precision: { amount: 100, price: 0.00001 },
    limits: { amount: { min: 100, max: 44500000 } },
  };
  const exchange = {
    name: 'mexc',
    placeOrder,
    store: {
      markets: [market],
      positions: [{ symbol: market.symbol, side, contracts: 13000, extra: { contractsNative: 130 } }],
      orders: [],
      tickers: [{ symbol: market.symbol, last: 0.1 }],
    },
    emitter: { emit: vi.fn() },
  };
  return {
    placeOrder,
    run: async (size: string) => {
      const selector = global ? `${side}s` : side;
      const result = await new CloseCommand()
        .execute({
          raw: `close ${selector} ${size}`,
          args: { _: ['close', selector, size] },
          symbol: market.symbol,
          accountName: 'mexc',
          exchange,
          adapter: { printSuccess: vi.fn(), printError },
        } as never)
        .catch((error: unknown) => ({ success: false, error: error instanceof Error ? error.message : String(error) }));
      return { result, visibleError: [result.error, ...printError.mock.calls.flat()].filter(Boolean).join('\n') };
    },
  };
};

describe.each([PositionSide.Long, PositionSide.Short])('MEXC explicit close before capping (%s)', (side) => {
  describe.each([false, true])('global: %s', (global) => {
    it('refuses the requested 130.5 before the 130-contract position cap can hide it', async () => {
      const { run, placeOrder } = fixture(side, global);
      const { visibleError } = await run('$1305');
      expect(visibleError).toMatch(/130\.5.*lot step 1/);
      expect(placeOrder).not.toHaveBeenCalled();
    });

    it.each([
      ['$1290', 129],
      ['$1300', 130],
      ['$2000', 130],
      ['200%', 130],
    ] as const)('%s retains the valid capped amount %i', async (size, amount) => {
      const { run, placeOrder } = fixture(side, global);
      const { result, visibleError } = await run(size);
      expect(result.success).toBe(true);
      expect(visibleError).toBe('');
      expect(placeOrder).toHaveBeenCalledExactlyOnceWith(
        expect.objectContaining({
          symbol: 'DOGEUSDT',
          amount,
          reduceOnly: true,
          side: side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
        }),
      );
    });
  });
});
