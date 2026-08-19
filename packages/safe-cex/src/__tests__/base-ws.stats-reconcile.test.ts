import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseWebSocket } from '../exchanges/base.ws';

const createSocket = () => new BaseWebSocket({ name: 'test', isDisposed: false } as any) as any;

// The reconcile is throttled off wall-clock, so every test drives Date.now().
let clock = 1_000_000;
const advance = (ms: number) => {
  clock += ms;
};

beforeEach(() => {
  clock = 1_000_000;
  vi.spyOn(Date, 'now').mockImplementation(() => clock);
});

describe('recordMessageHandlerStats / reconcileMessageHandlerStats', () => {
  it('zeroes a handler-backed topic once its handlers go away', () => {
    const ws = createSocket();
    ws.messageHandlers['orderBook_BTCUSDT'] = [() => undefined];
    ws.recordMessageHandlerStats('orderBook_BTCUSDT');

    expect(ws.messageHandlersStats.orderBook_BTCUSDT.lastCallbacksCount).toBe(1);
    expect(ws.messageHandlersStats.orderBook_BTCUSDT.hadHandlers).toBe(true);

    // Teardown, then a frame on some other topic drives the throttled walk.
    ws.messageHandlers['orderBook_BTCUSDT'] = [];
    advance(6000);
    ws.recordMessageHandlerStats('ticker');

    expect(ws.messageHandlersStats.orderBook_BTCUSDT.lastCallbacksCount).toBe(0);
  });

  it('zeroes it even when the exchange DELETES the handler key on teardown', () => {
    const ws = createSocket();
    ws.messageHandlers['orderBook_BTCUSDT'] = [() => undefined];
    ws.recordMessageHandlerStats('orderBook_BTCUSDT');

    // bitget/binance/cryptocom/kucoinspot/weex delete rather than empty. Without
    // `hadHandlers` this is indistinguishable from a never-handler-backed topic
    // and the `: 1` fallback would pin it active forever.
    delete ws.messageHandlers['orderBook_BTCUSDT'];
    advance(6000);
    ws.recordMessageHandlerStats('ticker');

    expect(ws.messageHandlersStats.orderBook_BTCUSDT.lastCallbacksCount).toBe(0);
  });

  it('leaves never-handler-backed topics alone', () => {
    const ws = createSocket();
    // `pong` and bare `ticker` have no `messageHandlers` key on most venues and
    // are often a socket's only liveness signal — zeroing them would blind the
    // staleness check and can synthesize a false Disconnected.
    ws.recordMessageHandlerStats('pong');
    expect(ws.messageHandlersStats.pong.hadHandlers).toBe(false);

    advance(6000);
    ws.recordMessageHandlerStats('pong');

    expect(ws.messageHandlersStats.pong.lastCallbacksCount).toBe(1);
  });

  it('does not walk the map on every frame', () => {
    const ws = createSocket();
    ws.messageHandlers['orderBook_BTCUSDT'] = [() => undefined];
    ws.recordMessageHandlerStats('orderBook_BTCUSDT');
    ws.messageHandlers['orderBook_BTCUSDT'] = [];

    advance(100);
    ws.recordMessageHandlerStats('ticker');

    expect(ws.messageHandlersStats.orderBook_BTCUSDT.lastCallbacksCount).toBe(1);
  });

  it('reconciles immediately when an exchange calls cleanup', () => {
    const ws = createSocket();
    ws.messageHandlers['a'] = [() => undefined];
    ws.messageHandlers['b'] = [() => undefined];
    ws.recordMessageHandlerStats('a');
    ws.recordMessageHandlerStats('b');

    ws.messageHandlers['a'] = [];
    ws.messageHandlers['b'] = [];
    advance(300);
    // Only `a` is cleaned, but the forced walk catches `b` too — a teardown that
    // drops one topic has usually dropped its siblings, and the socket may now
    // be too quiet for a frame to trigger the throttled walk.
    ws.cleanupMessageHandlerStats('a');

    expect(ws.messageHandlersStats.a).toBeUndefined();
    expect(ws.messageHandlersStats.b.lastCallbacksCount).toBe(0);
  });
});
