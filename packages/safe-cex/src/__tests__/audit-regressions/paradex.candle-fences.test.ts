import type { Candle } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { DefaultStore } from '../../store/store.base';

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

it('retains newer trade candles when an old poll or an older trade bucket arrives', async () => {
  vi.useFakeTimers();
  const now = 1_710_000_120_000;
  vi.setSystemTime(now);
  const exchange = new ParadexExchange('paradex', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  let resolve!: (candles: Candle[]) => void;
  const pending = new Promise<Candle[]>((done) => { resolve = done; });
  const fetch = vi.spyOn(exchange, 'fetchOHLCV').mockReturnValueOnce(pending).mockResolvedValue([]);
  const candles: Candle[] = [];
  const stop = exchange.publicWebsocket.listenOHLCV({ symbol: 'BTC-USD-PERP', interval: '1m' }, (candle) => candles.push(candle));
  const trade = (price: number, created_at: number) => exchange.publicWebsocket.onMessage({ data: JSON.stringify({
    jsonrpc: '2.0', method: 'subscription', params: { channel: 'trades.BTC-USD-PERP',
      data: { market: 'BTC-USD-PERP', price: String(price), size: '1', created_at } },
  }) } as MessageEvent);
  trade(100, now);
  trade(50, now - 120000);
  expect(candles).toEqual([{ timestamp: now, open: 100, high: 100, low: 100, close: 100, volume: 1 }]);
  resolve([{ timestamp: now, open: 90, high: 90, low: 90, close: 90, volume: 2 }]);
  await pending;
  await Promise.resolve();
  expect(candles).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetch).toHaveBeenCalledTimes(2);
  stop();
  exchange.dispose();
});

it('rejects older buckets and decreased quantity from polls started after the latest trade', async () => {
  vi.useFakeTimers();
  const now = 1_710_000_120_000;
  vi.setSystemTime(now);
  const exchange = new ParadexExchange('paradex', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  const row = (timestamp: number, close: number, volume: number): Candle => ({ timestamp, open: close, high: close, low: close, close, volume });
  const fetch = vi.spyOn(exchange, 'fetchOHLCV')
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([row(now - 60_000, 90, 99)])
    .mockResolvedValueOnce([row(now, 90, 1)])
    .mockResolvedValueOnce([row(now, 101, 3)])
    .mockResolvedValueOnce([row(now + 60_000, 120, 1)]);
  const candles: Candle[] = [];
  const stop = exchange.publicWebsocket.listenOHLCV({ symbol: 'BTC-USD-PERP', interval: '1m' }, (candle) => candles.push(candle));
  await Promise.resolve();
  exchange.publicWebsocket.onMessage({ data: JSON.stringify({
    jsonrpc: '2.0', method: 'subscription', params: { channel: 'trades.BTC-USD-PERP',
      data: { market: 'BTC-USD-PERP', price: '100', size: '2', created_at: now } },
  }) } as MessageEvent);
  expect(candles).toEqual([row(now, 100, 2)]);
  await vi.advanceTimersByTimeAsync(3000);
  expect(candles).toEqual([row(now, 100, 2)]);
  await vi.advanceTimersByTimeAsync(3000);
  expect(candles).toEqual([row(now, 100, 2)]);
  await vi.advanceTimersByTimeAsync(3000);
  expect(candles).toEqual([row(now, 100, 2), row(now, 101, 3)]);
  await vi.advanceTimersByTimeAsync(3000);
  expect(candles).toEqual([row(now, 100, 2), row(now, 101, 3), row(now + 60_000, 120, 1)]);
  expect(fetch).toHaveBeenCalledTimes(5);
  stop();
  exchange.dispose();
});
