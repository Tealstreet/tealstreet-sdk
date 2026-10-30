vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName, Market, Order, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const names = ['phemex', 'okx', 'okxeu', 'okxdemo', 'okxeudemo', 'asterdex', 'bitget', 'bitgetinverse', 'bitgetusdc', 'blofin', 'bitrue'] as const;
const market: Market = {
  id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
};
const order = (extra: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100, ...extra,
});
const running: RustEngineExchange[] = [];
const setup = (name: ExchangeName) => {
  const store = new DefaultStore();
  store.update({ markets: [market] });
  const exchange = new RustEngineExchange(name, { key: 'fixture', secret: 'fixture', passphrase: 'fixture-presentation' }, store);
  const command = vi.fn<(command: Record<string, unknown>) => Promise<unknown>>().mockResolvedValue(['native-order']);
  Object.assign(exchange, { host: { command: (input: Record<string, unknown>) => command(input), dispose: vi.fn() } });
  adoptEngineCapabilities(exchange);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  running.push(exchange);
  return { exchange, command, store };
};
afterEach(() => running.splice(0).forEach((exchange) => exchange.dispose()));

describe('Rust cancel-all placement scopes', () => {
  it.each(names)('%s cancels an acknowledged placement before its first store frame', async (name) => {
    const { exchange, command, store } = setup(name);
    await exchange.placeOrder(order());
    expect(store.orders).toEqual([]);
    await exchange.cancelAllOrders();
    expect(command.mock.calls.map(([call]) => call)).toEqual([
      { op: 'placeOrders', orders: [order()] },
      { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
    ]);
  });

  it.each(names)('%s dispatches cancel-all while placement is still in flight', async (name) => {
    const { exchange, command } = setup(name);
    let finish!: (ids: string[]) => void;
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    command.mockImplementation((call) => call.op === 'placeOrders' ? pending : pending.then(() => []));
    const placed = exchange.placeOrder(order());
    const canceled = exchange.cancelAllOrders();
    expect(command.mock.calls.map(([call]) => call.op)).toEqual(['placeOrders', 'cancelSymbolOrders']);
    let completed = false;
    void canceled.then(() => { completed = true; });
    await Promise.resolve();
    expect(completed).toBe(false);
    finish(['native-order']);
    await expect(placed).resolves.toEqual(['native-order']);
    await canceled;
    expect(completed).toBe(true);
  });

  it('retains rejected or uncertain placement scopes conservatively across later cancel-all calls', async () => {
    const { exchange, command } = setup('phemex');
    command.mockRejectedValueOnce(new Error('placement rejected or response lost'));
    await expect(exchange.placeOrder(order())).rejects.toThrow('placement rejected');
    await exchange.cancelAllOrders();
    await exchange.cancelAllOrders();
    expect(command.mock.calls.slice(1).map(([call]) => call)).toEqual([
      { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
      { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
    ]);
  });

  it('attempts later acknowledged scopes after an invalid attempted symbol rejects cancellation', async () => {
    const { exchange, command, store } = setup('phemex');
    command.mockImplementation(async (call) => {
      if (call.op === 'placeOrders' && (call.orders as PlaceOrderOpts[])[0].symbol === 'INVALID') {
        throw new Error('unknown placement symbol');
      }
      if (call.op === 'cancelSymbolOrders' && call.symbol === 'INVALID') {
        throw new Error('unknown cancellation symbol');
      }
      return ['native-order'];
    });
    await expect(exchange.placeOrder(order({ symbol: 'INVALID' }))).rejects.toThrow('unknown placement symbol');
    await exchange.placeOrder(order());
    expect(store.orders).toEqual([]);
    await expect(exchange.cancelAllOrders()).rejects.toThrow('INVALID: unknown cancellation symbol');
    expect(command.mock.calls.filter(([call]) => call.op === 'cancelSymbolOrders').map(([call]) => call.symbol))
      .toEqual(['INVALID', 'BTCUSDT']);
    // Failed scopes remain retained; a later call cannot silently forget uncertainty.
    await expect(exchange.cancelAllOrders()).rejects.toThrow('INVALID: unknown cancellation symbol');
    expect(command.mock.calls.filter(([call]) => call.op === 'cancelSymbolOrders')).toHaveLength(4);
  });

  it('reports every cancellation failure after attempting the complete scope set', async () => {
    const { exchange, command } = setup('okx');
    await exchange.placeOrders([order(), order({ symbol: 'ETHUSDT' }), order({ symbol: 'SOLUSDT' })]);
    command.mockImplementation(async (call) => {
      if (call.symbol !== 'SOLUSDT') throw new Error(`rejected ${call.symbol}`);
      return [];
    });
    await expect(exchange.cancelAllOrders()).rejects.toThrow('BTCUSDT: rejected BTCUSDT; ETHUSDT: rejected ETHUSDT');
    expect(command.mock.calls.slice(1).map(([call]) => call.symbol)).toEqual(['BTCUSDT', 'ETHUSDT', 'SOLUSDT']);
  });

  it('unions stored and attempted scopes, deduplicates them, and awaits cancellations in order', async () => {
    const { exchange, command, store } = setup('okx');
    store.update({ orders: [{ id: 'external', symbol: 'ETHUSDT' }, { id: 'existing', symbol: 'BTCUSDT' }] as Order[] });
    await exchange.placeOrders([order(), order({ symbol: 'SOLUSDT' })]);
    let finish!: () => void;
    command.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
    const canceled = exchange.cancelAllOrders();
    expect(command.mock.calls.slice(1).map(([call]) => call.symbol)).toEqual(['ETHUSDT']);
    finish();
    await canceled;
    expect(command.mock.calls.slice(1).map(([call]) => call.symbol)).toEqual(['ETHUSDT', 'BTCUSDT', 'SOLUSDT']);
  });

  it('does not retain placement scopes rejected by wrapper preflight', async () => {
    const phemex = setup('phemex');
    await expect(phemex.exchange.placeOrder(order({ amount: NaN }))).rejects.toThrow('finite');
    await phemex.exchange.cancelAllOrders();
    expect(phemex.command).not.toHaveBeenCalled();
    const aster = setup('asterdex');
    await expect(aster.exchange.placeOrder(order({ clientOrderId: 'caller123' }))).rejects.toThrow(/clientOrderId/);
    await aster.exchange.cancelAllOrders();
    expect(aster.command).not.toHaveBeenCalled();
  });

  it.each(names.filter((name) => name !== 'asterdex'))('%s forwards caller IDs unchanged', async (name) => {
    const { exchange, command } = setup(name);
    const submitted = order({ clientOrderId: 'caller123' });
    await exchange.placeOrder(submitted);
    await exchange.cancelAllOrders();
    expect(command).toHaveBeenNthCalledWith(1, { op: 'placeOrders', orders: [submitted] });
    expect(submitted.clientOrderId).toBe('caller123');
  });

  it('keeps the generic open-orders cancel-all behavior on unaffected venues', async () => {
    const { exchange, command, store } = setup('backpack');
    // Generic venues select scopes from the authoritative native order read.
    Object.defineProperty(exchange, 'fetchOrders', { value: async () => store.orders });
    await exchange.placeOrder(order());
    await exchange.cancelAllOrders();
    expect(command).toHaveBeenCalledTimes(1);
  });

  it.each(['woo', 'grvt', 'polymarket', 'breakoutprop'] as const)('preserves %s global cancel-all override', async (name) => {
    const { exchange, command } = setup(name);
    await exchange.cancelAllOrders();
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'cancelSymbolOrders', symbol: '' });
  });

  it.each(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'] as const)(
    '%s cancels every product through its native global endpoint even with an empty store', async (name) => {
      const { exchange, command, store } = setup(name);
      await exchange.cancelAllOrders();
      store.update({ orders: [{ id: 'unrelated', symbol: 'ETHUSDT' }] as Order[] });
      await exchange.cancelAllOrders();
      expect(command.mock.calls.map(([call]) => call)).toEqual([
        { op: 'cancelSymbolOrders', symbol: '' },
        { op: 'cancelSymbolOrders', symbol: '' },
      ]);
    },
  );

  it('skips every remaining scope if the account is disposed during cancellation', async () => {
    const { exchange, command } = setup('blofin');
    await exchange.placeOrders([order(), order({ symbol: 'ETHUSDT' })]);
    command.mockImplementationOnce(async () => { exchange.dispose(); });
    await exchange.cancelAllOrders();
    expect(command.mock.calls.slice(1).map(([call]) => call.symbol)).toEqual(['BTCUSDT']);
  });
});
