import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '@tealstreet/safe-cex/src/exchanges/registry';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';

import { supportedTools, tools } from './tools';

const live: any[] = [];
afterEach(() => live.splice(0).forEach((e) => e.dispose?.()));

const construct = async (name: 'bybit' | 'binancepm') => {
  const Exchange = await exchangeRegistry[name]();
  const exchange = new Exchange(name, { key: 'k', secret: 's', publicDataOnly: true }, new DefaultStore());
  live.push(exchange);
  return exchange as unknown as Exchange;
};

const run = (name: string, exchange: Partial<Exchange>, args: object) =>
  tools.find((t) => t.name === name)!.run(exchange as Exchange, args as never);

const order = (id: string, symbol: string) => ({ id, symbol }) as any;

describe('supportedTools', () => {
  it('leaves out tools whose adapter method is the base fallback', async () => {
    const names = supportedTools(await construct('binancepm')).map((t) => t.name);
    expect(names).not.toContain('get_closed_pnl');
    expect(names).toContain('get_positions');
  });

  it('offers the history tools where the adapter implements them', async () => {
    const names = supportedTools(await construct('bybit')).map((t) => t.name);
    expect(names).toEqual(expect.arrayContaining(['get_closed_pnl', 'get_income', 'get_order_history', 'get_balance']));
  });
});

describe('market filters', () => {
  it('reads an empty symbols list as every market', async () => {
    const exchange = { fetchPositions: async () => [order('1', 'BTCUSDT')] };
    expect(await run('get_positions', exchange, { symbols: [] })).toHaveLength(1);
  });
});

describe('order tools', () => {
  it('names the ids that are not open and cancels nothing', async () => {
    const cancelOrders = vi.fn();
    const exchange = { fetchOrders: async () => [order('1', 'BTCUSDT')], cancelOrders };

    await expect(run('cancel_orders', exchange, { orderIds: ['1', '2'] })).rejects.toThrow('No open order with id 2');
    expect(cancelOrders).not.toHaveBeenCalled();
  });

  it('cancels only the market asked for when the adapter returns every open order', async () => {
    const cancelOrders = vi.fn();
    const exchange = { fetchOrders: async () => [order('1', 'BTCUSDT'), order('2', 'ETHUSDT')], cancelOrders };

    await run('cancel_all_orders', exchange, { symbol: 'ETHUSDT' });
    expect(cancelOrders).toHaveBeenCalledWith([order('2', 'ETHUSDT')]);
  });
});
