import type { EngineCapabilities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { Order } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const running: RustEngineExchange[] = [];

/** A venue whose bridge behaviour comes only from the record its engine reports. */
const fixture = (capabilities: Partial<EngineCapabilities>) => {
  const commands: Array<Record<string, unknown>> = [];
  const started = vi.fn(() => '[]');
  const noActions = () => '[]';
  const replies: Record<string, unknown> = {
    fetchAccount: { userId: 'account-1' },
    fetchLeverage: { buyLeverage: 20, sellLeverage: 50 },
    findOrderByClientOrderId: { found: false, definitive: true },
  };
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('woo', capabilities),
    start: started,
    command(json) {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols') return '[]';
      commands.push(command);
      return JSON.stringify([
        { op: 'done', command: command.command, result: JSON.stringify(replies[command.op] ?? []) },
      ]);
    },
    takeFrame: () =>
      new Uint8Array(
        consumerFrame({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } }),
      ),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: vi.fn(function () {
      return engine;
    }) as unknown as WasmEngineModule['WasmEngine'],
  }));
  const exchange = new RustEngineExchange('woo', { key: 'key', secret: 'secret' }, new DefaultStore());
  running.push(exchange);
  return { exchange, commands, started };
};

const order = (id: string, symbol: string): Order =>
  ({
    id,
    symbol,
    status: OrderStatus.Open,
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 1,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
  }) as Order;

describe('Rust bridge routes from the engine capability record', () => {
  afterEach(() => {
    for (const exchange of running.splice(0)) exchange.dispose();
    vi.restoreAllMocks();
  });

  it('is unknown until the engine exists, then brands the missing history axes', async () => {
    const { exchange } = fixture({ fillHistory: false, orderHistory: false, incomeHistory: false });
    expect(exchange.engineCapabilities).toBeUndefined();
    expect(exchange.capabilities.orderHistory).toBe(true);
    await exchange.start();
    expect(exchange.engineCapabilities?.cancelAll).toBe(true);
    expect(exchange.capabilities).toMatchObject({
      fillHistory: false,
      orderHistory: false,
      incomeHistory: false,
      incomeRange: false,
    });
  });

  it('keeps reported history axes as engine queries', async () => {
    const { exchange } = fixture({});
    await exchange.start();
    expect(exchange.capabilities).toMatchObject({
      fillHistory: true,
      orderHistory: true,
      incomeHistory: true,
      incomeRange: true,
    });
    expect(exchange.fetchFillsForOrderIds).toBeUndefined();
  });

  it('cancels everything in one native command only where the engine supports it', async () => {
    const native = fixture({ cancelAll: true });
    await native.exchange.start();
    await native.exchange.cancelAllOrders();
    expect(native.commands.filter((c) => c.op === 'cancelSymbolOrders')).toEqual([
      expect.objectContaining({ symbol: '' }),
    ]);

    const perSymbol = fixture({ cancelAll: false });
    await perSymbol.exchange.start();
    perSymbol.exchange.store.update({ orders: [order('a', 'BTC'), order('b', 'ETH'), order('c', 'BTC')] });
    await perSymbol.exchange.cancelAllOrders();
    expect(perSymbol.commands.filter((c) => c.op === 'cancelSymbolOrders').map((c) => c.symbol)).toEqual([
      'BTC',
      'ETH',
    ]);
  });

  it('reads account identity as the record says', async () => {
    const unsupported = fixture({ account: 'unsupported' });
    await expect(unsupported.exchange.getAccount()).rejects.toThrow('Not implemented');
    expect(unsupported.commands).toEqual([]);
    expect(unsupported.started).not.toHaveBeenCalled();

    const query = fixture({ account: 'query' });
    await expect(query.exchange.getAccount()).resolves.toEqual({ userId: 'account-1' });
    expect(query.started).not.toHaveBeenCalled();

    const afterLoad = fixture({ account: 'afterLoad' });
    await expect(afterLoad.exchange.getAccount()).resolves.toEqual({ userId: 'account-1' });
    expect(afterLoad.started).toHaveBeenCalledTimes(1);
  });

  it('asks the venue for a client order id only where it can answer', async () => {
    const venue = fixture({ clientOrderLookup: true });
    await venue.exchange.start();
    await expect(venue.exchange.findOrderByClientOrderId('BTC', 'id-1')).resolves.toEqual({
      found: false,
      definitive: true,
    });
    expect(venue.commands.map((c) => c.op)).toContain('findOrderByClientOrderId');

    const store = fixture({ clientOrderLookup: false });
    await store.exchange.start();
    await store.exchange.findOrderByClientOrderId('BTC', 'id-1');
    expect(store.commands.map((c) => c.op)).not.toContain('findOrderByClientOrderId');
  });

  it('reads leverage rails on demand only where the venue answers', async () => {
    const onRead = fixture({ leverageQuery: 'onRead' });
    await onRead.exchange.start();
    await expect(onRead.exchange.fetchLeverageForSymbol('BTC')).resolves.toEqual({
      buyLeverage: 20,
      sellLeverage: 50,
    });

    const settings = fixture({ leverageQuery: 'settings' });
    await settings.exchange.start();
    await expect(settings.exchange.fetchLeverageForSymbol('BTC')).resolves.toBeNull();
    expect(settings.commands.map((c) => c.op)).not.toContain('fetchLeverage');
  });

  it('takes maximum leverage from the reported source', async () => {
    const rails = fixture({ maxLeverage: 'rails' });
    await rails.exchange.start();
    await expect(rails.exchange.fetchSymbolMaxLeverage('BTC')).resolves.toBe(50);

    const unsupported = fixture({ maxLeverage: 'unsupported' });
    await unsupported.exchange.start();
    await expect(unsupported.exchange.fetchSymbolMaxLeverage('BTC')).rejects.toThrow('Not implemented');
  });

  it('exposes fills by order id only where the engine filters by it', async () => {
    const { exchange } = fixture({ fillsByOrderId: true });
    await exchange.start();
    expect(exchange.fetchFillsForOrderIds).toBeTypeOf('function');
  });

  it('rejects option orders unless the engine reports options', async () => {
    const { exchange, commands } = fixture({ options: false });
    await exchange.start();
    await expect(
      exchange.placeOrders([
        { symbol: 'BTC', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 1, impliedVolatility: 0.5 },
      ]),
    ).rejects.toThrow('Options are not enabled on this account');
    expect(commands.map((c) => c.op)).not.toContain('placeOrders');
  });
});
