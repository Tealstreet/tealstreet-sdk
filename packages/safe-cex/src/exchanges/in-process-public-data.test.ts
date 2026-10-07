import type { PublicDataSource, StoreData } from '../types';
import type { BaseExchange } from './base';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { InProcessPublicDataProvider } from './in-process-public-data';

const THRESHOLD_MS = 20_000;

const makeTicker = (symbol: string) => ({ symbol }) as unknown as StoreData['tickers'][number];
const makeMarket = (symbol: string) => ({ symbol }) as unknown as StoreData['markets'][number];

const makeSource = (seed: { tickers?: unknown[]; markets?: unknown[] } = {}) => {
  let notify: ((data: StoreData) => void) | undefined;
  const store = {
    tickers: (seed.tickers ?? []) as StoreData['tickers'],
    markets: (seed.markets ?? []) as StoreData['markets'],
    loaded: { tickers: !!seed.tickers, markets: !!seed.markets } as StoreData['loaded'],
    subscribe: (cb: (data: StoreData) => void) => {
      notify = cb;
      return () => {
        notify = undefined;
      };
    },
  };

  return {
    source: {
      store,
      listenOrderBook: () => () => {},
      listenOHLCV: () => () => {},
    } as unknown as PublicDataSource,
    push: (data: Partial<StoreData>) => notify?.(data as StoreData),
  };
};

const makeParent = () =>
  ({
    name: 'bybit',
    emitter: { emit: vi.fn() },
    adaptExternalMarkets: (markets: StoreData['markets']) => markets,
    store: {
      loaded: {},
      update: vi.fn(),
    },
  }) as unknown as BaseExchange;

describe('InProcessPublicDataProvider health surface', () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it('is uninitialized before connecting', () => {
    const { source } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);

    expect(provider.isInitialized).toBe(false);
    expect(provider.isHealthy).toBe(false);
    expect(provider.lastMessageTimestamp).toBeUndefined();
  });

  it('stays uninitialized when the source has no data yet', () => {
    const { source } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();

    expect(provider.isConnected).toBe(true);
    expect(provider.isInitialized).toBe(false);
  });

  it('initializes from the seed when the source already has data', () => {
    const { source } = makeSource({ tickers: [makeTicker('BTCUSDT')] });
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();

    expect(provider.isInitialized).toBe(true);
    expect(provider.isHealthy).toBe(true);
    expect(provider.lastMessageTimestamp).toBeTypeOf('number');
  });

  it('initializes on the first pushed update', () => {
    const { source, push } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();

    push({ tickers: [makeTicker('BTCUSDT')] as StoreData['tickers'] });

    expect(provider.isInitialized).toBe(true);
    expect(provider.isHealthy).toBe(true);
  });

  // The source mutates tickers in place and the change filter drops those
  // notifies. Liveness must be stamped anyway or a busy feed reads as stale.
  it('counts a notify the change filter drops as liveness', () => {
    const { source, push } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();

    const tickers = [makeTicker('BTCUSDT')] as StoreData['tickers'];
    push({ tickers });
    const first = provider.lastMessageTimestamp!;

    vi.useFakeTimers();
    vi.setSystemTime(first + 5_000);

    // Same array reference and length: the filter returns early, no store update.
    push({ tickers });

    expect(provider.lastMessageTimestamp).toBe(first + 5_000);
    expect(provider.isHealthy).toBe(true);
  });

  it('ignores a notify carrying no tickers or markets', () => {
    const { source, push } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();

    push({ tickers: [] as unknown as StoreData['tickers'] });

    expect(provider.isInitialized).toBe(false);
    expect(provider.lastMessageTimestamp).toBeUndefined();
  });

  it('goes unhealthy once data stops arriving, while staying initialized', () => {
    const { source, push } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();
    push({ markets: [makeMarket('BTCUSDT')] as StoreData['markets'] });
    const stamped = provider.lastMessageTimestamp!;

    vi.useFakeTimers();
    vi.setSystemTime(stamped + THRESHOLD_MS + 1);

    expect(provider.isHealthy).toBe(false);
    expect(provider.isInitialized).toBe(true);
  });

  it('is unhealthy after dispose but reports the last timestamp it saw', () => {
    const { source, push } = makeSource();
    const provider = new InProcessPublicDataProvider(makeParent(), source);
    provider.connect();
    push({ tickers: [makeTicker('BTCUSDT')] as StoreData['tickers'] });
    const stamped = provider.lastMessageTimestamp!;

    provider.dispose();

    expect(provider.isConnected).toBe(false);
    expect(provider.isHealthy).toBe(false);
    expect(provider.lastMessageTimestamp).toBe(stamped);
  });
});
