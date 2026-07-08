import type { Store } from '../store/store.interface';
import type { ExchangeOptions, RemoteSigner } from '../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from './base';

// Minimal Store stub — BaseExchange's constructor only calls store.subscribe(),
// and dispose() only calls store.reset(). Nothing in the methods under test
// touches the store further.
const makeStubStore = (): Store =>
  ({
    subscribe: vi.fn(() => () => {}),
    reset: vi.fn(),
  }) as unknown as Store;

const makeExchange = (opts: Partial<ExchangeOptions> = {}) => {
  const store = makeStubStore();
  return new BaseExchange('bybit' as any, opts as ExchangeOptions, store);
};

// tiny-emitter stores registered handlers in `.e[event]` as an array.
const listenerCount = (exchange: BaseExchange, event: string): number => {
  const e = (exchange.emitter as any).e;
  return e && e[event] ? e[event].length : 0;
};

describe('BaseExchange.callMethodInWorker', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('removes its callMethod:done listener when the worker call times out', async () => {
    const exchange = makeExchange();
    const baseline = listenerCount(exchange, 'callMethod:done');

    const promise = (exchange.callMethodInWorker as any)('fetchOrders');
    // Swallow the eventual rejection so it doesn't surface as unhandled.
    const settled = promise.then(
      () => 'resolved',
      (err: Error) => err.message,
    );

    // Listener is attached while the call is in-flight.
    expect(listenerCount(exchange, 'callMethod:done')).toBe(baseline + 1);

    // Fire the 5s timeout.
    await vi.advanceTimersByTimeAsync(5000);

    expect(await settled).toContain('Timeout');
    // Listener must be removed on the timeout path (leak fix).
    expect(listenerCount(exchange, 'callMethod:done')).toBe(baseline);
  });

  it('removes its updatePositions:done listener when the worker call times out', async () => {
    const exchange = makeExchange();
    const baseline = listenerCount(exchange, 'updatePositions:done');

    const promise = exchange.updatePositionsInWebworker();
    const settled = promise.then(
      () => 'resolved',
      (err: Error) => err.message,
    );

    expect(listenerCount(exchange, 'updatePositions:done')).toBe(baseline + 1);

    await vi.advanceTimersByTimeAsync(5000);

    expect(await settled).toContain('Timeout');
    expect(listenerCount(exchange, 'updatePositions:done')).toBe(baseline);
  });

  it('still removes the listener on the normal resolve path', async () => {
    const exchange = makeExchange();
    const baseline = listenerCount(exchange, 'callMethod:done');

    let capturedRequestId: string | undefined;
    // Capture the requestId emitted alongside the callMethod op.
    exchange.emitter.on('callMethod', (payload: any) => {
      capturedRequestId = payload.requestId;
    });

    const promise = (exchange.callMethodInWorker as any)('fetchOrders');
    const settled = promise.then(
      (r: any) => r,
      (err: Error) => err.message,
    );

    // Emit the matching done event before the timeout fires.
    exchange.emitter.emit('callMethod:done', {
      requestId: capturedRequestId,
      success: true,
      result: ['ok'],
    });

    await settled;
    expect(listenerCount(exchange, 'callMethod:done')).toBe(baseline);
  });
});

describe('BaseExchange.waitForRemoteSigner', () => {
  const makeSigner = () => {
    let cb: ((available: boolean) => void) | undefined;
    const unsubscribe = vi.fn(() => {
      cb = undefined;
    });
    const signer: RemoteSigner = {
      sign: vi.fn(),
      isAvailable: vi.fn(() => false),
      onAvailabilityChange: vi.fn((c: (available: boolean) => void) => {
        cb = c;
        return unsubscribe;
      }),
    };
    return {
      signer,
      unsubscribe,
      fireAvailable: () => cb?.(true),
    };
  };

  it('resolves false when the exchange is disposed while waiting', async () => {
    const { signer, unsubscribe } = makeSigner();
    const exchange = makeExchange({ remoteSigner: signer });

    const waitPromise = (exchange as any).waitForRemoteSigner() as Promise<boolean>;

    // Guard against a hang if the promise never settles (the bug).
    const raced = await Promise.race([
      waitPromise,
      new Promise<'PENDING'>((resolve) => setTimeout(() => resolve('PENDING'), 100)),
    ]);
    expect(raced).toBe('PENDING'); // still pending before dispose

    exchange.dispose();

    const result = await waitPromise;
    expect(result).toBe(false);
    expect(unsubscribe).toHaveBeenCalled();
  });

  it('resolves true when the signer becomes available normally', async () => {
    const { signer, unsubscribe, fireAvailable } = makeSigner();
    const exchange = makeExchange({ remoteSigner: signer });

    const waitPromise = (exchange as any).waitForRemoteSigner() as Promise<boolean>;

    fireAvailable();

    const result = await waitPromise;
    expect(result).toBe(true);
    expect(unsubscribe).toHaveBeenCalled();
    // Cleanup entry was removed so dispose() won't double-resolve.
    expect((exchange as any).remoteSignerAvailabilityCleanups.size).toBe(0);
  });
});
