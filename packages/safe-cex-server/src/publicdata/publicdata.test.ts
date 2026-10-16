import type { Market } from '../../../safe-cex/src/types';
import { describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_PUBLICDATA_TICKER_CAPABILITIES,
  buildTickerSubscribeMessage,
  filterPublicdataTickerExchanges,
} from './capabilities';
import { PublicdataTickerBridge } from './PublicdataTickerBridge';
import { applyTickerDeltaForExchange, applyTickerSnapshotForExchange, buildPublicdataMarketIndex } from './tickerState';
import type { PublicMarketMetadataSource, PublicdataExchangeHealth } from './types';

const now = 1_775_766_154_600;

const connectedHealth = (
  exchange: string,
  overrides: Partial<PublicdataExchangeHealth> = {},
): PublicdataExchangeHealth => ({
  exchange,
  status: 'connected',
  tickerCount: 10,
  lastUpdate: now - 1_000,
  ...overrides,
});

const btcMarket: Market = {
  id: 'BTCUSDT',
  symbol: 'BTC/USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: {
    amount: { min: 0.001, max: 1_000 },
    leverage: { min: 1, max: 100 },
  },
};

const ethMarket: Market = {
  ...btcMarket,
  id: 'ETHUSDT',
  symbol: 'ETH/USDT',
  base: 'ETH',
};

describe('publicdata capability filtering', () => {
  it('requires an explicit requested exchange set', () => {
    const filtered = filterPublicdataTickerExchanges({
      requestedExchanges: [],
      exchangeHealth: [connectedHealth('bybit')],
      now,
      maxStalenessMs: 30_000,
    });

    expect(filtered).toEqual([]);
    expect(buildTickerSubscribeMessage(filtered)).toBeNull();
  });

  it('filters by capability allowlist, health, ticker count, and freshness', () => {
    const filtered = filterPublicdataTickerExchanges({
      requestedExchanges: ['bybit', 'hyperliquid', 'binance', 'bybit'],
      exchangeHealth: [
        connectedHealth('bybit'),
        connectedHealth('binance', { lastUpdate: now - 120_000 }),
        connectedHealth('hyperliquid'),
      ],
      now,
      maxStalenessMs: 30_000,
    });

    expect(filtered.map((item) => item.exchange)).toEqual(['bybit']);
    expect(filtered[0]?.capability.orderbook).toBe(false);
    expect(filtered[0]?.capability.liveOhlcv).toBe(false);
    expect(filtered[0]?.capability.safeCexMarketShape).toBe(false);
  });

  it('always includes a non-empty exchanges filter in subscribe messages', () => {
    const filtered = filterPublicdataTickerExchanges({
      requestedExchanges: ['bybit'],
      exchangeHealth: [connectedHealth('bybit')],
      now,
      maxStalenessMs: 30_000,
    });

    expect(buildTickerSubscribeMessage(filtered, { includeTps30s: true })).toMatchObject({
      op: 'subscribe',
      channel: 'tickers',
      exchanges: ['bybit'],
      includeTps30s: true,
    });
  });

  it('keeps the initial mapping narrow', () => {
    expect(DEFAULT_PUBLICDATA_TICKER_CAPABILITIES.map((item) => item.exchange)).toEqual(['bybit', 'binance']);
  });
});

describe('publicdata ticker state conversion', () => {
  it('converts snapshots with zero defaults and skips unknown markets', () => {
    const state = applyTickerSnapshotForExchange({
      exchange: 'bybit',
      marketIndex: buildPublicdataMarketIndex([btcMarket]),
      payload: {
        BTCUSDT: { b: 95_000, a: 95_001, l: 95_000.5, n: 17 },
        UNKNOWNUSDT: { b: 1 },
      },
      timestamp: now,
    });

    expect(state.skippedMissingMarkets).toBe(1);
    expect(state.lastSnapshotTime).toBe(now);
    expect(state.tickersByNativeSymbol.get('BTCUSDT')?.ticker).toMatchObject({
      id: 'BTCUSDT',
      symbol: 'BTC/USDT',
      bid: 95_000,
      ask: 95_001,
      last: 95_000.5,
      mark: 0,
      index: 0,
    });
    expect(state.tickersByNativeSymbol.get('BTCUSDT')?.metrics).toEqual({
      tradesPerSecond: 17,
      tradesPerSecond30s: undefined,
      tradesPerSecond60s: undefined,
      openInterestUsd: undefined,
      openInterestUsdAggregate: undefined,
      volumeUsdAggregate: undefined,
    });
  });

  it('applies sparse deltas and removed symbols', () => {
    const snapshot = applyTickerSnapshotForExchange({
      exchange: 'bybit',
      marketIndex: buildPublicdataMarketIndex([btcMarket, ethMarket]),
      payload: {
        BTCUSDT: { b: 95_000, a: 95_001, l: 95_000.5, m: 95_000.4, n: 17 },
        ETHUSDT: { b: 3_400, a: 3_401, l: 3_400.5 },
      },
    });

    const next = applyTickerDeltaForExchange({
      previous: snapshot,
      exchange: 'bybit',
      marketIndex: buildPublicdataMarketIndex([btcMarket, ethMarket]),
      payload: {
        BTCUSDT: { l: 95_010, n3: 12 },
      },
      removedSymbols: ['ETHUSDT'],
      timestamp: now + 1_000,
    });

    expect(next.tickersByNativeSymbol.get('BTCUSDT')?.ticker).toMatchObject({
      bid: 95_000,
      ask: 95_001,
      last: 95_010,
      mark: 95_000.4,
    });
    expect(next.tickersByNativeSymbol.get('BTCUSDT')?.metrics).toMatchObject({
      tradesPerSecond: 17,
      tradesPerSecond30s: 12,
    });
    expect(next.tickersByNativeSymbol.has('ETHUSDT')).toBe(false);
    expect(next.removedSymbols).toEqual(['ETHUSDT']);
    expect(next.lastDeltaTime).toBe(now + 1_000);
  });
});

describe('PublicdataTickerBridge', () => {
  it('does not create a websocket when the filtered exchange set is empty', async () => {
    const webSocketFactory = vi.fn();
    const marketSource: PublicMarketMetadataSource = {
      getMarkets: vi.fn(async () => [btcMarket]),
    };

    const bridge = new PublicdataTickerBridge({
      wsUrl: 'wss://example.test/publicdata/ws',
      exchanges: ['hyperliquid'],
      exchangeHealth: [connectedHealth('hyperliquid')],
      maxStalenessMs: 30_000,
      now: () => now,
      marketSource,
      webSocketFactory,
    });

    await expect(bridge.start()).resolves.toEqual({
      started: false,
      reason: 'empty-filter',
      exchanges: [],
    });
    expect(webSocketFactory).not.toHaveBeenCalled();
    expect(marketSource.getMarkets).not.toHaveBeenCalled();
  });
});
