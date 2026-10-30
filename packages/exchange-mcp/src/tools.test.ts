import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '@tealstreet/safe-cex/src/exchanges/registry';
import { RustEngineExchange } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { adoptEngineCapabilities } from '@tealstreet/safe-cex/src/__tests__/fixtures/engineCapabilities';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';

import { supportedTools, tools } from './tools';

const live: any[] = [];
afterEach(() => live.splice(0).forEach((e) => e.dispose?.()));

const construct = async (name: 'bybit' | 'binancepm', options = false) => {
  const Exchange = await exchangeRegistry[name]();
  const exchange = new Exchange(name, { key: 'k', secret: 's', publicDataOnly: true }, new DefaultStore());
  live.push(exchange);
  // Server tool discovery runs after loading the engine capability record.
  adoptEngineCapabilities(exchange as RustEngineExchange, { options });
  return exchange as unknown as Exchange;
};

const run = (name: string, exchange: object, args: object) =>
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

describe('option tools', () => {
  const OPTION_TOOLS = ['get_option_chain', 'get_option_expiries', 'get_account_greeks'];
  const names = (exchange: Exchange, options: boolean) => supportedTools(exchange, { options }).map((t) => t.name);

  it('lists option tools only when the server runs with options on', async () => {
    const bybit = await construct('bybit', true);
    expect(names(bybit, true)).toEqual(expect.arrayContaining(OPTION_TOOLS));
    for (const name of OPTION_TOOLS) expect(names(bybit, false)).not.toContain(name);
  });

  it('loads the options-only beta venue through the Rust registry', async () => {
    await expect(exchangeRegistry.binanceoptions()).resolves.toBe(RustEngineExchange);
  });

  it('does not advertise cold Rust option methods without native support', async () => {
    const cold = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    live.push(cold);
    for (const name of OPTION_TOOLS) expect(names(cold, true)).not.toContain(name);
  });

  it('lists none on a venue without option methods, even with options on', async () => {
    const listed = names(await construct('binancepm'), true);
    for (const name of OPTION_TOOLS) expect(listed).not.toContain(name);
  });

  it('leaves option markets out of get_markets unless named', async () => {
    const markets = [
      { symbol: 'BTCUSDT', base: 'BTC' },
      { symbol: 'BTC-6NOV26-84000-C-USDT', base: 'BTC', option: { strike: 84000 } },
    ];
    const exchange = { store: { markets } };
    expect(await run('get_markets', exchange, {})).toEqual(['BTCUSDT']);
    expect(await run('get_markets', exchange, { search: 'btc' })).toHaveLength(1);
    expect(await run('get_markets', exchange, { symbols: ['BTC-6NOV26-84000-C-USDT'] })).toHaveLength(1);
  });

  it('flattens the chain and states the greek units once', async () => {
    const market = {
      symbol: 'BTC-6NOV26-84000-C-USDT',
      option: { strike: 84000, right: 'call', expiry: 1793952000000 },
    };
    const quote = {
      bid: 2865,
      ask: 2895,
      mark: 2897,
      markIv: 0.35,
      bidIv: 0.34,
      askIv: 0.35,
      underlyingPrice: 83893,
      greeks: { delta: 0.51, gamma: 0.00005, vega: 84.2, theta: -63.7, units: 'per 1 vol point' },
    };
    const exchange = { fetchOptionChain: async () => [{ market, quote }] };

    expect(await run('get_option_chain', exchange, { underlying: 'BTC' })).toEqual({
      units: 'per 1 vol point',
      rows: [
        {
          symbol: market.symbol,
          strike: 84000,
          right: 'call',
          expiry: 1793952000000,
          date: '2026-11-06T08:00:00.000Z',
          bid: 2865,
          ask: 2895,
          mark: 2897,
          markIv: 0.35,
          bidIv: 0.34,
          askIv: 0.35,
          underlyingPrice: 83893,
          delta: 0.51,
          gamma: 0.00005,
          vega: 84.2,
          theta: -63.7,
        },
      ],
    });
  });
});

describe('dates', () => {
  it('adds a UTC date next to each time field, reading seconds and milliseconds correctly', async () => {
    const exchange = {
      fetchHistoricalFills: async () => [{ symbol: 'BTCUSDT', timestamp: 1791860271 }],
      fetchIncomeHistory: async () => [{ symbol: 'BTCUSDT', timestamp: 1791820800000 }],
    };
    expect(await run('get_fills', exchange, { symbol: 'BTCUSDT' })).toEqual([
      { symbol: 'BTCUSDT', timestamp: 1791860271, timestampUtc: '2026-10-13T02:57:51.000Z' },
    ]);
    expect(await run('get_income', exchange, {})).toEqual([
      { symbol: 'BTCUSDT', timestamp: 1791820800000, timestampUtc: '2026-10-12T16:00:00.000Z' },
    ]);
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

  it('rounds an amended size to the lot step and refuses one that rounds to zero', async () => {
    const updateOrder = vi.fn(async () => ['id']);
    const market = { symbol: 'BTCUSDT', precision: { amount: 0.001, price: 0.1 } };
    const exchange = {
      store: { markets: [market] },
      fetchOrders: async () => [order('1', 'BTCUSDT')],
      updateOrder,
    } as any;

    await run('amend_order', exchange, { orderId: '1', amount: 0.0123 });
    expect(updateOrder).toHaveBeenCalledWith(expect.objectContaining({ update: { amount: 0.012 } }));

    await expect(run('amend_order', exchange, { orderId: '1', amount: 0.0002 })).rejects.toThrow(
      'below the venue lot size',
    );
    expect(updateOrder).toHaveBeenCalledTimes(1);
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

it.each(['get_order_history', 'get_fills', 'get_closed_pnl', 'get_income'])(
  '%s preserves the incomplete marker while adding UTC fields',
  async (name) => {
    const rows = Object.defineProperty([{ timestamp: 1 }], 'incomplete', { value: true });
    const exchange = {
      fetchHistoricalOrders: async () => rows,
      fetchHistoricalFills: async () => rows,
      fetchClosedPnl: async () => rows,
      fetchIncomeHistory: async () => rows,
    };
    const result = await run(name, exchange, {});
    expect(result).toHaveProperty('incomplete', true);
    expect(Object.getOwnPropertyDescriptor(result, 'incomplete')?.enumerable).toBe(false);
    expect(rows).toEqual([{ timestamp: 1 }]);
  },
);
