import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceMarketWebsocket } from './reference/binance/binance.ws-public';
import type { BinanceExchange } from './reference/binance/binance.exchange';

const OPEN = typeof WebSocket === 'undefined' ? 1 : WebSocket.OPEN;
const CONNECTING = typeof WebSocket === 'undefined' ? 0 : WebSocket.CONNECTING;

const createMarketWebsocket = () => {
  const parent = {
    name: 'binance',
    options: { testnet: false },
    isDisposed: false,
    store: {
      tickersBySymbol: {},
      updateTicker: vi.fn(),
    },
    emitter: {
      emit: vi.fn(),
    },
    log: vi.fn(),
    syncSocketHealthToStore: vi.fn(),
  } as unknown as BinanceExchange;

  const ws = new BinanceMarketWebsocket(parent);
  vi.spyOn(ws, 'send').mockImplementation(() => {});

  return ws;
};

const setReadyState = (ws: BinanceMarketWebsocket, readyState: number) => {
  (ws as any).ws = { readyState };
};

describe('BinanceMarketWebsocket listenOHLCV', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('cancels a pending kline subscription when unsubscribed before connection opens', () => {
    vi.useFakeTimers();

    const ws = createMarketWebsocket();
    setReadyState(ws, CONNECTING);

    const unsubscribe = ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    unsubscribe();

    setReadyState(ws, OPEN);
    vi.advanceTimersByTime(100);

    expect(ws.send).not.toHaveBeenCalled();
    expect(ws.messageHandlers['kline.1m.BTCUSDT']).toBeUndefined();
    expect(ws.topics['btcusdt@kline_1m']).toBeUndefined();
  });

  it('removes zero-ref kline topics after the final unsubscribe', () => {
    const ws = createMarketWebsocket();
    setReadyState(ws, OPEN);

    const unsubscribe = ws.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());

    expect(ws.topics['btcusdt@kline_1m']).toBe(1);

    unsubscribe();

    expect(ws.messageHandlers['kline.1m.BTCUSDT']).toBeUndefined();
    expect(ws.topics['btcusdt@kline_1m']).toBeUndefined();
    expect(ws.send).toHaveBeenLastCalledWith(
      expect.stringContaining('"method":"UNSUBSCRIBE","params":["btcusdt@kline_1m"]'),
    );
  });
});
