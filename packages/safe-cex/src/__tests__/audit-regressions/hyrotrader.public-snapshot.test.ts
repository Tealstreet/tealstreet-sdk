import type { Timeframe } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { HyrotraderExchange } from '../../exchanges/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';

class TestHyroExchange extends HyrotraderExchange {
  getBybitRestBaseUrl() {
    return 'https://simulator.test';
  }
}
const exchanges: BybitExchange[] = [];
const setup = (hyro = true) => {
  const exchange = hyro
    ? new TestHyroExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore())
    : new BybitExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  exchanges.push(exchange);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  const socket = exchange.publicWebsocket;
  const send = vi.fn();
  socket.ws = { readyState: WebSocket.OPEN, send, close: vi.fn(), removeEventListener: vi.fn() } as any;
  const frames = () => send.mock.calls.map(([text]) => JSON.parse(text));
  const receive = (value: unknown) => socket.onMessage(new MessageEvent('message', { data: JSON.stringify(value) }));
  return { exchange, socket, frames, receive };
};
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

describe('HyroTrader periodic public snapshots', () => {
  it('keeps authoritative candle volume when publicTrade snapshots repeat', () => {
    const { socket, frames, receive } = setup();
    const callback = vi.fn();
    const unsubscribe = socket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, callback);
    const candle = { start: 1_800_000_000_000, open: '100', high: '101', low: '99', close: '100', volume: '10' };
    receive({ topic: 'kline.1.BTCUSDT', data: [candle] });
    const trade = { topic: 'publicTrade.BTCUSDT', data: [{ T: candle.start, p: '100', v: '200', S: 'Buy' }] };
    receive(trade);
    receive(trade);
    expect(callback).toHaveBeenCalledTimes(1);
    expect(callback.mock.calls[0][0].volume).toBe(10);
    expect(frames()).toEqual([{ op: 'subscribe', args: ['kline.1.BTCUSDT'] }]);
    unsubscribe();
    expect(frames().at(-1)).toEqual({ op: 'unsubscribe', args: ['kline.1.BTCUSDT'] });
  });
  it('rejects seconds candles without a supported trade stream', () => {
    const { exchange, socket, frames } = setup();
    expect(getExchangeMetadata('hyrotrader').supportsKlineSeconds).toBe(false);
    expect(() => socket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1s' as Timeframe }, vi.fn())).toThrow(
      'seconds candles',
    );
    expect(exchange.emitAndLogError).toHaveBeenCalled();
    expect(frames()).toEqual([]);
  });
  it('rejects seconds for REST and bulk subscriptions before sending anything', async () => {
    const { exchange, socket, frames } = setup();
    const get = vi.spyOn(exchange.xhr, 'get');
    await expect(exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1s' as Timeframe })).rejects.toThrow(
      'seconds candles',
    );
    expect(() => socket.listenOHLCVs({ symbols: ['BTCUSDT'], interval: '1s' as Timeframe }, vi.fn())).toThrow(
      'seconds candles',
    );
    expect(get).not.toHaveBeenCalled();
    expect(frames()).toEqual([]);
  });
  it('keeps bulk kline subscribe batches below Hyro topic limits', () => {
    const { socket, frames } = setup();
    const symbols = Array.from({ length: 33 }, (_, index) => `TEST${index}USDT`);
    socket.listenOHLCVs({ symbols, interval: '1m' }, vi.fn());
    expect(frames().map((frame) => frame.args.length)).toEqual([10, 10, 10, 3]);
  });
  it('preserves the pre-Hyro bulk kline subscribe cadence on ordinary Bybit', () => {
    // Hyro's 10-topic cap is a simulator quirk; ordinary Bybit livenet handled
    // 50-topic kline bulks fine, and shrinking that would 5x the subscribe
    // frame count on dashboards with many kline streams. Guard the split.
    const { socket, frames } = setup(false);
    const symbols = Array.from({ length: 33 }, (_, index) => `TEST${index}USDT`);
    socket.listenOHLCVs({ symbols, interval: '1m' }, vi.fn());
    expect(frames().map((frame) => frame.args.length)).toEqual([33]);
  });
  it('preserves trade-derived candles on ordinary Bybit', () => {
    const { socket, frames } = setup(false);
    const unsubscribe = socket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, vi.fn());
    expect(frames().map((frame) => frame.args)).toEqual([['kline.1.BTCUSDT'], ['publicTrade.BTCUSDT']]);
    unsubscribe();
    expect(frames().at(-1).args).toEqual(['publicTrade.BTCUSDT']);
  });
});
