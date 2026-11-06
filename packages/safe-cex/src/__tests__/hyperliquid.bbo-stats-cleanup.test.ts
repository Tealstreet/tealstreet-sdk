import { describe, expect, it, vi } from 'vitest';

import { HyperliquidPublicWebSocket } from './reference/hyperliquid/hyperliquid.ws-public';

const market = (symbol: string, id: string) => ({ symbol, id, precision: { price: 0.1, amount: 0.001 } });

const createMockParent = () =>
  ({
    name: 'hyperliquid',
    options: { testnet: false, publicDataOnly: true, key: '' },
    isDisposed: false,
    log: vi.fn(),
    emitAndLogError: vi.fn(),
    emitter: { emit: vi.fn(), on: vi.fn(), off: vi.fn() },
    syncSocketHealthToStore: vi.fn(),
    store: { markets: [market('BTC-PERP', 'BTC'), market('ETH-PERP', 'ETH')], update: vi.fn() },
  }) as any;

// Drives the real subscribe/teardown path with the socket faked as connected,
// so the assertions are about lifecycle bookkeeping rather than transport.
const createSocket = () => {
  const ws = new HyperliquidPublicWebSocket(createMockParent()) as any;
  vi.spyOn(ws, 'createWebsocket').mockReturnValue({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  });
  ws.send = vi.fn();
  Object.defineProperty(ws, 'isConnected', { get: () => true, configurable: true });
  return ws;
};

// Drives a real bbo frame through onMessage so the stats key comes from the
// dispatch site, not the test. It is `bbo.{coin}` — the exact `messageHandlers`
// key — which is what lets base.ws.ts reconcile it. The bare `bbo` key it
// replaced was shared by every symbol and matched no handler key, so nothing
// could ever mark it idle.
const recordBboFrame = (ws: any, coin: string) =>
  ws.onMessage({ data: JSON.stringify({ channel: 'bbo', data: { coin, bbo: [] } }) });

// The reconcile walk is throttled — including the forced path, which has its own
// 250ms floor — so teardown does not zero the entry on the spot. Drive it with a
// later timestamp rather than sleeping.
const forceReconcile = (ws: any) => ws.reconcileMessageHandlerStats(Date.now() + 10_000, true);

describe('hyperliquid bbo stats', () => {
  it('stops counting bbo as a live feed once the last orderbook listener goes', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('BTC-PERP', () => undefined);
    recordBboFrame(ws, 'BTC');
    expect(ws.messageHandlersStats['bbo.BTC'].lastCallbacksCount).toBe(1);
    // The shared bare key is no longer recorded at all.
    expect(ws.messageHandlersStats.bbo).toBeUndefined();

    dispose();
    forceReconcile(ws);

    // Left counted, the entry keeps its final timestamp, ages past the staleness
    // threshold and reports Bbo dead on a healthy socket — bbo is the only
    // orderbook liveness signal, since l2Book is not stats-tracked.
    expect(ws.messageHandlersStats['bbo.BTC'].lastCallbacksCount).toBe(0);
  });

  it('tracks each symbol independently', () => {
    const ws = createSocket();

    const disposeBtc = ws.listenOrderBook('BTC-PERP', () => undefined);
    ws.listenOrderBook('ETH-PERP', () => undefined);
    recordBboFrame(ws, 'BTC');
    recordBboFrame(ws, 'ETH');

    disposeBtc();
    forceReconcile(ws);

    // Per-symbol keys are the point: one symbol going away must not silence the
    // other, and the other staying must not keep the first one counted.
    expect(ws.messageHandlersStats['bbo.BTC'].lastCallbacksCount).toBe(0);
    expect(ws.messageHandlersStats['bbo.ETH'].lastCallbacksCount).toBe(1);
  });

  // The cleanup this replaced. l2Book is excluded from stats at record time, so
  // clearing `l2Book.<symbol>` could never have released anything.
  it('never records a stats entry for l2Book in the first place', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('BTC-PERP', () => undefined);
    ws.onMessage({ data: JSON.stringify({ channel: 'l2Book', data: { coin: 'BTC', levels: [[], []], time: 1 } }) });

    expect(ws.messageHandlersStats['l2Book.BTC']).toBeUndefined();
    expect(ws.messageHandlersStats.l2Book).toBeUndefined();

    dispose();
  });
});
