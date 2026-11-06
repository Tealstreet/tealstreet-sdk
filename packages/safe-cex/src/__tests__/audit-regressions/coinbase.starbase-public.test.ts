import { describe, expect, it } from 'vitest';

import { CoinbaseStarbaseProtocolError } from '../reference/coinbase/coinbase.starbase';
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
} from '../reference/coinbase/coinbase.starbase-public';
import {
  STARBASE_BOOK_CHANGE_PAYLOAD,
  STARBASE_BOOK_SNAPSHOT_PAYLOAD,
  STARBASE_CHART_DATA_FIXTURE,
  STARBASE_CHART_PAYLOAD_FIXTURE,
  STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE,
  STARBASE_PUBLIC_INSTRUMENT_DISCOVERY_FIXTURE,
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
      contractSize: 0.0001,
      counter: 'USDC',
      createdAtMs: 1_646_824_342_000,
      expiresAtMs: 32_503_708_800_000,
      instrumentName: 'BTC_USDC-PERPETUAL',
      instrumentType: 'linear',
      isActive: true,
      kind: 'future',
      legacySavedSymbol: 'BTC-PERP-INTX',
      maximumLeverage: 50,
      minimumBaseAmount: 0.0001,
      priceIncrement: 0.5,
      priceIncrementSteps: [],
      priceIndex: 'btc_usdc',
      quote: 'USDC',
      settlement: 'USDC',
      settlementPeriod: 'perpetual',
      state: 'open',
    });
    expect(assessCoinbaseStarbaseMarketParity(instrument)).toEqual({
      status: 'blocked',
      gateway: instrument,
      blockers: ['amount-increment-unpublished', 'maximum-base-amount-unpublished'],
    });
  });

  it('filters raw discovery before strict decoding and requires both supported perpetuals', () => {
    expect(normalizeCoinbaseStarbaseInstruments(STARBASE_PUBLIC_INSTRUMENT_DISCOVERY_FIXTURE)).toEqual([
      normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_INSTRUMENT_FIXTURE),
      normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE),
    ]);
    expect(() => normalizeCoinbaseStarbaseInstruments([STARBASE_PUBLIC_INSTRUMENT_FIXTURE])).toThrow(
      'Missing supported',
    );
  });

  it('fails closed on product discriminators, live settlement fields, sizing, and duplicate names', () => {
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
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, future_type: 'reversed' }),
    ).toThrow('future_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, underlying_type: 'fiat' }),
    ).toThrow('underlying_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, option_type: 'call' }),
    ).toThrow('option_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, option_type: null }),
    ).toThrow('option_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, option_type: undefined }),
    ).toThrow('option_type');

    const accessorInstrument = { ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE };
    Object.defineProperty(accessorInstrument, 'option_type', {
      enumerable: true,
      get: () => {
        throw new Error('option_type accessor must not run');
      },
    });
    expect(() => normalizeCoinbaseStarbaseInstrument(accessorInstrument)).toThrow('Unsupported Starbase option_type');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, quote_currency: 'USD' }),
    ).toThrow('quote_currency');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, counter_currency: 'USD' }),
    ).toThrow('counter_currency');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, settlement_currency: 'USD' }),
    ).toThrow('settlement_currency');
    expect(() =>
      normalizeCoinbaseStarbaseInstrument({ ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE, contract_size: 0 }),
    ).toThrow('contract_size');
    expect(() =>
      normalizeCoinbaseStarbaseInstruments([
        STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
        STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
        STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE,
      ]),
    ).toThrow('Duplicate Starbase instrument_name');

    expect(
      normalizeCoinbaseStarbaseInstrument({
        ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
        unrelated_additive_gateway_field: { safely: 'ignored' },
      }),
    ).toEqual(normalizeCoinbaseStarbaseInstrument(STARBASE_PUBLIC_INSTRUMENT_FIXTURE));
  });

  it('preserves ticker amounts while naming live base/USD statistics explicitly', () => {
    expect(normalizeCoinbaseStarbaseTicker(STARBASE_PUBLIC_TICKER_FIXTURE)).toEqual({
      ask: 65_017,
      askBaseAmount: 0.0914,
      bid: 65_016.5,
      bidBaseAmount: 0.0046,
      fundingRate: 0.00001483,
      fundingRate8h: 0.00002502,
      index: 64_984.48,
      instrumentName: 'BTC_USDC-PERPETUAL',
      last: 64_985.5,
      legacySavedSymbol: 'BTC-PERP-INTX',
      mark: 65_001.69,
      openInterestBase: 430.2811,
      percentage: 0.748,
      state: 'open',
      timestampMs: 1_784_139_975_074,
      volumeBase: 216.8374,
      volumeUsd: 14_079_674.28,
    });
    expect(normalizeCoinbaseStarbaseTickerPayload({ data: STARBASE_PUBLIC_TICKER_FIXTURE }).bidBaseAmount).toBe(0.0046);
  });

  it('rejects malformed ticker numbers and unknown book states', () => {
    expect(() =>
      normalizeCoinbaseStarbaseTicker({ ...STARBASE_PUBLIC_TICKER_FIXTURE, best_bid_price: '50000' }),
    ).toThrow('best_bid_price');
    expect(() => normalizeCoinbaseStarbaseTicker({ ...STARBASE_PUBLIC_TICKER_FIXTURE, state: 'TRADING' })).toThrow(
      'unknown book state',
    );
  });

  it('preserves REST order-book base amounts and computes sorted cumulative totals', () => {
    expect(normalizeCoinbaseStarbaseOrderBookSnapshot(STARBASE_PUBLIC_ORDER_BOOK_FIXTURE)).toEqual({
      asks: [
        { amount: 0.0002, price: 65_015, total: 0.0002 },
        { amount: 0.01, price: 65_018.5, total: 0.0102 },
      ],
      bids: [
        { amount: 0.01, price: 65_013.5, total: 0.01 },
        { amount: 0.2222, price: 65_005, total: 0.23220000000000002 },
      ],
    });
  });

  it('applies published book snapshot/delta sequences and requires recovery on a gap or reconnect', () => {
    const book = new CoinbaseStarbaseSequencedOrderBook();

    expect(book.applyPayload(STARBASE_BOOK_SNAPSHOT_PAYLOAD)).toEqual({
      frame: 'snapshot',
      sequence: '167744732271',
      status: 'applied',
    });
    expect(book.orderBook).toEqual({
      asks: [
        { amount: 0.0149, price: 65_087, total: 0.0149 },
        { amount: 0.0498, price: 65_087.5, total: 0.0647 },
        { amount: 0.0011, price: 97_577.5, total: 0.0658 },
      ],
      bids: [],
    });

    expect(book.applyPayload(STARBASE_BOOK_CHANGE_PAYLOAD)).toEqual({
      frame: 'delta',
      sequence: '167744733629',
      status: 'applied',
    });
    expect(book.orderBook).toEqual({
      asks: [
        { amount: 0.0149, price: 65_073, total: 0.0149 },
        { amount: 0.0498, price: 65_073.5, total: 0.0647 },
        { amount: 0.0012, price: 97_577.5, total: 0.0659 },
      ],
      bids: [{ amount: 2.8663, price: 64_809.5, total: 2.8663 }],
    });

    expect(
      book.applyPayload({
        data: {
          ...STARBASE_BOOK_CHANGE_PAYLOAD.data,
          asks: [],
          bids: [],
          change_id: 167_744_733_631,
          prev_change_id: 167_744_733_630,
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
        data: { ...STARBASE_BOOK_SNAPSHOT_PAYLOAD.data, asks: [['replace', 65_087, 0.0149]] },
      }),
    ).toThrow('action');
    expect(() =>
      book.applyPayload({
        data: { ...STARBASE_BOOK_CHANGE_PAYLOAD.data, bids: [['delete', 65_087, 1]] },
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
            ['change', 65_087, 0.02],
            ['change', 65_020, 0.01],
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
        data: { ...STARBASE_BOOK_CHANGE_PAYLOAD.data, asks: [['new', 65_087, 0.02]], bids: [] },
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
      params: { currency: 'USDC', expired: false, kind: 'future' },
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
