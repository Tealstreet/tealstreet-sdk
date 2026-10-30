import type { ExchangeName } from '../types';
import type { AppSettingsWorkerType } from '../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../webworker/ExchangeMetadataWorkerType';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { aggregateBybitClosedPnl } from '../journal/bybit.position-adapter';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType, PositionSide } from '../types';
import { ExchangeWorker } from '../webworker/ExchangeWorker';
import { engineCapabilities } from './fixtures/engineCapabilities';

const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 2 };

describe('main-thread Rust trading routing', () => {
  let main: RustEngineExchange | undefined;
  afterEach(() => {
    main?.dispose();
    main = undefined;
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('keeps forwarding to the worker after a main-thread read started a local engine', async () => {
    setWasmEngineLoader(() => new Promise(() => {}));
    main = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    const calls: unknown[] = [];
    main.on('callMethod', (event: { requestId: string; method: string }) => {
      calls.push(event.method);
      main!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: ['1'] });
    });
    // A warm-start `fetchMarkets` starts the instance's own engine in the background.
    void main.fetchMarkets().catch(() => {});
    expect(main.hasStarted).toBe(true);
    await expect(main.placeOrder(order)).resolves.toEqual(['1']);
    expect(calls).toEqual(['placeOrder']);
  });

  it('never runs an engine on a worker mirror used before its bridge is wired', async () => {
    const loader = vi.fn(() => new Promise<never>(() => {}));
    setWasmEngineLoader(loader);
    const store = new DefaultStore();
    main = new RustEngineExchange('bybit', { key: '', secret: '', workerMirror: true }, store);
    const calls: string[] = [];
    const replies: Record<string, unknown> = {
      fetchEngineCapabilities: undefined,
      fetchOrders: [{ id: 'o1' }],
      fetchPositions: [],
      getAccount: { userId: 'u1' },
      placeOrder: ['p1'],
    };
    // The warm start runs before the host wires `callMethod`.
    const capabilities = main.loadCapabilities();
    const orders = main.fetchOrders();
    const positions = main.fetchPositions();
    const account = main.getAccount();
    const markets = main.fetchMarkets();
    void main.start();
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(calls).toEqual([]);
    main.on('callMethod', (event: { requestId: string; method: string }) => {
      calls.push(event.method);
      main!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: replies[event.method] });
    });
    await expect(capabilities).resolves.toBeDefined();
    await expect(orders).resolves.toEqual([{ id: 'o1' }]);
    await expect(positions).resolves.toEqual([]);
    await expect(account).resolves.toEqual({ userId: 'u1' });
    await expect(main.placeOrder(order)).resolves.toEqual(['p1']);
    // The worker's frames, not a local engine, load the mirror's catalogue.
    store.update({ markets: [], loaded: { ...store.loaded, markets: true } });
    await expect(markets).resolves.toEqual([]);
    expect(loader).not.toHaveBeenCalled();
    expect(calls.sort()).toEqual(['fetchEngineCapabilities', 'fetchOrders', 'fetchPositions', 'getAccount', 'placeOrder']);
    expect(main.hasStarted).toBe(false);
    expect(store.orders).toEqual([]);
  });

  it('asks the worker for capabilities after hosted signing built a mirror its signing host', async () => {
    const loader = vi.fn(() => new Promise<never>(() => {}));
    setWasmEngineLoader(loader);
    main = new RustEngineExchange('hyperliquid', { key: '', secret: '', workerMirror: true }, new DefaultStore());
    // A signing-only host exists once main answered a hosted sign request.
    (main as unknown as { host: object }).host = { dispose: () => {}, flush: () => {} };
    const calls: string[] = [];
    main.on('callMethod', (event: { requestId: string; method: string }) => {
      calls.push(event.method);
      main!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: undefined });
    });
    await expect(main.loadCapabilities()).resolves.toBeDefined();
    expect(calls).toEqual(['fetchEngineCapabilities']);
    expect(main.hasStarted).toBe(false);
    expect(loader).not.toHaveBeenCalled();
  });

  it('settles an idle mirror read on disposal without waiting for the store', async () => {
    main = new RustEngineExchange('bybit', { key: '', secret: '', workerMirror: true }, new DefaultStore());
    const markets = main.fetchMarkets();
    main.dispose();
    await expect(markets).rejects.toThrow('disposed');
  });

  it('strips the mirror flag from the options a worker runs its engine with', () => {
    const worker = new ExchangeWorker(
      { postMessage: vi.fn() } as unknown as Worker,
      'mirror',
      'bybit',
      { key: 'k', secret: 's', workerMirror: true },
      {} as AppSettingsWorkerType,
      {} as ExchangeMetadataWorkerType,
      'session',
    );
    expect((worker as unknown as { _options: { workerMirror?: boolean } })._options.workerMirror).toBeUndefined();
    worker.dispose();
  });

  it('waits longer for multi-command methods than for one command', async () => {
    vi.useFakeTimers();
    main = new RustEngineExchange(
      'binance',
      { key: 'k', secret: 's', extra: { rustCommandTimeoutMs: 100 } },
      new DefaultStore(),
    );
    main.on('callMethod', () => {});
    const place = main.placeOrder(order).catch((error: unknown) => error);
    const nuke = main.nuke().catch((error: unknown) => error);
    let nukeSettled = false;
    void nuke.then(() => (nukeSettled = true));
    await vi.advanceTimersByTimeAsync(1100);
    expect(await place).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'placeOrder', outcome: 'unknown' });
    expect(nukeSettled).toBe(false);
    await vi.advanceTimersByTimeAsync(400);
    expect(await nuke).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'nuke', outcome: 'unknown' });
  });

  it('answers a request buffered before startup as never sent when the worker stops', () => {
    const postMessage = vi.fn();
    const worker = new ExchangeWorker(
      { postMessage } as unknown as Worker,
      'buffered',
      'binance',
      { key: 'k', secret: 's', engine: 'rust' },
      {} as AppSettingsWorkerType,
      {} as ExchangeMetadataWorkerType,
      'session',
    );
    worker.handlePayload({
      op: 'callMethod',
      data: { requestId: 'r-1', method: 'placeOrder', args: [{ ...order, clientOrderId: 'c-1' }] },
    });
    expect(postMessage).not.toHaveBeenCalled();
    worker.dispose();
    expect(postMessage).toHaveBeenCalledWith({
      op: 'callMethod:done',
      data: {
        requestId: 'r-1',
        success: false,
        result: expect.objectContaining({ outcome: 'notSent', command: 'placeOrder' }),
      },
    });
  });

  it('learns capability methods from the worker and forwards Lighter tier changes there', async () => {
    const loader = vi.fn(() => new Promise<never>(() => {}));
    setWasmEngineLoader(loader);
    main = new RustEngineExchange('lighter', { key: 'k', secret: 's', apiKeyIndex: '3' }, new DefaultStore());
    const calls: Array<{ method: string; args: unknown[] }> = [];
    const answers: Record<string, unknown> = {
      fetchEngineCapabilities: { accountTier: true, fillHistory: false, orderHistory: true, incomeHistory: true },
      changeAccountTier: null,
      fetchAccountTier: 'premium',
    };
    main.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      queueMicrotask(() =>
        main!.emitter.emit('callMethod:done', {
          requestId: event.requestId,
          success: true,
          result: answers[event.method],
        }),
      );
    });
    expect(main.changeAccountTier).toBeUndefined();
    main.store.update({ loaded: { ...main.store.loaded, markets: true } });
    await vi.waitFor(() => expect(main!.changeAccountTier).toBeTypeOf('function'));
    expect(main.capabilities.fillHistory).toBe(false);
    await main.changeAccountTier!('premium');
    await expect(main.fetchAccountTier!()).resolves.toBe('premium');
    expect(calls).toEqual([
      { method: 'fetchEngineCapabilities', args: [] },
      { method: 'changeAccountTier', args: ['premium'] },
      { method: 'fetchAccountTier', args: [] },
    ]);
    expect(main.hasStarted).toBe(false);
    expect(loader).not.toHaveBeenCalled();
  });

  it('forwards main-thread candle and journal reads to the worker', async () => {
    main = new RustEngineExchange('kraken', { key: 'k', secret: 's' }, new DefaultStore());
    const calls: Array<{ method: string; args: unknown[] }> = [];
    main.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      queueMicrotask(() =>
        main!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: [] }),
      );
    });
    const window = { from: 1, to: 2 };
    await main.fetchOHLCV({ symbol: 'PF_XBTUSD', interval: '1m', from: 1, to: 2 });
    await main.fetchHistoricalFills(window);
    await main.fetchHistoricalOrders(window);
    await main.fetchIncomeHistory({ startTime: 1, endTime: 2 });
    await main.fetchIncomeHistoryRange(1, 2);
    expect(calls.map(({ method }) => method)).toEqual([
      'fetchOHLCV',
      'fetchHistoricalFills',
      'fetchHistoricalOrders',
      'fetchIncomeHistory',
      'fetchIncomeHistoryRange',
    ]);
    expect(main.hasStarted).toBe(false);
  });

  /** A mirror whose worker (or hosted server) answers each method from `answers`. */
  const answering = (name: ExchangeName, answers: Record<string, unknown>) => {
    setWasmEngineLoader(() => new Promise(() => {}));
    main = new RustEngineExchange(name, { key: 'k', secret: 's' }, new DefaultStore());
    const calls: Array<{ method: string; args: unknown[] }> = [];
    main.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      queueMicrotask(() =>
        main!.emitter.emit('callMethod:done', {
          requestId: event.requestId,
          success: true,
          result: answers[event.method],
        }),
      );
    });
    return calls;
  };

  it('folds the Bybit journal on the mirror from forwarded raw closed-position rows and orders', async () => {
    const rows = [
      {
        symbol: 'BTCUSDT',
        orderId: 'o1',
        side: 'Sell',
        closedSize: '1',
        avgEntryPrice: '100',
        avgExitPrice: '110',
        closedPnl: '10',
        createdTime: '1000',
        updatedTime: '1001',
      },
    ];
    const orders = [{ id: 'o1', positionSide: PositionSide.Long }];
    const calls = answering('bybit', {
      fetchEngineCapabilities: JSON.parse(engineCapabilities('bybit')),
      fetchClosedPositions: rows,
      fetchHistoricalOrders: orders,
    });
    const window = { from: 1_000, to: 2_000 };
    const records = await main!.fetchHistoricalPositions!({ ...window, account: 'a' });
    expect(records).toEqual(
      aggregateBybitClosedPnl(rows, {
        account: 'a',
        resolveSettleCurrency: () => 'USDT',
        resolveLeg: (orderId: string) => (orderId === 'o1' ? PositionSide.Long : undefined),
      }),
    );
    expect(records.length).toBe(1);
    expect(calls).toEqual([
      { method: 'fetchEngineCapabilities', args: [] },
      { method: 'fetchClosedPositions', args: [window] },
      { method: 'fetchHistoricalOrders', args: [window] },
    ]);
    expect(main!.hasStarted).toBe(false);
  });

  it('folds a fill venue journal on the mirror from forwarded fills and income', async () => {
    const calls = answering('kraken', {
      fetchEngineCapabilities: JSON.parse(engineCapabilities('kraken')),
      fetchHistoricalFills: [],
      fetchIncomeHistoryRange: [],
    });
    await expect(main!.fetchHistoricalPositions!({ from: 1_000, to: 2_000, account: 'a' })).resolves.toEqual([]);
    const methods = calls.map(({ method }) => method);
    expect(methods[0]).toBe('fetchEngineCapabilities');
    expect(methods).toContain('fetchHistoricalFills');
    expect(methods).not.toContain('fetchHistoricalPositions');
    expect(main!.hasStarted).toBe(false);
  });

  it('answers a mirror journal read on the worker with the engine rows', async () => {
    const worker = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, new DefaultStore());
    const command = vi
      .spyOn(worker as unknown as { command: (c: Record<string, unknown>) => Promise<unknown> }, 'command')
      .mockResolvedValue([{ symbol: 'BTCUSDT' }]);
    await expect(worker.fetchClosedPositions({ symbol: 'BTCUSDT', from: 1, to: 2 })).resolves.toEqual([
      { symbol: 'BTCUSDT' },
    ]);
    expect(command.mock.calls[0][0]).toEqual({ op: 'fetchClosedPositions', symbol: 'BTCUSDT', from: 1, to: 2 });
    worker.dispose();
  });

  it('forwards main-thread native account reads to the worker without starting an engine', async () => {
    const construct = vi.fn();
    setWasmEngineLoader(async () => {
      construct();
      return new Promise(() => {});
    });
    main = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    const calls: string[] = [];
    main.on('callMethod', (event: { requestId: string; method: string }) => {
      calls.push(event.method);
      const result = event.method === 'fetchPositionMode' ? true : event.method === 'fetchBalance' ? {} : [];
      queueMicrotask(() => main!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result }));
    });
    await main.fetchOrders();
    await main.fetchPositions();
    await main.fetchBalance();
    await main.fetchPositionMode();
    expect(calls).toEqual(['fetchOrders', 'fetchPositions', 'fetchBalance', 'fetchPositionMode']);
    expect(main.hasStarted).toBe(false);
    expect(construct).not.toHaveBeenCalled();
  });

  it('leaves a mirror store to worker frames instead of syncing a local engine', () => {
    setWasmEngineLoader(() => new Promise(() => {}));
    main = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    main.on('callMethod', () => {});
    expect(() => main!.syncAccountState()).not.toThrow();
    expect(main.hasStarted).toBe(false);
  });
});
