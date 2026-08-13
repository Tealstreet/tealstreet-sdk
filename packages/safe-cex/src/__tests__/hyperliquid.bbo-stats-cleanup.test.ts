import { describe, expect, it, vi } from 'vitest';

import { HyperliquidPublicWebSocket } from '../exchanges/hyperliquid/hyperliquid.ws-public';

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

// A bbo frame carries no symbol at this layer: the stats recorder is handed
// `json.channel`, so every symbol shares one `bbo` entry.
const recordBboFrame = (ws: any) => ws.recordMessageHandlerStats('bbo');

describe('hyperliquid bbo stats cleanup', () => {
  it('clears the shared bbo stats entry once the last orderbook listener goes', () => {
    const ws = createSocket();

    const dispose = ws.listenOrderBook('BTC-PERP', () => undefined);
    recordBboFrame(ws);
    expect(ws.messageHandlersStats.bbo).toBeDefined();

    dispose();

    // Left behind, the entry keeps its final timestamp, ages past the staleness
    // threshold and reports Bbo dead on a healthy socket — bbo is the only
    // orderbook liveness signal, since l2Book is excluded from stats.
    expect(ws.messageHandlersStats.bbo).toBeUndefined();
  });

  it('keeps the entry while another symbol is still subscribed', () => {
    const ws = createSocket();

    const disposeBtc = ws.listenOrderBook('BTC-PERP', () => undefined);
    const disposeEth = ws.listenOrderBook('ETH-PERP', () => undefined);
    recordBboFrame(ws);

    disposeBtc();
    expect(ws.messageHandlersStats.bbo).toBeDefined();

    disposeEth();
    expect(ws.messageHandlersStats.bbo).toBeUndefined();
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
