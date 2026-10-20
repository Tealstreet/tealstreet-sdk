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
});
