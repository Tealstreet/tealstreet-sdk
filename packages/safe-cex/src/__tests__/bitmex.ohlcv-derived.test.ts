import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BitmexExchange } from '../exchanges/bitmex/bitmex.exchange';
import { aggregateBitmexCandles, getBitmexOHLCVPlan } from '../exchanges/bitmex/bitmex.ohlcv-helpers';
import { DefaultStore } from '../store/store.base';
import { Candle } from '../types';

const makeFiveMinuteCandles = (startTimestamp: number, count: number): Candle[] => {
  return Array.from({ length: count }, (_, index) => {
    const open = 100 + index;
    return {
      timestamp: startTimestamp + index * 300,
      open,
      high: open + 2,
      low: open - 1,
      close: open + 1,
      volume: 10 + index,
    };
  });
};

describe('BitMEX OHLCV helpers', () => {
  it('maps 30m to 5m source candles', () => {
    const plan = getBitmexOHLCVPlan('30m');
    expect(plan).toEqual({
      sourceInterval: '5m',
      sourcePerTarget: 6,
      isNative: false,
    });
  });

  it('keeps native intervals unchanged', () => {
    const plan = getBitmexOHLCVPlan('1m');
    expect(plan).toEqual({
      sourceInterval: '1m',
      sourcePerTarget: 1,
      isNative: true,
    });
  });

  it('aggregates 5m candles into 30m buckets', () => {
    const source = makeFiveMinuteCandles(1_704_067_200, 12);
    const aggregated = aggregateBitmexCandles(source, '30m');

    expect(aggregated).toHaveLength(2);
    expect(aggregated[0]).toEqual({
      timestamp: 1_704_067_200,
      open: 100,
      high: 107,
      low: 99,
      close: 106,
      volume: 75,
    });
    expect(aggregated[1]).toEqual({
      timestamp: 1_704_069_000,
      open: 106,
      high: 113,
      low: 105,
      close: 112,
      volume: 111,
    });
  });
});

describe('BitMEX fetchOHLCV derived intervals', () => {
  let exchange: BitmexExchange;
  let store: DefaultStore;
  let fetchMock: ReturnType<typeof vi.fn>;
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    store = new DefaultStore();
    exchange = new BitmexExchange(
      'bitmex',
      {
        key: 'test-key',
        secret: 'test-secret',
        testnet: false,
        publicDataOnly: true,
      },
      store,
    );

    originalFetch = globalThis.fetch;
    fetchMock = vi.fn();
    (globalThis as unknown as { fetch: typeof globalThis.fetch }).fetch = fetchMock as typeof globalThis.fetch;
  });

  afterEach(() => {
    (globalThis as unknown as { fetch: typeof globalThis.fetch }).fetch = originalFetch;
    exchange.dispose();
    vi.restoreAllMocks();
  });

  it('builds 30m candles from 5m source endpoint data', async () => {
    const source = makeFiveMinuteCandles(1_704_067_200, 12);
    fetchMock.mockResolvedValue({
      json: async () => ({
        t: source.map((c) => c.timestamp),
        o: source.map((c) => c.open),
        h: source.map((c) => c.high),
        l: source.map((c) => c.low),
        c: source.map((c) => c.close),
        v: source.map((c) => c.volume),
      }),
    });

    const candles = await exchange.fetchOHLCV({
      symbol: 'XBTUSD',
      interval: '30m',
      limit: 2,
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const requestUrl = String(fetchMock.mock.calls[0][0]);
    expect(requestUrl).toContain('resolution=5');
    expect(candles).toHaveLength(2);
    expect(candles[0].timestamp).toBe(1_704_067_200);
    expect(candles[1].timestamp).toBe(1_704_069_000);
  });

  it('keeps native 1m fetch path unchanged', async () => {
    const source = makeFiveMinuteCandles(1_704_067_200, 3);
    fetchMock.mockResolvedValue({
      json: async () => ({
        t: source.map((c) => c.timestamp),
        o: source.map((c) => c.open),
        h: source.map((c) => c.high),
        l: source.map((c) => c.low),
        c: source.map((c) => c.close),
        v: source.map((c) => c.volume),
      }),
    });

    const candles = await exchange.fetchOHLCV({
      symbol: 'XBTUSD',
      interval: '1m',
      limit: 3,
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const requestUrl = String(fetchMock.mock.calls[0][0]);
    expect(requestUrl).toContain('resolution=1');
    expect(candles).toHaveLength(3);
    expect(candles[0]).toEqual(source[0]);
  });
});

