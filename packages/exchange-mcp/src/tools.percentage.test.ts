import { describe, expect, it, vi } from 'vitest';
import { z } from 'zod';
import { OrderSide, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import { tools, supportedTools } from './tools';

const fixture = (side: PositionSide, unknown = false) => {
  const placeOrder = vi.fn(async () => ['close']);
  const updateOrder = vi.fn();
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
  const order = { id: 'close', symbol: market.symbol, type: OrderType.Limit, reduceOnly: true };
  const exchange = {
    name: 'mexc',
    capabilities: { trading: true, positions: true },
    placeOrder,
    updateOrder,
    store: { markets: [market], positions: [position], orders: [order], tickers: [] },
    fetchPositions: vi.fn(async () => [position]),
    fetchOrders: vi.fn(async () => [order]),
    fetchTickers: vi.fn(async () => [{ symbol: market.symbol, last: 0.1 }]),
  } as never;
  const run = async (name: string, args: object) => {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `MCP ${name} route must exist`).toBeDefined();
    return tool!.run(exchange, z.object(tool!.input).parse(args));
  };
  return {
    placeOrder,
    updateOrder,
    exchange,
    run: (opts: { percent?: number; amount?: number }) =>
      run('close_position', { symbol: market.symbol, side, ...opts }),
    amend: () => run('amend_order', { orderId: 'close', amount: 130.5 }),
  };
};

describe.each([PositionSide.Long, PositionSide.Short])('MCP percentage close policy (%s)', (side) => {
  it.each([
    [100, 261],
    [50, 130],
  ])('%i percent sends exactly %i native contracts', async (percent, amount) => {
    const { run, placeOrder, exchange } = fixture(side);
    expect(supportedTools(exchange).map((tool) => tool.name)).toContain('close_position');
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
    const { run, placeOrder } = fixture(side);
    await expect(run({ percent: 0.1 })).rejects.toThrow(/0\.261.*floors to 0.*step 1/);
    expect(placeOrder).not.toHaveBeenCalled();
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

it.each([
  { trading: false, positions: true },
  { trading: true, positions: false },
])(
  'does not advertise close_position without both trading and position reads ($trading/$positions)',
  (capabilities) => {
    const { exchange } = fixture(PositionSide.Long);
    Object.assign(exchange, { capabilities });
    expect(supportedTools(exchange).map((tool) => tool.name)).not.toContain('close_position');
  },
);
