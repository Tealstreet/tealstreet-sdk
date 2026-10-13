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
  it('rounds the price to the tick with fresh positions and orders before placing', async () => {
    const placeOrder = vi.fn(async () => ['id']);
    const market = {
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    };
    const exchange = {
      name: 'bybit',
      store: { markets: [market] },
      fetchPositions: async () => [],
      fetchOrders: async () => [],
      fetchTickers: async () => [],
      placeOrder,
    } as any;

    await run('place_order', exchange, {
      symbol: 'BTCUSDT',
      side: 'buy',
      type: 'limit',
      amount: 0.001,
      price: 79530.94,
    });
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ price: 79530.9, amount: 0.001 }));
  });

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
