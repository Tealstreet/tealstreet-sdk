import type { Candle, Market, OrderBook } from '../../../types';
import type { WooV3Decimal, WooV3Instrument } from './woo.v3.contracts';

import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';

/**
 * Strict, transport-free decoders and mappers for the namespaced WOO V3 public
 * REST surface. This module is intentionally not imported by the live Woo
 * exchange or websocket implementations.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3FuturesRow = Readonly<{
  symbol: string;
  indexPrice: WooV3Decimal;
  markPrice: WooV3Decimal;
  estFundingRate: WooV3Decimal;
  lastFundingRate: WooV3Decimal;
  openInterest: WooV3Decimal;
  '24hOpen': WooV3Decimal;
  '24hClose': WooV3Decimal;
  '24hHigh': WooV3Decimal;
  '24hLow': WooV3Decimal;
  '24hVolume': WooV3Decimal;
  '24hAmount': WooV3Decimal;
  nextFundingTime: number;
}>;

export type WooV3OrderBookLevel = Readonly<{
  price: WooV3Decimal;
  quantity: WooV3Decimal;
}>;

export type WooV3OrderBookSnapshot = Readonly<{
  timestamp: number;
  asks: readonly WooV3OrderBookLevel[];
  bids: readonly WooV3OrderBookLevel[];
}>;

export type WooV3MappedOrderBookSnapshot = OrderBook &
  Readonly<{
    timestamp: number;
  }>;

export type WooV3KlineRow = Readonly<{
  open: WooV3Decimal;
  close: WooV3Decimal;
  low: WooV3Decimal;
  high: WooV3Decimal;
  volume: WooV3Decimal;
  amount: WooV3Decimal;
  symbol: string;
  type: string;
  startTimestamp: number;
  endTimestamp: number;
}>;

/**
 * Public futures statistics intentionally stop short of safe-cex `Ticker`.
 * The V3 REST response does not publish authoritative best bid, best ask, or
 * last-trade fields, and filling those fields from `24hClose` would mislabel a
 * rolling statistic as a live quote.
 */
export type WooV3FuturesStats = Readonly<{
  id: string;
  symbol: string;
  index: number;
  mark: number;
  open24h: number;
  close24h: number;
  high24h: number;
  low24h: number;
  percentage24h: number;
  openInterest: number;
  estimatedFundingRate: number;
  lastFundingRate: number;
  fundingTime: number;
  volume24h: number;
  quoteVolume24h: number;
}>;

const isRecord = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (!isRecord(value)) throw new Error(`${path} must be an object`);
  return value;
};

const requireArray = (value: unknown, path: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
  return value;
};

const requireString = (value: unknown, path: string): string => {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`${path} must be a non-empty string`);
  return value;
};

const optionalString = (value: unknown, path: string): string | undefined => {
  if (value === undefined) return undefined;
  return requireString(value, path);
};

const requireInteger = (value: unknown, path: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`${path} must be a non-negative safe integer`);
  }
  return value as number;
};

const requireTimestamp = (value: unknown, path: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    throw new Error(`${path} must be a positive safe integer millisecond timestamp`);
  }
  return value as number;
};

const requireBoolean = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  return value;
};

const requireDecimalString = (value: unknown, path: string): WooV3Decimal => {
  if (typeof value !== 'string' || value.trim() === '' || !Number.isFinite(Number(value))) {
    throw new Error(`${path} must be a finite decimal string`);
  }
  return value;
};

const requireInstrumentDecimal = (value: unknown, path: string): WooV3Decimal | number => {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  return requireDecimalString(value, path);
};

const optionalInstrumentDecimal = (value: unknown, path: string): WooV3Decimal | number | undefined => {
  if (value === undefined) return undefined;
  return requireInstrumentDecimal(value, path);
};

const requireFiniteNumber = (value: WooV3Decimal | number, path: string, minimum = 0): number => {
  const result = Number(value);
  if (!Number.isFinite(result) || result < minimum) throw new Error(`${path} must be at least ${minimum}`);
  return result;
};

const decodeEnvelopeWithTimestamp = (
  input: unknown,
  name: string,
  timestampDecoder: (value: unknown, path: string) => number,
): Readonly<{ data: UnknownRecord; timestamp: number }> => {
  const response = requireRecord(input, `WOO V3 ${name} response`);
  if (requireBoolean(response.success, `WOO V3 ${name}.success`) !== true) {
    throw new Error(`WOO V3 ${name} response reports success=false`);
  }
  return Object.freeze({
    data: requireRecord(response.data, `WOO V3 ${name}.data`),
    timestamp: timestampDecoder(response.timestamp, `WOO V3 ${name}.timestamp`),
  });
};

const decodeEnvelope = (input: unknown, name: string): UnknownRecord =>
  decodeEnvelopeWithTimestamp(input, name, requireInteger).data;

const decodeRows = <T>(input: unknown, name: string, decoder: (row: unknown, path: string) => T): readonly T[] => {
  const data = decodeEnvelope(input, name);
  return requireArray(data.rows, `WOO V3 ${name}.data.rows`).map((row, index) =>
    decoder(row, `WOO V3 ${name}.data.rows[${index}]`),
  );
};

const decodeInstrument = (input: unknown, path: string): WooV3Instrument => {
  const row = requireRecord(input, path);
  return Object.freeze({
    symbol: requireString(row.symbol, `${path}.symbol`),
    status: optionalString(row.status, `${path}.status`),
    baseAsset: requireString(row.baseAsset, `${path}.baseAsset`),
    quoteAsset: requireString(row.quoteAsset, `${path}.quoteAsset`),
    baseAssetMultiplier: optionalInstrumentDecimal(row.baseAssetMultiplier, `${path}.baseAssetMultiplier`),
    baseMin: requireInstrumentDecimal(row.baseMin, `${path}.baseMin`),
    baseMax: requireInstrumentDecimal(row.baseMax, `${path}.baseMax`),
    baseTick: requireInstrumentDecimal(row.baseTick, `${path}.baseTick`),
    quoteTick: requireInstrumentDecimal(row.quoteTick, `${path}.quoteTick`),
    minNotional: requireInstrumentDecimal(row.minNotional, `${path}.minNotional`),
    baseIMR: requireInstrumentDecimal(row.baseIMR, `${path}.baseIMR`),
    fundingIntervalHours:
      row.fundingIntervalHours === undefined
        ? undefined
        : requireInteger(row.fundingIntervalHours, `${path}.fundingIntervalHours`),
    isAllowedRpi: row.isAllowedRpi === undefined ? undefined : requireBoolean(row.isAllowedRpi, `${path}.isAllowedRpi`),
  });
};

const decodeFuturesRow = (input: unknown, path: string): WooV3FuturesRow => {
  const row = requireRecord(input, path);
  return Object.freeze({
    symbol: requireString(row.symbol, `${path}.symbol`),
    indexPrice: requireDecimalString(row.indexPrice, `${path}.indexPrice`),
    markPrice: requireDecimalString(row.markPrice, `${path}.markPrice`),
    estFundingRate: requireDecimalString(row.estFundingRate, `${path}.estFundingRate`),
    lastFundingRate: requireDecimalString(row.lastFundingRate, `${path}.lastFundingRate`),
    openInterest: requireDecimalString(row.openInterest, `${path}.openInterest`),
    '24hOpen': requireDecimalString(row['24hOpen'], `${path}.24hOpen`),
    '24hClose': requireDecimalString(row['24hClose'], `${path}.24hClose`),
    '24hHigh': requireDecimalString(row['24hHigh'], `${path}.24hHigh`),
    '24hLow': requireDecimalString(row['24hLow'], `${path}.24hLow`),
    '24hVolume': requireDecimalString(row['24hVolume'], `${path}.24hVolume`),
    '24hAmount': requireDecimalString(row['24hAmount'], `${path}.24hAmount`),
    nextFundingTime: requireInteger(row.nextFundingTime, `${path}.nextFundingTime`),
  });
};

const decodeOrderBookLevel = (input: unknown, path: string): WooV3OrderBookLevel => {
  const level = requireRecord(input, path);
  return Object.freeze({
    price: requireDecimalString(level.price, `${path}.price`),
    quantity: requireDecimalString(level.quantity, `${path}.quantity`),
  });
};

const decodeKlineRow = (input: unknown, path: string): WooV3KlineRow => {
  const row = requireRecord(input, path);
  const startTimestamp = requireInteger(row.startTimestamp, `${path}.startTimestamp`);
  const endTimestamp = requireInteger(row.endTimestamp, `${path}.endTimestamp`);
  if (endTimestamp < startTimestamp) throw new Error(`${path}.endTimestamp must not precede startTimestamp`);

  return Object.freeze({
    open: requireDecimalString(row.open, `${path}.open`),
    close: requireDecimalString(row.close, `${path}.close`),
    low: requireDecimalString(row.low, `${path}.low`),
    high: requireDecimalString(row.high, `${path}.high`),
    volume: requireDecimalString(row.volume, `${path}.volume`),
    amount: requireDecimalString(row.amount, `${path}.amount`),
    symbol: requireString(row.symbol, `${path}.symbol`),
    type: requireString(row.type, `${path}.type`),
    startTimestamp,
    endTimestamp,
  });
};

export const decodeWooV3Instruments = (input: unknown): readonly WooV3Instrument[] =>
  decodeRows(input, 'instruments', decodeInstrument);

export const decodeWooV3Futures = (input: unknown): readonly WooV3FuturesRow[] =>
  decodeRows(input, 'futures', decodeFuturesRow);

export const decodeWooV3OrderBook = (input: unknown): WooV3OrderBookSnapshot => {
  const { data, timestamp } = decodeEnvelopeWithTimestamp(input, 'orderbook', requireTimestamp);
  return Object.freeze({
    timestamp,
    asks: requireArray(data.asks, 'WOO V3 orderbook.data.asks').map((level, index) =>
      decodeOrderBookLevel(level, `WOO V3 orderbook.data.asks[${index}]`),
    ),
    bids: requireArray(data.bids, 'WOO V3 orderbook.data.bids').map((level, index) =>
      decodeOrderBookLevel(level, `WOO V3 orderbook.data.bids[${index}]`),
    ),
  });
};

export const decodeWooV3Klines = (input: unknown): readonly WooV3KlineRow[] =>
  decodeRows(input, 'kline', decodeKlineRow);

export const mapWooV3Markets = (rows: readonly WooV3Instrument[]): Market[] =>
  rows
    .filter((row) => row.symbol.startsWith('PERP_'))
    .map((row, index): Market => {
      const path = `WOO V3 instrument[${index}]`;
      const amountMin = requireFiniteNumber(row.baseMin, `${path}.baseMin`, Number.MIN_VALUE);
      const amountMax = requireFiniteNumber(row.baseMax, `${path}.baseMax`, amountMin);
      const baseIMR = requireFiniteNumber(row.baseIMR, `${path}.baseIMR`, Number.MIN_VALUE);
      if (baseIMR > 1) throw new Error(`${path}.baseIMR must not exceed 1`);

      return {
        id: `${row.baseAsset}/${row.quoteAsset}:${row.quoteAsset}`,
        symbol: `${row.baseAsset}${row.quoteAsset}`,
        base: row.baseAsset,
        quote: row.quoteAsset,
        active: row.status === undefined || row.status === 'TRADING',
        precision: {
          amount: requireFiniteNumber(row.baseTick, `${path}.baseTick`, Number.MIN_VALUE),
          price: requireFiniteNumber(row.quoteTick, `${path}.quoteTick`, Number.MIN_VALUE),
        },
        limits: {
          amount: { min: amountMin, max: amountMax },
          leverage: { min: 1, max: Math.floor(1 / baseIMR) },
        },
        info: row,
      };
    });

export const mapWooV3FuturesStats = (
  rows: readonly WooV3FuturesRow[],
  marketsBySymbol: Readonly<Record<string, Market>>,
): WooV3FuturesStats[] =>
  rows.flatMap((row, index) => {
    const normalizedSymbol = (
      row.symbol.startsWith('PERP_') ? row.symbol.slice('PERP_'.length) : row.symbol
    ).replaceAll('_', '');
    const market = marketsBySymbol[normalizedSymbol] ?? marketsBySymbol[row.symbol];
    if (!market) return [];

    const path = `WOO V3 futures[${index}]`;
    const open24h = requireFiniteNumber(row['24hOpen'], `${path}.24hOpen`, Number.MIN_VALUE);
    const close24h = requireFiniteNumber(row['24hClose'], `${path}.24hClose`);
    return [
      {
        id: market.id,
        symbol: market.symbol,
        index: requireFiniteNumber(row.indexPrice, `${path}.indexPrice`),
        mark: requireFiniteNumber(row.markPrice, `${path}.markPrice`),
        open24h,
        close24h,
        high24h: requireFiniteNumber(row['24hHigh'], `${path}.24hHigh`),
        low24h: requireFiniteNumber(row['24hLow'], `${path}.24hLow`),
        percentage24h: Math.round(((close24h - open24h) / open24h) * 10_000) / 100,
        openInterest: requireFiniteNumber(row.openInterest, `${path}.openInterest`),
        estimatedFundingRate: requireFiniteNumber(row.estFundingRate, `${path}.estFundingRate`, -Infinity),
        lastFundingRate: requireFiniteNumber(row.lastFundingRate, `${path}.lastFundingRate`, -Infinity),
        fundingTime: row.nextFundingTime,
        volume24h: requireFiniteNumber(row['24hVolume'], `${path}.24hVolume`),
        quoteVolume24h: requireFiniteNumber(row['24hAmount'], `${path}.24hAmount`),
      },
    ];
  });

export const mapWooV3OrderBook = (snapshot: WooV3OrderBookSnapshot): WooV3MappedOrderBookSnapshot => {
  const mapSide = (levels: readonly WooV3OrderBookLevel[], side: string) =>
    levels.map((level, index) => ({
      price: requireFiniteNumber(level.price, `WOO V3 orderbook.${side}[${index}].price`, Number.MIN_VALUE),
      amount: requireFiniteNumber(level.quantity, `WOO V3 orderbook.${side}[${index}].quantity`, Number.MIN_VALUE),
      total: 0,
    }));

  const orderBook: WooV3MappedOrderBookSnapshot = {
    timestamp: requireTimestamp(snapshot.timestamp, 'WOO V3 orderbook.timestamp'),
    bids: mapSide(snapshot.bids, 'bids'),
    asks: mapSide(snapshot.asks, 'asks'),
  };
  // WOO documents both snapshot sides as descending. safe-cex requires bids
  // descending and asks ascending, so normalize instead of trusting wire order.
  sortOrderBook(orderBook);
  calcOrderBookTotal(orderBook);
  return orderBook;
};

export const mapWooV3Candles = (rows: readonly WooV3KlineRow[]): Candle[] =>
  rows
    .map((row, index): Candle => {
      const path = `WOO V3 kline[${index}]`;
      return {
        timestamp: row.startTimestamp / 1000,
        open: requireFiniteNumber(row.open, `${path}.open`),
        high: requireFiniteNumber(row.high, `${path}.high`),
        low: requireFiniteNumber(row.low, `${path}.low`),
        close: requireFiniteNumber(row.close, `${path}.close`),
        volume: requireFiniteNumber(row.volume, `${path}.volume`),
      };
    })
    .sort((a, b) => a.timestamp - b.timestamp);
