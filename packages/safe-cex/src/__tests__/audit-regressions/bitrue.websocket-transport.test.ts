import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { BASE_WS_URL, ENDPOINTS } from '../reference/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import { Market } from '../../types';

const MARKET: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1_000 }, leverage: { min: 1, max: 100 } },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  return new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
};

const websocket = () => ({ addEventListener: vi.fn() }) as any;

afterEach(() => vi.useRealTimers());

describe('audit regression: Bitrue websocket transport boundaries', () => {
  it('uses the documented futures public host on every reconnect without duplicating ticker subscriptions', () => {
    const exchange = createExchange();
    const socket = exchange.publicWebsocket;
    const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue(websocket());

    socket.connectAndSubscribe();
    socket.connectAndSubscribe(true);
    socket.connectAndSubscribe(true);

    expect(createWebsocket).toHaveBeenCalledTimes(3);
    for (const [url] of createWebsocket.mock.calls) {
      expect(url).toContain(BASE_WS_URL.public);
      expect(url).not.toContain('wss://ws.bitrue.com/market/ws');
    }
    const topic = 'market_e_btcusdt_ticker';
    expect(socket.topics[topic]).toBe(1);
    expect(socket.messageHandlers[topic]).toHaveLength(1);
  });

  it('retries transient futures listen-key failures and connects only to the futures user stream', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const socket = exchange.privateWebsocket;
    const post = vi
      .fn()
      .mockRejectedValueOnce(new Error('temporary 503'))
      .mockRejectedValueOnce(new Error('temporary 503'))
      .mockResolvedValueOnce({ data: { code: 200, data: { listenKey: 'futures-key' } } });
    exchange.xhr = { post } as any;
    const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue(websocket());

    const connected = socket.connectAndSubscribe();
    await vi.advanceTimersByTimeAsync(750);
    await connected;

    expect(post).toHaveBeenCalledTimes(3);
    expect(post).toHaveBeenNthCalledWith(1, ENDPOINTS.LISTEN_KEY);
    expect(post).toHaveBeenNthCalledWith(2, ENDPOINTS.LISTEN_KEY);
    expect(post).toHaveBeenNthCalledWith(3, ENDPOINTS.LISTEN_KEY);
    expect(createWebsocket).toHaveBeenCalledWith(`${BASE_WS_URL.private}?listenKey=futures-key`);
    expect(createWebsocket.mock.calls.flat().join(' ')).not.toContain('open.bitrue.com');
    expect(createWebsocket.mock.calls.flat().join(' ')).not.toContain('wsapi.bitrue.com');
  });

  it('fails closed after bounded futures listen-key retries instead of starting a spot socket', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const socket = exchange.privateWebsocket;
    const post = vi.fn().mockRejectedValue(new Error('futures auth unavailable'));
    const error = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = { post } as any;
    const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue(websocket());

    const connected = socket.connectAndSubscribe();
    await vi.advanceTimersByTimeAsync(2_000);
    await connected;

    expect(post).toHaveBeenCalledTimes(4);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.LISTEN_KEY);
    expect(createWebsocket).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledTimes(1);
    // One string since the socket retries through the close path's backoff (`retryFailedConnect`,
    // fix/online-recovery) instead of giving up; it still opens no socket during the quick attempts.
    expect(error.mock.calls[0][0]).toContain('was not started');
    expect(error.mock.calls[0][0]).toContain('Failed to acquire Bitrue futures listen key after 4 attempts');
  });
});
