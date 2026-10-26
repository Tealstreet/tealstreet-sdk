import type { Market } from '../../types';

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import {
  decodeWooV3Futures,
  decodeWooV3Instruments,
  decodeWooV3Klines,
  decodeWooV3OrderBook,
  mapWooV3Candles,
  mapWooV3FuturesStats,
  mapWooV3Markets,
  mapWooV3OrderBook,
} from '../reference/woo/woo.v3.public';
import { composeWooV3AuthoritativeTicker } from '../reference/woo/woo.v3.ticker';
import type { WooV3WsBbo, WooV3WsTrade } from '../reference/woo/woo.v3.websocket';
import { decodeWooV3WsDataFrame } from '../reference/woo/woo.v3.websocket';

const envelope = <T>(data: T) => ({ success: true, timestamp: 1_725_000_000_000, data });
const publicFrame = (topic: string, data: unknown) => ({ topic, ts: 1_725_000_000_000, data });

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

  it('keeps futures REST data as statistics rather than fabricating quote fields from 24hClose', () => {
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

  it('composes live quotes and selects estimated funding while retaining the last settled rate as provenance', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0];
    const futuresRow = decodeWooV3Futures(futuresFixture)[0];
    const bbo = decodeWooV3WsDataFrame(
      publicFrame('bbo@PERP_BTC_USDT', {
        s: 'PERP_BTC_USDT',
        ap: '62002.2',
        aq: '1.05',
        bp: '62001.1',
        bq: '2.01',
        ts: 1_725_000_000_010,
      }),
    ).payload as WooV3WsBbo;
    const trade = decodeWooV3WsDataFrame(
      publicFrame('trade@PERP_BTC_USDT', {
        s: 'PERP_BTC_USDT',
        px: '62001.7',
        sx: '0.25',
        sd: 'BUY',
        src: 1,
        rpi: false,
        ts: 1_725_000_000_011,
      }),
    ).payload as WooV3WsTrade;

    const ticker = composeWooV3AuthoritativeTicker(futuresRow, bbo, trade, market);

    expect(ticker).toMatchObject({
      id: 'BTC/USDT:USDT',
      symbol: 'BTCUSDT',
      bid: 62001.1,
      ask: 62002.2,
      last: 62001.7,
      index: 62010.5,
      mark: 62011.2,
      percentage: 3,
      openInterest: 120.5,
      fundingRate: -0.0001,
      fundingTime: 1_725_001_200_000,
      volume: 250.25,
      quoteVolume: 15200000.5,
    });
    expect(ticker.last).not.toBe(Number(futuresRow['24hClose']));
    expect(ticker.info.futuresStats).toMatchObject({
      open24h: 60000,
      close24h: 61800,
      high24h: 63000,
      low24h: 59000,
      estimatedFundingRate: -0.0001,
      lastFundingRate: 0.0002,
    });
    expect(ticker.info.bbo).toBe(bbo);
    expect(ticker.info.trade).toBe(trade);

    const zeroEstimatedRow = decodeWooV3Futures(
      envelope({
        rows: [{ ...futuresFixture.data.rows[0], estFundingRate: '0', lastFundingRate: '0.0002' }],
      }),
    )[0];
    const zeroEstimatedTicker = composeWooV3AuthoritativeTicker(zeroEstimatedRow, bbo, trade, market);
    expect(zeroEstimatedTicker.fundingRate).toBe(0);
    expect(zeroEstimatedTicker.info.futuresStats.lastFundingRate).toBe(0.0002);
  });

  it('rejects mismatched product/symbol identities instead of cross-composing sources', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0];
    const futuresRow = decodeWooV3Futures(futuresFixture)[0];
    const trade = decodeWooV3WsDataFrame(
      publicFrame('trade@PERP_BTC_USDT', {
        s: 'PERP_BTC_USDT',
        px: '62001.7',
        sx: '0.25',
        sd: 'SELL',
        src: 0,
        rpi: false,
        ts: 1,
      }),
    ).payload as WooV3WsTrade;
    const ethBbo = decodeWooV3WsDataFrame(
      publicFrame('bbo@PERP_ETH_USDT', {
        s: 'PERP_ETH_USDT',
        ap: '3501',
        aq: '1',
        bp: '3500',
        bq: '1',
        ts: 1,
      }),
    ).payload as WooV3WsBbo;
    const ethTrade = decodeWooV3WsDataFrame(
      publicFrame('trade@PERP_ETH_USDT', {
        s: 'PERP_ETH_USDT',
        px: '3500.5',
        sx: '0.5',
        sd: 'BUY',
        src: 1,
        rpi: false,
        ts: 1,
      }),
    ).payload as WooV3WsTrade;
    const spotBbo = decodeWooV3WsDataFrame(
      publicFrame('bbo@SPOT_BTC_USDT', {
        s: 'SPOT_BTC_USDT',
        ap: '62002',
        aq: '1',
        bp: '62001',
        bq: '1',
        ts: 1,
      }),
    ).payload as WooV3WsBbo;

    expect(() => composeWooV3AuthoritativeTicker(futuresRow, ethBbo, trade, market)).toThrow(
      'matching normalized symbol identities',
    );
    expect(() => composeWooV3AuthoritativeTicker(futuresRow, ethBbo, ethTrade, market)).toThrow(
      'matching normalized symbol identities',
    );
    expect(() => composeWooV3AuthoritativeTicker(futuresRow, spotBbo, trade, market)).toThrow(
      'PERP_BASE_QUOTE',
    );
    expect(() => composeWooV3AuthoritativeTicker(futuresRow, ethBbo, ethTrade, { ...market, symbol: 'ETHUSDT' })).toThrow(
      'matching normalized symbol identities',
    );
  });

  it('rejects malformed, non-finite, underflowed, and non-positive quote or trade prices', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0];
    const futuresRow = decodeWooV3Futures(futuresFixture)[0];
    const bboFrame = (bidPrice: string, askPrice = '62002') =>
      publicFrame('bbo@PERP_BTC_USDT', {
        s: 'PERP_BTC_USDT',
        ap: askPrice,
        aq: '1',
        bp: bidPrice,
        bq: '1',
        ts: 1,
      });
    const tradeFrame = (price: string) =>
      publicFrame('trade@PERP_BTC_USDT', {
        s: 'PERP_BTC_USDT',
        px: price,
        sx: '1',
        sd: 'BUY',
        src: 1,
        rpi: false,
        ts: 1,
      });
    const validBbo = decodeWooV3WsDataFrame(bboFrame('62001')).payload as WooV3WsBbo;
    const validTrade = decodeWooV3WsDataFrame(tradeFrame('62001.5')).payload as WooV3WsTrade;

    expect(() => decodeWooV3WsDataFrame(bboFrame('NaN'))).toThrow('finite decimal string');
    expect(() => decodeWooV3WsDataFrame(tradeFrame('0'))).toThrow('positive decimal');

    const underflow = `0.${'0'.repeat(400)}1`;
    const underflowBbo = decodeWooV3WsDataFrame(bboFrame(underflow)).payload as WooV3WsBbo;
    const overflowTrade = decodeWooV3WsDataFrame(tradeFrame('9'.repeat(400))).payload as WooV3WsTrade;
    expect(() => composeWooV3AuthoritativeTicker(futuresRow, underflowBbo, validTrade, market)).toThrow(
      'positive finite price',
    );
    expect(() => composeWooV3AuthoritativeTicker(futuresRow, validBbo, overflowTrade, market)).toThrow(
      'positive finite price',
    );
    expect(() =>
      composeWooV3AuthoritativeTicker(futuresRow, { ...validBbo, askPrice: '-1' }, validTrade, market),
    ).toThrow('positive finite price');
  });

  it('normalizes the documented descending snapshot into safe-cex side order and cumulative totals', () => {
    const snapshot = decodeWooV3OrderBook(orderBookFixture);
    const orderBook = mapWooV3OrderBook(snapshot);

    expect(snapshot.timestamp).toBe(1_725_000_000_000);

    expect(orderBook).toEqual({
      timestamp: 1_725_000_000_000,
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
    expect(() =>
      decodeWooV3Futures(
        envelope({ rows: [{ ...(futuresFixture.data.rows[0] as object), estFundingRate: 'not-a-rate' }] }),
      ),
    ).toThrow('estFundingRate must be a finite decimal string');
    expect(() =>
      decodeWooV3Futures(
        envelope({ rows: [{ ...(futuresFixture.data.rows[0] as object), estFundingRate: undefined }] }),
      ),
    ).toThrow('estFundingRate must be a finite decimal string');
    expect(() =>
      decodeWooV3Futures(
        envelope({ rows: [{ ...(futuresFixture.data.rows[0] as object), lastFundingRate: 'not-a-rate' }] }),
      ),
    ).toThrow('lastFundingRate must be a finite decimal string');
    expect(() => decodeWooV3OrderBook(envelope({ asks: [], bids: [{ price: '62000', quantity: 'NaN' }] }))).toThrow(
      'quantity must be a finite decimal string',
    );
    expect(() =>
      mapWooV3OrderBook({ timestamp: 1_725_000_000_000, asks: [], bids: [{ price: '62000', quantity: '-1' }] }),
    ).toThrow('quantity must be at least');
  });

  it.each([
    ['missing', undefined],
    ['string', '1725000000000'],
    ['zero', 0],
    ['negative', -1],
    ['fractional', 1_725_000_000_000.5],
    ['not finite', Number.POSITIVE_INFINITY],
    ['unsafe', Number.MAX_SAFE_INTEGER + 1],
  ])('rejects a %s top-level orderbook timestamp', (_name, timestamp) => {
    expect(() => decodeWooV3OrderBook({ ...orderBookFixture, timestamp })).toThrow(
      'orderbook.timestamp must be a positive safe integer millisecond timestamp',
    );
  });

  it('preserves all hard-false migration gates and has no import from active Woo runtime entrypoints', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);

    const runtimeFiles = ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts'];
    for (const runtimeFile of runtimeFiles) {
      const source = readFileSync(join(process.cwd(), 'src/exchanges/woo', runtimeFile), 'utf8');
      expect(source).not.toContain('woo.v3.public');
      expect(source).not.toContain('woo.v3.ticker');
    }
  });

  it('does not accidentally make the incomplete futures statistics assignable as a safe-cex market quote', () => {
    const market = mapWooV3Markets(decodeWooV3Instruments(instrumentFixture))[0] satisfies Market;
    expect(market.symbol).toBe('BTCUSDT');
  });
});
