import { describe, expect, it } from 'vitest';

import { CoinbaseStarbaseProtocolError } from '../../exchanges/coinbase/coinbase.starbase';
import {
  assessCoinbaseStarbaseMarketParity,
  buildCoinbaseStarbaseBookChannel,
  buildCoinbaseStarbaseChartChannel,
  buildCoinbaseStarbaseChartData,
  buildCoinbaseStarbaseGetInstruments,
  buildCoinbaseStarbaseInstrumentStateChannel,
  buildCoinbaseStarbaseOrderBook,
  buildCoinbaseStarbaseTicker,
  buildCoinbaseStarbaseTickerChannel,
  CoinbaseStarbasePublicSubscriptions,
  CoinbaseStarbaseSequencedOrderBook,
  normalizeCoinbaseStarbaseChartData,
  normalizeCoinbaseStarbaseChartPayload,
  normalizeCoinbaseStarbaseInstrument,
  normalizeCoinbaseStarbaseInstruments,
  normalizeCoinbaseStarbaseOrderBookSnapshot,
  normalizeCoinbaseStarbaseTicker,
  normalizeCoinbaseStarbaseTickerPayload,
  toCoinbaseLegacyIntxSymbol,
  toCoinbaseStarbaseInstrumentName,
} from '../../exchanges/coinbase/coinbase.starbase-public';
import {
  STARBASE_BOOK_CHANGE_PAYLOAD,
  STARBASE_BOOK_SNAPSHOT_PAYLOAD,
  STARBASE_CHART_DATA_FIXTURE,
  STARBASE_CHART_PAYLOAD_FIXTURE,
  STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
  STARBASE_PUBLIC_ORDER_BOOK_FIXTURE,
  STARBASE_PUBLIC_TICKER_FIXTURE,
} from './coinbase.starbase-public.fixtures';

describe('Coinbase Starbase inert markets and public data', () => {
  it('maps saved INTX perpetual symbols bidirectionally without changing their persisted identity', () => {
    expect(toCoinbaseStarbaseInstrumentName('BTC-PERP-INTX')).toBe('BTC_USDC-PERPETUAL');
    expect(toCoinbaseStarbaseInstrumentName('BTC_USDC-PERPETUAL')).toBe('BTC_USDC-PERPETUAL');
    expect(toCoinbaseLegacyIntxSymbol('ETH_USDC-PERPETUAL')).toBe('ETH-PERP-INTX');

    expect(() => toCoinbaseStarbaseInstrumentName('BTC-USD')).toThrow(CoinbaseStarbaseProtocolError);
    expect(() => toCoinbaseStarbaseInstrumentName('SOL-PERP-INTX')).toThrow(CoinbaseStarbaseProtocolError);
  });

  it('normalizes only published linear USDC perpetual fields and preserves base sizing metadata', () => {
    const instrument = normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_INSTRUMENT_FIXTURE);
    expect(instrument).toEqual({
      base: 'BTC',
      contractSize: 1,
      createdAtMs: 1_536_569_522_277,
      expiresAtMs: 4_102_444_800_000,
      instrumentName: 'BTC_USDC-PERPETUAL',
      instrumentType: 'linear',
      isActive: true,
      kind: 'future',
      legacySavedSymbol: 'BTC-PERP-INTX',
      maximumLeverage: 50,
      minimumBaseAmount: 0.001,
      priceIncrement: 0.5,
      priceIncrementSteps: [{ abovePrice: 100_000, priceIncrement: 1 }],
      priceIndex: 'btc_usdc',
      quote: 'USDC',
      settlementPeriod: 'perpetual',
      state: 'open',
    });
    expect(assessCoinbaseStarbaseMarketParity(instrument)).toEqual({
      status: 'blocked',
      gateway: instrument,
      blockers: ['amount-increment-unpublished', 'maximum-base-amount-unpublished'],
    });
  });

  it('fails closed on unknown instrument enums, missing sizing mode, and duplicate names', () => {
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, kind: 'option' }),
    ).toThrow('Unsupported Starbase instrument kind');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, state: 'trading' }),
    ).toThrow('unknown book state');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, instrument_type: undefined }),
    ).toThrow('instrument_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstruments([STARBASE_PUBLIC_INSTRUMENT_FIXTURE, STARBASE_PUBLIC_INSTRUMENT_FIXTURE]),
    ).toThrow('Duplicate Starbase instrument_name');
  });

  it('preserves ticker amount units while naming published base/USD statistics explicitly', () => {
    expect(normalizeCoinbaseStarbaseTicker(STARBASE_PUBLIC_TICKER_FIXTURE)).toEqual({
      ask: 50_010,
      askGatewayAmount: 100_020,
      bid: 50_000,
      bidGatewayAmount: 150_000,
      fundingRate: 0.000_001,
      fundingRate8h: 0.000_008,
      index: 50_004,
      instrumentName: 'BTC_USDC-PERPETUAL',
      last: 50_005,
      legacySavedSymbol: 'BTC-PERP-INTX',
      mark: 50_003,
      openInterestUsd: 25_000_000,
      percentage: 2.5,
      state: 'open',
      timestampMs: 1_800_000_000_000,
      volumeBase: 1_200,
      volumeUsd: 60_000_000,
    });
    expect(normalizeCoinbaseStarbaseTickerPayload({ data: STARBASE_PUBLIC_TICKER_FIXTURE }).bidGatewayAmount).toBe(
      150_000,
    );
  });

  it('rejects malformed ticker numbers and unknown book states', () => {
    expect(() =>
      normalizeCoinbaseStarbaseTicker({ ...STARBASE_PUBLIC_TICKER_FIXTURE, best_bid_price: '50000' }),
    ).toThrow('best_bid_price');
    expect(() => normalizeCoinbaseStarbaseTicker({ ...STARBASE_PUBLIC_TICKER_FIXTURE, state: 'TRADING' })).toThrow(
      'unknown book state',
    );
  });

  it('maps REST order-book USD notional levels to sorted base amounts and cumulative totals', () => {
    expect(normalizeCoinbaseStarbaseOrderBookSnapshot(STARBASE_PUBLIC_ORDER_BOOK_FIXTURE)).toEqual({
      asks: [
        { amount: 2, price: 50_010, total: 2 },
        { amount: 1, price: 50_020, total: 3 },
      ],
      bids: [
        { amount: 2, price: 50_000, total: 2 },
        { amount: 1, price: 49_990, total: 3 },
      ],
    });
  });

  it('applies published book snapshot/delta sequences and requires recovery on a gap or reconnect', () => {
    const book = new CoinbaseStarbaseSequencedOrderBook();

    expect(book.applyPayload(STARBASE_BOOK_SNAPSHOT_PAYLOAD)).toEqual({
      frame: 'snapshot',
      sequence: '297217',
      status: 'applied',
    });
    expect(book.orderBook).toEqual({
      asks: [{ amount: 2, price: 50_010, total: 2 }],
      bids: [{ amount: 2, price: 50_000, total: 2 }],
    });

    expect(book.applyPayload(STARBASE_BOOK_CHANGE_PAYLOAD)).toEqual({
      frame: 'delta',
      sequence: '297218',
      status: 'applied',
    });
    expect(book.orderBook).toEqual({ asks: [{ amount: 1, price: 50_010, total: 1 }], bids: [] });

    expect(
      book.applyPayload({
        data: {
          ...STARBASE_BOOK_CHANGE_PAYLOAD.data,
          bids: [],
          change_id: 297_220,
          prev_change_id: 297_219,
        },
      }),
    ).toEqual({ reason: 'sequence-gap', status: 'recover' });
    expect(book.orderBook).toEqual({ asks: [], bids: [] });

    book.onReconnect();
    expect(book.applyPayload(STARBASE_BOOK_CHANGE_PAYLOAD)).toEqual({
      reason: 'missing-snapshot',
      status: 'recover',
    });
  });

  it('rejects unknown book actions and non-zero deletes before mutating the book', () => {
    const book = new CoinbaseStarbaseSequencedOrderBook();
    expect(() =>
      book.applyPayload({
        data: { ...STARBASE_BOOK_SNAPSHOT_PAYLOAD.data, asks: [['replace', 50_010, 100_020]] },
      }),
    ).toThrow('action');
    expect(() =>
      book.applyPayload({
        data: { ...STARBASE_BOOK_CHANGE_PAYLOAD.data, bids: [['delete', 50_000, 1]] },
      }),
    ).toThrow('delete amount');
  });

  it('recovers atomically when new/change/delete contradict the published price-level lifecycle', () => {
    const book = new CoinbaseStarbaseSequencedOrderBook();
    book.applyPayload(STARBASE_BOOK_SNAPSHOT_PAYLOAD);

    expect(
      book.applyPayload({
        data: {
          ...STARBASE_BOOK_CHANGE_PAYLOAD.data,
          asks: [
            ['change', 50_010, 50_010],
            ['change', 50_020, 50_020],
          ],
          bids: [],
        },
      }),
    ).toEqual({ reason: 'invalid-level', status: 'recover' });
    expect(book.orderBook).toEqual({ asks: [], bids: [] });
    expect(book.applyPayload(STARBASE_BOOK_CHANGE_PAYLOAD)).toEqual({
      reason: 'awaiting-snapshot',
      status: 'ignored',
    });

    expect(book.applyPayload(STARBASE_BOOK_SNAPSHOT_PAYLOAD).status).toBe('applied');
    expect(
      book.applyPayload({
        data: { ...STARBASE_BOOK_CHANGE_PAYLOAD.data, asks: [['new', 50_010, 50_010]], bids: [] },
      }),
    ).toEqual({ reason: 'invalid-level', status: 'recover' });
  });

  it('maps REST and streaming chart schemas to safe-cex candles in seconds', () => {
    expect(normalizeCoinbaseStarbaseChartData(STARBASE_CHART_DATA_FIXTURE)).toEqual([
      { close: 50_100, high: 50_150, low: 49_900, open: 50_000, timestamp: 1_800_000_000, volume: 100 },
      { close: 50_200, high: 50_250, low: 50_050, open: 50_100, timestamp: 1_800_000_060, volume: 101 },
    ]);
    expect(normalizeCoinbaseStarbaseChartPayload(STARBASE_CHART_PAYLOAD_FIXTURE)).toEqual({
      close: 50_100,
      high: 50_150,
      low: 49_900,
      open: 50_000,
      timestamp: 1_800_000_000,
      volume: 100,
    });
    expect(normalizeCoinbaseStarbaseChartData({ status: 'no_data' })).toEqual([]);
    expect(() => normalizeCoinbaseStarbaseChartData({ ...STARBASE_CHART_DATA_FIXTURE, close: [50_100] })).toThrow(
      'different lengths',
    );
  });

  it('builds exact published request params and channel names from compatibility symbols', () => {
    expect(buildCoinbaseStarbaseGetInstruments()).toEqual({
      method: 'public/get_instruments',
      params: { currency: 'any', expired: false, kind: 'future' },
    });
    expect(buildCoinbaseStarbaseTicker('BTC-PERP-INTX')).toEqual({
      method: 'public/ticker',
      params: { instrument_name: 'BTC_USDC-PERPETUAL' },
    });
    expect(buildCoinbaseStarbaseOrderBook('BTC-PERP-INTX', 20)).toEqual({
      method: 'public/get_order_book',
      params: { depth: 20, instrument_name: 'BTC_USDC-PERPETUAL' },
    });
    expect(buildCoinbaseStarbaseChartData('BTC-PERP-INTX', '1h', 1_800_000_000_000, 1_800_003_600_000)).toEqual({
      method: 'public/get_tradingview_chart_data',
      params: {
        end_timestamp: 1_800_003_600_000,
        instrument_name: 'BTC_USDC-PERPETUAL',
        resolution: '60',
        start_timestamp: 1_800_000_000_000,
      },
    });
    expect(buildCoinbaseStarbaseTickerChannel('BTC-PERP-INTX')).toBe('ticker.BTC_USDC-PERPETUAL.100ms');
    expect(buildCoinbaseStarbaseBookChannel('BTC-PERP-INTX', 'agg2')).toBe('book.BTC_USDC-PERPETUAL.agg2');
    expect(buildCoinbaseStarbaseChartChannel('BTC-PERP-INTX', '1d')).toBe('chart.trades.BTC_USDC-PERPETUAL.1D');
    expect(buildCoinbaseStarbaseInstrumentStateChannel('any')).toBe('instrument.state.future.any');
    expect(() => buildCoinbaseStarbaseChartChannel('BTC-PERP-INTX', '4h')).toThrow('Unsupported');
  });

  it('retains desired public subscriptions for reconnect and validates exact acknowledgements', () => {
    const subscriptions = new CoinbaseStarbasePublicSubscriptions();
    const ticker = buildCoinbaseStarbaseTickerChannel('BTC-PERP-INTX');
    const book = buildCoinbaseStarbaseBookChannel('BTC-PERP-INTX');

    expect(subscriptions.subscribe([ticker, book])).toEqual({
      method: 'public/subscribe',
      params: { channels: [ticker, book] },
    });
    subscriptions.validateAcknowledgement([ticker, book], [ticker, book]);
    expect(subscriptions.buildReconnectRequest()).toEqual({
      method: 'public/subscribe',
      params: { channels: [ticker, book] },
    });

    expect(subscriptions.unsubscribe([ticker])).toEqual({
      method: 'public/unsubscribe',
      params: { channels: [ticker] },
    });
    expect(subscriptions.buildReconnectRequest()).toEqual({
      method: 'public/subscribe',
      params: { channels: [book] },
    });
    expect(() => subscriptions.validateAcknowledgement([], [book])).toThrow('Incomplete');
  });
});
