import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitPublicWebsocket } from '../reference/bybit/bybit.ws-public';

const OHLCV_NO_DATA_TIMEOUT_MS = 25_000;

const makeSocket = () => {
  const parent = {
    isDisposed: false,
    isDemo: false,
    name: 'bybit',
    options: { testnet: false },
    emitter: { emit: vi.fn(), off: vi.fn(), on: vi.fn() },
    store: { markets: [], update: vi.fn() },
    log: vi.fn(),
    syncSocketHealthToStore: vi.fn(),
    emitWsReconnected: vi.fn(),
  } as any;
  const websocket = new BybitPublicWebsocket(parent);
  const send = vi.fn();
  websocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
  vi.spyOn(websocket, 'reconnect').mockImplementation(vi.fn());

  return { parent, send, websocket };
};

const sentJson = (send: ReturnType<typeof vi.fn>) => send.mock.calls.map(([payload]) => JSON.parse(payload));

describe('Bybit public OHLCV stream recovery', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('re-subscribes a silent kline/publicTrade pair before the chart-level stale refresh', () => {
    vi.useFakeTimers();
    const { parent, send, websocket } = makeSocket();

    websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    expect(sentJson(send)).toEqual([
      { op: 'subscribe', args: ['kline.1.BTCUSDT'] },
      { op: 'subscribe', args: ['publicTrade.BTCUSDT'] },
    ]);

    vi.advanceTimersByTime(OHLCV_NO_DATA_TIMEOUT_MS - 1);
    expect(send).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(1);

    expect(sentJson(send).slice(2)).toEqual([
      { op: 'unsubscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
      { op: 'subscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
    ]);
    expect(parent.log).toHaveBeenCalledWith(
      expect.stringContaining('Public WS OHLCV recovery re-subscribing kline.1.BTCUSDT'),
      expect.any(String),
    );
  });

  it('resets the no-data timer when a kline frame arrives', () => {
    vi.useFakeTimers();
    const { send, websocket } = makeSocket();
    const callback = vi.fn();

    websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, callback);
    vi.advanceTimersByTime(OHLCV_NO_DATA_TIMEOUT_MS - 1);

    websocket.messageHandlers['kline.1.BTCUSDT'][0]({
      data: [
        {
          start: 1_700_000_000_000,
          open: '100',
          high: '101',
          low: '99',
          close: '100.5',
          volume: '12',
        },
      ],
    });

    vi.advanceTimersByTime(OHLCV_NO_DATA_TIMEOUT_MS - 1);
    expect(send).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(1);
    expect(sentJson(send).slice(2)).toEqual([
      { op: 'unsubscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
      { op: 'subscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
    ]);
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it('does not let unseeded publicTrade frames mask a missing kline stream', () => {
    vi.useFakeTimers();
    const { send, websocket } = makeSocket();
    const callback = vi.fn();

    websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, callback);
    vi.advanceTimersByTime(OHLCV_NO_DATA_TIMEOUT_MS - 1);

    websocket.messageHandlers['publicTrade.BTCUSDT'][0]({
      data: [{ T: 1_700_000_000_000, p: '100', v: '1', S: 'Buy' }],
    });

    vi.advanceTimersByTime(1);

    expect(callback).not.toHaveBeenCalled();
    expect(sentJson(send).slice(2)).toEqual([
      { op: 'unsubscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
      { op: 'subscribe', args: ['kline.1.BTCUSDT', 'publicTrade.BTCUSDT'] },
    ]);
  });

  it('keeps shared publicTrade reconnect state while another interval still listens', () => {
    vi.useFakeTimers();
    const { send, websocket } = makeSocket();

    const stopOneMinute = websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    const stopFiveMinute = websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '5m' }, vi.fn());

    stopOneMinute();

    expect(websocket.topics['publicTrade.BTCUSDT']).toBe('publicTrade.BTCUSDT');
    expect(websocket.messageHandlers['publicTrade.BTCUSDT']).toHaveLength(1);
    expect(sentJson(send).slice(4)).toEqual([{ op: 'unsubscribe', args: ['kline.1.BTCUSDT'] }]);

    stopFiveMinute();
  });

  it('does not recover after the active OHLCV listener is cleaned up', () => {
    vi.useFakeTimers();
    const { send, websocket } = makeSocket();

    const stop = websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    stop();
    const callsAfterCleanup = send.mock.calls.length;

    vi.advanceTimersByTime(OHLCV_NO_DATA_TIMEOUT_MS);

    expect(send).toHaveBeenCalledTimes(callsAfterCleanup);
  });
});
