import type { AppSettingsWorkerType } from '../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../webworker/ExchangeMetadataWorkerType';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType } from '../types';
import { ExchangeWorker } from '../webworker/ExchangeWorker';

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
    await main.fetchHistoricalPositions!({ ...window, account: 'a' });
    expect(calls.map(({ method }) => method)).toEqual([
      'fetchOHLCV',
      'fetchHistoricalFills',
      'fetchHistoricalOrders',
      'fetchIncomeHistory',
      'fetchIncomeHistoryRange',
      'fetchHistoricalPositions',
    ]);
    expect(main.hasStarted).toBe(false);
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
