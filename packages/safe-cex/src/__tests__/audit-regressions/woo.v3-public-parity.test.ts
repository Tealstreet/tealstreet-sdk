import type {
  WooV3FuturesStats,
  WooV3KlineRow,
  WooV3MappedOrderBookSnapshot,
} from '../reference/woo/woo.v3.public';
import type { Candle, Market, OrderBook, Ticker } from '../../types';

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import {
  canonicalizeWooLegacyCandles,
  canonicalizeWooLegacyTickerStats,
  canonicalizeWooV3KlineRows,
  canonicalizeWooV3Markets,
  canonicalizeWooV3OrderBook,
  canonicalizeWooV3TickerStats,
  compareWooV3Candles,
  compareWooV3Markets,
  compareWooV3OrderBook,
  compareWooV3TickerStats,
} from '../reference/woo/woo.v3.public-parity';
import { decodeWooV3OrderBook, mapWooV3OrderBook } from '../reference/woo/woo.v3.public';
import { compareWooV3ShadowRead, selectWooV3RuntimeRoute } from '../reference/woo/woo.v3.runtime-selector';

const market = (symbol: string, overrides: Partial<Market> = {}): Market => {
  const base = symbol.replace('USDT', '');
  return {
    id: `${base}/USDT:USDT`,
    symbol,
    base,
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
    ...overrides,
  };
};

const ticker = (symbol: string, overrides: Partial<Ticker> = {}): Ticker => {
  const id = `${symbol.replace('USDT', '')}/USDT:USDT`;
  return {
    id,
    symbol,
    bid: 101,
    ask: 102,
    last: 101.5,
    index: 101.2,
    mark: 101.3,
    percentage: 2.5,
    openInterest: 50,
    fundingRate: 0.0002,
    fundingTime: 1_725_001_200_000,
    volume: 250,
    quoteVolume: 25_000,
    ...overrides,
  };
};

const futuresStats = (symbol: string, overrides: Partial<WooV3FuturesStats> = {}): WooV3FuturesStats => {
  const id = `${symbol.replace('USDT', '')}/USDT:USDT`;
  return {
    id,
    symbol,
    index: 101.2,
    mark: 101.3,
    open24h: 100,
    close24h: 102.5,
    high24h: 103,
    low24h: 99,
    percentage24h: 2.5,
    openInterest: 50,
    estimatedFundingRate: 0.0002,
    lastFundingRate: 0.0001,
    fundingTime: 1_725_001_200_000,
    volume24h: 250,
    quoteVolume24h: 25_000,
    ...overrides,
  };
};

const orderBook = (): OrderBook => ({
  bids: [
    { price: 101, amount: 2, total: 2 },
    { price: 100, amount: 3, total: 5 },
  ],
  asks: [
    { price: 102, amount: 1, total: 1 },
    { price: 103, amount: 4, total: 5 },
  ],
});

const v3OrderBook = (timestamp = 1_725_000_000_000): WooV3MappedOrderBookSnapshot => ({
  ...orderBook(),
  timestamp,
});

const candles: Candle[] = [
  { timestamp: 1_725_000_000, open: 100, high: 103, low: 99, close: 102, volume: 10 },
  { timestamp: 1_725_000_060, open: 102, high: 104, low: 101, close: 103, volume: 12 },
];

const klineRows = (): WooV3KlineRow[] =>
  candles.map((candle) => ({
    open: String(candle.open),
    close: String(candle.close),
    low: String(candle.low),
    high: String(candle.high),
    volume: String(candle.volume),
    amount: '1000',
    symbol: 'PERP_BTC_USDT',
    type: '1m',
    startTimestamp: candle.timestamp * 1000,
    endTimestamp: candle.timestamp * 1000 + 60_000,
  }));

describe('WOO V3 endpoint-specific public read parity', () => {
  it('canonicalizes markets by normalized symbol and excludes source-only info', () => {
    const btc = market('BTCUSDT', { info: { source: 'legacy' } });
    const eth = market('ETHUSDT');
    const reordered = [eth, { ...btc, info: { source: 'v3' } }];

    expect(canonicalizeWooV3Markets([btc, eth])).toBe(canonicalizeWooV3Markets(reordered));
    const comparison = compareWooV3Markets([btc, eth], reordered);
    expect(comparison).toMatchObject({ status: 'match', mismatches: [], storeAction: 'retain-legacy' });
    expect(comparison.authoritative).toBeDefined();
    expect(comparison.authoritative[0]).toBe(btc);
  });

  it('reports market value, missing, unexpected, and duplicate-key provenance without tolerances', () => {
    const legacy = [market('BTCUSDT'), market('ETHUSDT'), market('ETHUSDT')];
    const observed = [market('BTCUSDT', { precision: { amount: 0.001, price: 0.1000000001 } }), market('SOLUSDT')];

    const comparison = compareWooV3Markets(legacy, observed);
    expect(comparison.status).toBe('mismatch');
    expect(comparison.authoritative).toBe(legacy);
    expect(comparison.mismatches).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ kind: 'duplicate-key', path: 'markets[ETHUSDT]' }),
        expect.objectContaining({
          kind: 'value',
          path: 'markets[BTCUSDT].precision.price',
          legacyValue: 0.1,
          v3Value: 0.1000000001,
          provenance: { legacy: 'v1 quote_tick', v3: 'V3 quoteTick' },
        }),
        expect.objectContaining({ kind: 'missing-in-v3', path: 'markets[ETHUSDT]' }),
        expect.objectContaining({ kind: 'unexpected-in-v3', path: 'markets[SOLUSDT]' }),
      ]),
    );
  });

  it('compares shared futures statistics but always blocks REST ticker parity on quote semantics', () => {
    const legacy = [ticker('BTCUSDT')];
    const observed = [futuresStats('BTCUSDT')];
    const comparison = compareWooV3TickerStats(legacy, observed);

    expect(observed[0]!.estimatedFundingRate).not.toBe(observed[0]!.lastFundingRate);
    expect(canonicalizeWooLegacyTickerStats(legacy)).toBe(canonicalizeWooV3TickerStats(observed));
    expect(canonicalizeWooV3TickerStats(observed)).toBe(
      canonicalizeWooV3TickerStats([futuresStats('BTCUSDT', { lastFundingRate: 0.9876 })]),
    );
    expect(comparison).toMatchObject({
      status: 'blocked',
      mismatches: [],
      blockers: [
        {
          code: 'v3-futures-rest-has-no-live-quotes',
          detail: expect.stringContaining('Legacy REST fills bid/ask/last from 24h_close'),
        },
      ],
      storeAction: 'retain-legacy',
    });
    expect(comparison.authoritative).toBe(legacy);

    const changed = compareWooV3TickerStats(legacy, [futuresStats('BTCUSDT', { mark: 101.300000001 })]);
    expect(changed.status).toBe('blocked');
    expect(changed.mismatches).toContainEqual(
      expect.objectContaining({ path: 'tickers[BTCUSDT].mark', legacyValue: 101.3, v3Value: 101.300000001 }),
    );

    const changedEstimate = compareWooV3TickerStats(legacy, [
      futuresStats('BTCUSDT', { estimatedFundingRate: 0.0003, lastFundingRate: 0.0002 }),
    ]);
    expect(changedEstimate.mismatches).toContainEqual(
      expect.objectContaining({
        path: 'tickers[BTCUSDT].fundingRate',
        legacyValue: 0.0002,
        v3Value: 0.0003,
        provenance: {
          legacy: 'legacy safe-cex fundingRate from estfundingrate stream',
          v3: 'V3 estFundingRate',
        },
      }),
    );
  });

  it('compares normalized orderbook level order and cumulative totals exactly', () => {
    const legacy = orderBook();
    const observed = v3OrderBook();
    const request = { symbol: 'PERP_BTC_USDT' };
    const match = compareWooV3OrderBook({
      legacyRequest: request,
      v3Request: request,
      legacyOrderBook: legacy,
      v3OrderBook: observed,
    });
    expect(match).toMatchObject({ status: 'match', mismatches: [] });
    expect(match.authoritative).toBe(legacy);
    expect(match.observed).toBe(observed);
    expect(match.observed.timestamp).toBe(1_725_000_000_000);
    expect(canonicalizeWooV3OrderBook(request, observed)).toBe(
      canonicalizeWooV3OrderBook(request, v3OrderBook(1_725_000_000_001)),
    );

    observed.asks[1]!.total = 5.000000001;
    const changed = compareWooV3OrderBook({
      legacyRequest: request,
      v3Request: request,
      legacyOrderBook: legacy,
      v3OrderBook: observed,
    });
    expect(changed).toMatchObject({ status: 'mismatch' });
    expect(changed.mismatches).toEqual([
      expect.objectContaining({
        kind: 'value',
        path: 'orderbook.asks[1].total',
        legacyValue: 5,
        v3Value: 5.000000001,
      }),
    ]);

    const wrongTarget = compareWooV3OrderBook({
      legacyRequest: request,
      v3Request: { symbol: 'PERP_ETH_USDT' },
      legacyOrderBook: legacy,
      v3OrderBook: v3OrderBook(),
    });
    expect(wrongTarget).toMatchObject({
      status: 'blocked',
      blockers: [
        expect.objectContaining({
          code: 'request-symbol-mismatch',
          expected: 'PERP_BTC_USDT',
          actual: 'PERP_ETH_USDT',
        }),
      ],
    });
  });

  it('binds both kline routes to decoded row context and diagnoses complete timestamp coverage', () => {
    const request = { symbol: 'PERP_BTC_USDT', interval: '1m' };
    expect(canonicalizeWooLegacyCandles(request, candles)).toBe(
      canonicalizeWooV3KlineRows(request, [...klineRows()].reverse()),
    );
    for (const routeId of ['recent-klines', 'historical-klines'] as const) {
      const legacy = [...candles];
      const match = compareWooV3Candles({
        routeId,
        legacyRequest: request,
        v3Request: request,
        legacyCandles: legacy,
        v3Rows: [...klineRows()].reverse(),
      });
      expect(match).toMatchObject({ status: 'match', routeId, mismatches: [] });
      expect(match.authoritative).toBe(legacy);

      const rows = klineRows();
      const duplicateAndMissing = [rows[0]!, { ...rows[0]! }];
      const mismatch = compareWooV3Candles({
        routeId,
        legacyRequest: request,
        v3Request: request,
        legacyCandles: legacy,
        v3Rows: duplicateAndMissing,
      });
      expect(mismatch.mismatches).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ kind: 'duplicate-key', path: `candles[${candles[0]!.timestamp}]` }),
          expect.objectContaining({ kind: 'missing-in-v3', path: `candles[${candles[1]!.timestamp}]` }),
        ]),
      );
    }
  });

  it('blocks wrong V3 kline instrument, interval, and end boundary for both routes', () => {
    const request = { symbol: 'PERP_BTC_USDT', interval: '1m' };
    for (const routeId of ['recent-klines', 'historical-klines'] as const) {
      const wrongSymbol = klineRows();
      wrongSymbol[0] = { ...wrongSymbol[0]!, symbol: 'PERP_ETH_USDT' };
      const wrongInterval = klineRows();
      wrongInterval[0] = { ...wrongInterval[0]!, type: '5m' };
      const wrongEnd = klineRows();
      wrongEnd[0] = { ...wrongEnd[0]!, endTimestamp: wrongEnd[0]!.endTimestamp - 1 };

      for (const [rows, code] of [
        [wrongSymbol, 'v3-kline-row-symbol-mismatch'],
        [wrongInterval, 'v3-kline-row-interval-mismatch'],
        [wrongEnd, 'v3-kline-row-end-timestamp-mismatch'],
      ] as const) {
        const comparison = compareWooV3Candles({
          routeId,
          legacyRequest: request,
          v3Request: request,
          legacyCandles: candles,
          v3Rows: rows,
        });
        expect(comparison.status).toBe('blocked');
        expect(comparison.authoritative).toBe(candles);
        expect(comparison.blockers).toContainEqual(expect.objectContaining({ code }));
        expect(() => canonicalizeWooV3KlineRows(request, rows)).toThrow(code);
      }
    }
  });

  it('fails closed for mismatched request context and variable calendar intervals', () => {
    const wrongRequest = compareWooV3Candles({
      routeId: 'recent-klines',
      legacyRequest: { symbol: 'PERP_BTC_USDT', interval: '1m' },
      v3Request: { symbol: 'PERP_ETH_USDT', interval: '5m' },
      legacyCandles: candles,
      v3Rows: klineRows(),
    });
    expect(wrongRequest.blockers).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ code: 'request-symbol-mismatch' }),
        expect.objectContaining({ code: 'request-interval-mismatch' }),
      ]),
    );

    const monthly = compareWooV3Candles({
      routeId: 'historical-klines',
      legacyRequest: { symbol: 'PERP_BTC_USDT', interval: '1M' },
      v3Request: { symbol: 'PERP_BTC_USDT', interval: '1mon' },
      legacyCandles: candles,
      v3Rows: klineRows().map((row) => ({ ...row, type: '1mon' })),
    });
    expect(monthly.status).toBe('blocked');
    expect(monthly.blockers).toEqual(
      expect.arrayContaining([expect.objectContaining({ code: 'unsupported-kline-interval' })]),
    );
  });

  it('feeds immutable endpoint canonicals into the inert selector while preserving legacy authority', () => {
    const selection = selectWooV3RuntimeRoute('markets', 'shadow-v3');
    if (selection.status !== 'ready' || selection.requestedMode !== 'shadow-v3') {
      throw new Error('expected a public shadow selection');
    }
    const legacy = [market('BTCUSDT')];
    const observed = [market('BTCUSDT')];

    const comparison = compareWooV3ShadowRead({
      selection,
      legacySnapshot: legacy,
      legacyCanonical: canonicalizeWooV3Markets(legacy),
      v3Response: observed,
      decodeAndCanonicalizeV3: (value) => ({
        canonical: canonicalizeWooV3Markets(value as readonly Market[]),
        observed: value as readonly Market[],
      }),
    });

    expect(comparison).toMatchObject({ status: 'match', storeAction: 'retain-legacy' });
    expect(comparison.authoritative).toBe(legacy);

    const klineSelection = selectWooV3RuntimeRoute('recent-klines', 'shadow-v3');
    if (klineSelection.status !== 'ready' || klineSelection.requestedMode !== 'shadow-v3') {
      throw new Error('expected a kline shadow selection');
    }
    const request = { symbol: 'PERP_BTC_USDT', interval: '1m' };
    const wrongRows = klineRows();
    wrongRows[0] = { ...wrongRows[0]!, symbol: 'PERP_ETH_USDT' };
    const rejected = compareWooV3ShadowRead({
      selection: klineSelection,
      legacySnapshot: candles,
      legacyCanonical: canonicalizeWooLegacyCandles(request, candles),
      v3Response: wrongRows,
      decodeAndCanonicalizeV3: (value) => ({
        canonical: canonicalizeWooV3KlineRows(request, value as WooV3KlineRow[]),
        observed: value as WooV3KlineRow[],
      }),
    });
    expect(rejected).toMatchObject({
      status: 'decode-or-map-failed',
      error: expect.objectContaining({ message: expect.stringContaining('v3-kline-row-symbol-mismatch') }),
      storeAction: 'retain-legacy',
    });
    expect(rejected.authoritative).toBe(candles);
  });

  it('retains the mapped orderbook timestamp through the highest generic shadow boundary', () => {
    const selection = selectWooV3RuntimeRoute('orderbook', 'shadow-v3');
    if (selection.status !== 'ready' || selection.requestedMode !== 'shadow-v3') {
      throw new Error('expected an orderbook shadow selection');
    }
    const request = { symbol: 'PERP_BTC_USDT' };
    const legacy = orderBook();
    const raw = {
      success: true,
      timestamp: 1_725_000_000_000,
      data: {
        bids: [
          { price: '101', quantity: '2' },
          { price: '100', quantity: '3' },
        ],
        asks: [
          { price: '102', quantity: '1' },
          { price: '103', quantity: '4' },
        ],
      },
    };

    const comparison = compareWooV3ShadowRead({
      selection,
      legacySnapshot: legacy,
      legacyCanonical: canonicalizeWooV3OrderBook(request, { ...legacy, timestamp: raw.timestamp }),
      v3Response: raw,
      decodeAndCanonicalizeV3: (value) => {
        const observed = mapWooV3OrderBook(decodeWooV3OrderBook(value));
        return { canonical: canonicalizeWooV3OrderBook(request, observed), observed };
      },
    });

    expect(comparison).toMatchObject({ status: 'match', storeAction: 'retain-legacy' });
    expect(comparison.status === 'match' && comparison.observed.timestamp).toBe(raw.timestamp);
  });

  it('remains transport-free, disconnected, and behind all eight hard-false gates', () => {
    const wooDirectory = join(__dirname, '../reference/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.public-parity');
    }

    const source = readFileSync(join(wooDirectory, 'woo.v3.public-parity.ts'), 'utf8');
    expect(source).not.toMatch(/from ['"](?:axios|ws|@tealstreet\/reconnecting-websocket)['"]/);
    expect(source).not.toMatch(/\bnew WebSocket\b|\bfetch\s*\(|\.request\s*\(|\.store\b/);
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
  });
});
