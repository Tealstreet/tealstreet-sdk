import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { describe, expect, it, vi } from 'vitest';
import { supportedTools, tools } from './tools';

describe('MCP MEXC native amend amounts', () => {
  const fixture = (type = 'limit', name = 'mexc') => {
    const order = {
      id: 'close',
      symbol: 'DOGEUSDT',
      type,
      reduceOnly: type === 'limit',
      amount: 13000,
      remaining: 13000,
      filled: 0,
    };
    const market = {
      id: 'DOGE_USDT',
      symbol: order.symbol,
      contractSize: 100,
      precision: { amount: name === 'mexc' ? 100 : 1, price: 0.00001 },
      limits: { amount: { min: 100, max: 44500000 } },
    };
    const updateOrder = vi.fn(async () => ['close']);
    const exchange = {
      name,
      capabilities: {},
      store: { markets: [market] },
      fetchOrders: async () => [order],
      updateOrder,
    } as unknown as Exchange;
    const amend = tools.find((tool) => tool.name === 'amend_order')!;
    return {
      exchange,
      updateOrder,
      run: (amount: number) => amend.run(exchange, { orderId: 'close', amount } as never),
    };
  };

  it('discovers the actual amend tool and preserves the reviewers 130-contract repro', async () => {
    const { exchange, updateOrder, run } = fixture();
    expect(supportedTools(exchange).map((tool) => tool.name)).toContain('amend_order');
    await run(130);
    expect(updateOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ update: { amount: 130 } }));
  });

  it.each(['limit', 'stop_market', 'take_profit_market'])(
    'refuses off-step and above-limit %s amends before I/O',
    async (type) => {
      const { updateOrder, run } = fixture(type);
      await expect(run(130.4)).rejects.toThrow(/lot step/);
      await expect(run(445001)).rejects.toThrow(/maximum/);
      expect(updateOrder).not.toHaveBeenCalled();
    },
  );

  it('retains other venues nearest rounding', async () => {
    const { updateOrder, run } = fixture('limit', 'binance');
    await run(130.4);
    expect(updateOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ update: { amount: 130 } }));
  });
});
