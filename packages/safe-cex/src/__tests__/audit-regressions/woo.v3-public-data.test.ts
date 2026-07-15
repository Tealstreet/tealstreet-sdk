import type { Market } from '../../types';

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../../exchanges/woo/woo.v3.contracts';
import {
  decodeWooV3Futures,
  decodeWooV3Instruments,
  decodeWooV3Klines,
  decodeWooV3OrderBook,
  mapWooV3Candles,
  mapWooV3FuturesStats,
  mapWooV3Markets,
  mapWooV3OrderBook,
} from '../../exchanges/woo/woo.v3.public';

const envelope = <T>(data: T) => ({ success: true, timestamp: 1_725_000_000_000, data });

const instrumentFixture = envelope({
  rows: [
    {
      symbol: 'PERP_BTC_USDT',
      baseAsset: 'BTC',
      baseAssetMultiplier: '1',
      quoteAsset: 'USDT',
      quoteTick: '0.1',
      baseMin: '0.001',
      baseMax: '100',
      baseTick: '0.001',
      minNotional: '1',
      fundingIntervalHours: 8,
      baseIMR: '0.02',
      isAllowedRpi: true,
    },
    {
      symbol: 'SPOT_ETH_USDT',
      status: 'TRADING',
      baseAsset: 'ETH',
      quoteAsset: 'USDT',
      quoteTick: 0.01,
      baseMin: 0.001,
      baseMax: 1000,
      baseTick: 0.001,
      minNotional: 1,
      baseIMR: 0.1,
    },
  ],
});

const futuresFixture = envelope({
  rows: [
    {
      symbol: 'PERP_BTC_USDT',
      indexPrice: '62010.5',
      markPrice: '62011.2',
      estFundingRate: '-0.0001',
      lastFundingRate: '0.0002',
      openInterest: '120.5',
      '24hOpen': '60000',
      '24hClose': '61800',
      '24hHigh': '63000',
      '24hLow': '59000',
      '24hVolume': '250.25',
      '24hAmount': '15200000.5',
      nextFundingTime: 1_725_001_200_000,
    },
  ],
});

const orderBookFixture = envelope({
  // WOO's V3 page documents both sides as descending.
  asks: [
    { price: '62003', quantity: '2' },
    { price: '62002', quantity: '1.5' },
  ],
  bids: [
    { price: '62001', quantity: '1' },
    { price: '62000', quantity: '3' },
  ],
});

const klineFixture = envelope({
  rows: [
    {
      open: '62000',
      close: '62005',
      low: '61990',
      high: '62010',
      volume: '12.5',
      amount: '775031.25',
      symbol: 'PERP_BTC_USDT',
      type: '1m',
      startTimestamp: 1_725_000_060_000,
      endTimestamp: 1_725_000_119_999,
    },
    {
      open: '61990',
      close: '62000',
      low: '61980',
      high: '62001',
      volume: '10',
      amount: '619950',
      symbol: 'PERP_BTC_USDT',
      type: '1m',
      startTimestamp: 1_725_000_000_000,
      endTimestamp: 1_725_000_059_999,
    },
  ],
});

describe('WooX namespaced V3 public REST parity foundation', () => {
  it('decodes official instruments and maps only perpetual markets without requiring the drift-prone status field', () => {
    const rows = decodeWooV3Instruments(instrumentFixture);
    const markets = mapWooV3Markets(rows);

    expect(markets).toHaveLength(1);
    expect(markets[0]).toMatchObject({
      id: 'BTC/USDT:USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
    });
  });

  it('keeps futures REST data as statistics rather than fabricating Ticker bid, ask, or last fields', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0];
    const stats = mapWooV3FuturesStats(decodeWooV3Futures(futuresFixture), {
      [market.symbol]: market,
    });

    expect(stats).toEqual([
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        index: 62010.5,
        mark: 62011.2,
        open24h: 60000,
        close24h: 61800,
        high24h: 63000,
        low24h: 59000,
        percentage24h: 3,
        openInterest: 120.5,
        estimatedFundingRate: -0.0001,
        lastFundingRate: 0.0002,
        fundingTime: 1_725_001_200_000,
        volume24h: 250.25,
        quoteVolume24h: 15200000.5,
      },
    ]);
    expect(stats[0]).not.toHaveProperty('bid');
    expect(stats[0]).not.toHaveProperty('ask');
    expect(stats[0]).not.toHaveProperty('last');
  });

  it('normalizes the documented descending snapshot into safe-cex side order and cumulative totals', () => {
    const orderBook = mapWooV3OrderBook(decodeWooV3OrderBook(orderBookFixture));

    expect(orderBook).toEqual({
      bids: [
        { price: 62001, amount: 1, total: 1 },
        { price: 62000, amount: 3, total: 4 },
      ],
      asks: [
        { price: 62002, amount: 1.5, total: 1.5 },
        { price: 62003, amount: 2, total: 3.5 },
      ],
    });
  });

  it('maps both current and historical kline rows into chronological seconds-based candles', () => {
    expect(mapWooV3Candles(decodeWooV3Klines(klineFixture))).toEqual([
      { timestamp: 1_725_000_000, open: 61990, high: 62001, low: 61980, close: 62000, volume: 10 },
      { timestamp: 1_725_000_060, open: 62000, high: 62010, low: 61990, close: 62005, volume: 12.5 },
    ]);
  });

  it('fails closed on malformed envelopes, non-string V3 decimal stats, and invalid book levels', () => {
    expect(() => decodeWooV3Futures({ ...futuresFixture, success: false })).toThrow('success=false');
    expect(() =>
      decodeWooV3Futures(envelope({ rows: [{ ...(futuresFixture.data.rows[0] as object), markPrice: 62011.2 }] })),
    ).toThrow('markPrice must be a finite decimal string');
    expect(() => decodeWooV3OrderBook(envelope({ asks: [], bids: [{ price: '62000', quantity: 'NaN' }] }))).toThrow(
      'quantity must be a finite decimal string',
    );
    expect(() => mapWooV3OrderBook({ asks: [], bids: [{ price: '62000', quantity: '-1' }] })).toThrow(
      'quantity must be at least',
    );
  });

  it('preserves all hard-false migration gates and has no import from active Woo runtime entrypoints', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false]);

    const runtimeFiles = ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts'];
    for (const runtimeFile of runtimeFiles) {
      const source = readFileSync(join(process.cwd(), 'src/exchanges/woo', runtimeFile), 'utf8');
      expect(source).not.toContain('woo.v3.public');
    }
  });

  it('does not accidentally make the incomplete futures statistics assignable as a safe-cex market quote', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0] satisfies Market;
    expect(market.symbol).toBe('BTCUSDT');
  });
});
