import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, Market, PlaceOrderOpts, StoreData } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const market: Market = {
  id: 'BTC-USDT', symbol: 'BTC-USDT', base: 'BTC', quote: 'USDT', active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 1 } }, info: {},
};
const order: PlaceOrderOpts = { symbol: 'BTC-USDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100 };

describe('KuCoin spot Rust SDK boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));
  const setup = (options: Partial<ExchangeOptions> = {}) => {
    const commands: Array<Record<string, unknown>> = [];
    const noActions = () => '[]';
    let rails: StoreData['fetchedLeverage'] | undefined;
    const engine: WasmEngineLike = {
      capabilities: () => engineCapabilities('kucoinspot'),
      start: noActions,
      command: (json) => {
        // The caller wait is engine receipt metadata (pinned by the host tests),
        // not part of the native command these regressions compare.
        const { timeoutMs: _wait, ...command } = JSON.parse(json);
        if (command.op === 'setTickerSymbols' || command.op === 'setPollScale') return '[]';
        commands.push(command);
        // KuCoin spot leverage is fixed: the engine confirms 1x rails for the symbol.
        if (command.op === 'setLeverage') rails = { [command.symbol]: { buyLeverage: 1, sellLeverage: 1 } };
        const result = command.op === 'fetchAccount' ? { userId: 'user', subId: null, affiliateId: null }
          : command.op === 'findOrderByClientOrderId' ? { found: false, definitive: false }
          : command.op === 'placeOrders' || command.op === 'amendOrderWithSymbol' ? ['native-id'] : null;
        return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result) }]);
      },
      takeFrame: () => new Uint8Array(consumerFrame({
        markets: [market],
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
        ...(rails ? { fetchedLeverage: rails } : {}),
      })), snapshotFrame: () => new Uint8Array(),
      wsOpen: noActions, wsMessage: noActions, wsClose: noActions,
      httpStart: noActions, httpChunk: noActions, httpEnd: noActions, httpFailed: noActions, timer: noActions,
    };
    const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
    setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
    const store = new DefaultStore();
    installNativeStartupFrames(engine, store, () => (rails ? { fetchedLeverage: rails } : {}));
    store.update({ markets: [market], loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
    const exchange = new RustEngineExchange('kucoinspot', { key: 'key', secret: 'secret', passphrase: 'passphrase', ...options }, store);
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchanges.push(exchange);
    return { exchange, commands, construct, store };
  };

  it('registers a spot account and loads once for account queries before startup', async () => {
    const { exchange, construct } = setup();
    expect(isRustVenue('kucoinspot')).toBe(true);
    expect(exchange.isSpot).toBe(true);
    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'user', subId: null, affiliateId: null });
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    expect(construct.mock.calls[0][0]).toBe('kucoinspot');
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({ key: 'key', secret: 'secret', passphrase: 'passphrase' });
  });

  it('preserves caller IDs, rejects invalid IDs and marks open-only misses', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    await expect(exchange.placeOrder({ ...order, clientOrderId: 'Caller_01-id' })).resolves.toEqual(['native-id']);
    expect(commands[0]).toMatchObject({ op: 'placeOrders', orders: [{ ...order, clientOrderId: 'Caller_01-id' }] });
    await expect(exchange.placeOrder({ ...order, clientOrderId: 'has space' })).rejects.toThrow('clientOrderId');
    await expect(exchange.findOrderByClientOrderId('BTC-USDT', 'Caller_01-id')).resolves.toEqual({ found: false, definitive: false });
  });

  it('cancels account-wide and preserves TS fixed leverage/no-op mode setters', async () => {
    const { exchange, commands, store } = setup();
    await exchange.start();
    await exchange.cancelAllOrders();
    await exchange.setLeverage('BTC-USDT', 20);
    await exchange.changeMarginType(true);
    await exchange.changePositionMode(true);
    expect(commands.map(({ command: _command, ...body }) => body)).toEqual([
      { op: 'cancelSymbolOrders', symbol: '' }, { op: 'setLeverage', symbol: 'BTC-USDT', leverage: 20 },
      { op: 'setMarginType', symbol: '', isolated: true }, { op: 'setPositionMode', hedged: true },
    ]);
    expect(store.fetchedLeverage['BTC-USDT']).toMatchObject({ buyLeverage: 1, sellLeverage: 1 });
  });

  it('keeps the supplied symbol when canceling or amending an uncached order', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    const uncached = { id: 'lost-ack-id', symbol: 'BTC-USDT', type: OrderType.Limit } as Parameters<typeof exchange.cancelOrders>[0][number];
    await exchange.cancelOrders([uncached]);
    await exchange.updateOrder({ order: uncached, update: { price: 101 } });
    expect(commands.map(({ command: _command, ...body }) => body)).toEqual([
      { op: 'cancelOrdersWithSymbols', orders: [{ id: 'lost-ack-id', symbol: 'BTC-USDT', type: OrderType.Limit }] },
      { op: 'amendOrderWithSymbol', id: 'lost-ack-id', symbol: 'BTC-USDT', price: 101 },
    ]);
  });

  it('retains the TS unsupported-history brands for ingestion', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    const issued = commands.length;
    expect(exchange.capabilities).toMatchObject({
      orderHistory: false, closedPnl: false, incomeHistory: false, incomeRange: false,
    });
    await expect(exchange.fetchHistoricalOrders()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistory()).resolves.toEqual([]);
    expect(commands).toHaveLength(issued);
  });
});
