/**
 * Inert public-data normalization for Coinbase's Starbase gateway.
 *
 * This module is intentionally not imported by the active Coinbase exchange,
 * WebSockets, routing table, or metadata. It only models fields published in
 * Coinbase's Starbase OpenAPI/AsyncAPI schemas.
 */

import type { Candle, OrderBook, Timeframe } from '../../../types';
import type { SequencedBookResult } from '../../../utils/sequencedDeltaOrderBook';

import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';
import { SequencedDeltaOrderBook } from '../../../utils/sequencedDeltaOrderBook';
import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';

const STARBASE_BASE_CURRENCIES = ['BTC', 'ETH'] as const;
const STARBASE_SUPPORTED_INSTRUMENT_NAMES = ['BTC_USDC-PERPETUAL', 'ETH_USDC-PERPETUAL'] as const;
const STARBASE_BOOK_STATES = ['open', 'settlement', 'delivered', 'inactive', 'locked', 'halted', 'archivized'] as const;
const STARBASE_BOOK_INTERVALS = ['100ms', 'agg2'] as const;
export const COINBASE_STARBASE_CHART_RESOLUTIONS = [
  '1',
  '3',
  '5',
  '15',
  '30',
  '60',
  '120',
  '360',
  '720',
  '1D',
] as const;
const STARBASE_BOOK_DEPTHS = [1, 5, 10, 20, 50, 100, 1000, 10000] as const;

export type CoinbaseStarbaseBaseCurrency = (typeof STARBASE_BASE_CURRENCIES)[number];
export type CoinbaseStarbaseBookState = (typeof STARBASE_BOOK_STATES)[number];
export type CoinbaseStarbaseBookInterval = (typeof STARBASE_BOOK_INTERVALS)[number];
export type CoinbaseStarbaseChartResolution = (typeof COINBASE_STARBASE_CHART_RESOLUTIONS)[number];
export type CoinbaseStarbaseOrderBookDepth = (typeof STARBASE_BOOK_DEPTHS)[number];

export type CoinbaseStarbasePublicInstrument = {
  instrumentName: string;
  legacySavedSymbol: string;
  base: CoinbaseStarbaseBaseCurrency;
  quote: 'USDC';
  counter: 'USDC';
  settlement: 'USDC';
  kind: 'future';
  instrumentType: 'linear';
  settlementPeriod: 'perpetual';
  isActive: boolean;
  state?: CoinbaseStarbaseBookState;
  minimumBaseAmount: number;
  contractSize: number;
  priceIncrement: number;
  priceIncrementSteps: readonly { abovePrice: number; priceIncrement: number }[];
  maximumLeverage?: number;
  createdAtMs: number;
  expiresAtMs: number;
  priceIndex: string;
};

export type CoinbaseStarbaseMarketParityBlocker = 'amount-increment-unpublished' | 'maximum-base-amount-unpublished';

export type CoinbaseStarbaseMarketParityResult = {
  status: 'blocked';
  gateway: CoinbaseStarbasePublicInstrument;
  blockers: CoinbaseStarbaseMarketParityBlocker[];
};

export type CoinbaseStarbasePublicTicker = {
  instrumentName: string;
  legacySavedSymbol: string;
  timestampMs: number;
  state: CoinbaseStarbaseBookState;
  bid: number;
  bidBaseAmount: number;
  ask: number;
  askBaseAmount: number;
  last: number;
  mark: number;
  index: number;
  percentage?: number;
  fundingRate?: number;
  fundingRate8h?: number;
  volumeBase: number;
  volumeUsd?: number;
  openInterestBase: number;
};

export type CoinbaseStarbasePublicRequest<Method extends string, Params extends Record<string, unknown>> = {
  method: Method;
  params: Params;
};

export type CoinbaseStarbasePublicChannel =
  | `ticker.${string}.${CoinbaseStarbaseBookInterval}`
  | `book.${string}.${CoinbaseStarbaseBookInterval}`
  | `chart.trades.${string}.${CoinbaseStarbaseChartResolution}`
  | `instrument.state.future.${CoinbaseStarbaseBaseCurrency | 'USDC' | 'any'}`;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const protocolError = (message: string): never => {
  throw new CoinbaseStarbaseProtocolError(message);
};

const requireRecord = (value: unknown, context: string): Record<string, unknown> => {
  if (!isRecord(value)) protocolError(`Invalid ${context}: expected an object`);
  return value as Record<string, unknown>;
};

const requireString = (value: unknown, context: string): string => {
  if (typeof value !== 'string' || !value) protocolError(`Invalid ${context}: expected a non-empty string`);
  return value as string;
};

const requireFinite = (value: unknown, context: string, minimum = 0): number => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < minimum) {
    protocolError(`Invalid ${context}: expected a finite number >= ${minimum}`);
  }
  return value as number;
};

const requirePositive = (value: unknown, context: string): number => {
  const parsed = requireFinite(value, context);
  if (parsed <= 0) protocolError(`Invalid ${context}: expected a positive number`);
  return parsed;
};

const requirePositiveInteger = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || Number(value) <= 0) {
    protocolError(`Invalid ${context}: expected a positive safe integer`);
  }
  return Number(value);
};

const requireTimestamp = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    protocolError(`Invalid ${context}: expected a non-negative safe integer timestamp`);
  }
  return Number(value);
};

const parseBookState = (value: unknown, context: string): CoinbaseStarbaseBookState => {
  if (!STARBASE_BOOK_STATES.includes(value as CoinbaseStarbaseBookState)) {
    protocolError(`Invalid ${context}: unknown book state`);
  }
  return value as CoinbaseStarbaseBookState;
};

const parseBaseCurrency = (value: unknown, context: string): CoinbaseStarbaseBaseCurrency => {
  if (!STARBASE_BASE_CURRENCIES.includes(value as CoinbaseStarbaseBaseCurrency)) {
    protocolError(`Invalid ${context}: unknown base currency`);
  }
  return value as CoinbaseStarbaseBaseCurrency;
};

const parseStarbasePerpetualName = (value: unknown): CoinbaseStarbaseBaseCurrency => {
  const instrumentName = requireString(value, 'Starbase instrument_name');
  const match = /^([A-Z0-9]+)_USDC-PERPETUAL$/.exec(instrumentName);
  if (!match) protocolError(`Invalid Starbase perpetual instrument name: ${instrumentName}`);
  return parseBaseCurrency(match![1], 'Starbase instrument_name base');
};

export const toCoinbaseStarbaseInstrumentName = (savedSymbol: string): string => {
  if (/^[A-Z0-9]+_USDC-PERPETUAL$/.test(savedSymbol)) {
    parseStarbasePerpetualName(savedSymbol);
    return savedSymbol;
  }

  const match = /^([A-Z0-9]+)-PERP-INTX$/.exec(savedSymbol);
  if (!match) protocolError(`Unsupported Coinbase INTX saved symbol: ${savedSymbol}`);
  const base = parseBaseCurrency(match![1], 'Coinbase INTX saved symbol base');
  return `${base}_USDC-PERPETUAL`;
};

export const toCoinbaseLegacyIntxSymbol = (instrumentName: string): string => {
  const base = parseStarbasePerpetualName(instrumentName);
  return `${base}-PERP-INTX`;
};

const parseTickSizeSteps = (value: unknown): CoinbaseStarbasePublicInstrument['priceIncrementSteps'] => {
  if (value === undefined) return [];
  if (!Array.isArray(value)) protocolError('Invalid Starbase instrument tick_size_steps');

  return (value as unknown[]).map((entry, index) => {
    const record = requireRecord(entry, `Starbase tick_size_steps[${index}]`);
    return {
      abovePrice: requirePositive(record.above_price, `Starbase tick_size_steps[${index}].above_price`),
      priceIncrement: requirePositive(record.tick_size, `Starbase tick_size_steps[${index}].tick_size`),
    };
  });
};

export const normalizeCoinbaseStarbaseInstrument = (value: unknown): CoinbaseStarbasePublicInstrument => {
  const raw = requireRecord(value, 'Starbase instrument');
  if (raw.kind !== 'future') protocolError('Unsupported Starbase instrument kind');
  if (raw.instrument_type !== 'linear') protocolError('Unsupported or missing Starbase instrument_type');
  if (raw.future_type !== undefined && raw.future_type !== 'linear') {
    protocolError('Unsupported Starbase future_type');
  }
  if (raw.underlying_type !== 'crypto') protocolError('Unsupported or missing Starbase underlying_type');
  if (Object.getOwnPropertyDescriptor(raw, 'option_type') !== undefined) {
    protocolError('Unsupported Starbase option_type');
  }
  if (raw.settlement_period !== 'perpetual') protocolError('Unsupported Starbase settlement_period');
  if (raw.quote_currency !== 'USDC') protocolError('Unsupported Starbase quote_currency');
  if (raw.counter_currency !== 'USDC') protocolError('Unsupported Starbase counter_currency');
  if (raw.settlement_currency !== 'USDC') protocolError('Unsupported Starbase settlement_currency');
  if (typeof raw.is_active !== 'boolean') protocolError('Invalid Starbase instrument is_active');

  const base = parseBaseCurrency(raw.base_currency, 'Starbase instrument base_currency');
  const instrumentName = requireString(raw.instrument_name, 'Starbase instrument_name');
  if (parseStarbasePerpetualName(instrumentName) !== base) {
    protocolError('Starbase instrument_name does not match base_currency');
  }

  const maximumLeverage =
    raw.max_leverage === undefined
      ? undefined
      : requirePositiveInteger(raw.max_leverage, 'Starbase instrument max_leverage');

  return {
    instrumentName,
    legacySavedSymbol: toCoinbaseLegacyIntxSymbol(instrumentName),
    base,
    quote: 'USDC',
    counter: 'USDC',
    settlement: 'USDC',
    kind: 'future',
    instrumentType: 'linear',
    settlementPeriod: 'perpetual',
    isActive: raw.is_active as boolean,
    ...(raw.state === undefined ? {} : { state: parseBookState(raw.state, 'Starbase instrument state') }),
    minimumBaseAmount: requirePositive(raw.min_trade_amount, 'Starbase instrument min_trade_amount'),
    contractSize: requirePositive(raw.contract_size, 'Starbase instrument contract_size'),
    priceIncrement: requirePositive(raw.tick_size, 'Starbase instrument tick_size'),
    priceIncrementSteps: parseTickSizeSteps(raw.tick_size_steps),
    ...(maximumLeverage === undefined ? {} : { maximumLeverage }),
    createdAtMs: requireTimestamp(raw.creation_timestamp, 'Starbase instrument creation_timestamp'),
    expiresAtMs: requireTimestamp(raw.expiration_timestamp, 'Starbase instrument expiration_timestamp'),
    priceIndex: requireString(raw.price_index, 'Starbase instrument price_index'),
  };
};

export const normalizeCoinbaseStarbaseInstruments = (value: unknown): CoinbaseStarbasePublicInstrument[] => {
  if (!Array.isArray(value)) protocolError('Invalid public/get_instruments result');
  const rawInstruments = (value as unknown[]).map((entry, index) =>
    requireRecord(entry, `public/get_instruments result[${index}]`),
  );
  const instruments = rawInstruments
    .filter((instrument) =>
      STARBASE_SUPPORTED_INSTRUMENT_NAMES.includes(
        instrument.instrument_name as (typeof STARBASE_SUPPORTED_INSTRUMENT_NAMES)[number],
      ),
    )
    .map(normalizeCoinbaseStarbaseInstrument);
  const names = new Set<string>();
  for (const instrument of instruments) {
    if (names.has(instrument.instrumentName)) protocolError('Duplicate Starbase instrument_name');
    names.add(instrument.instrumentName);
  }
  if (STARBASE_SUPPORTED_INSTRUMENT_NAMES.some((name) => !names.has(name))) {
    protocolError('Missing supported Starbase perpetual instrument');
  }
  return instruments;
};

/**
 * Starbase publishes a minimum base amount for linear futures and a price
 * tick, but not the amount step or maximum base amount required by safe-cex's
 * Market contract. Keep the normalized gateway instrument useful for parity
 * checks without inventing either precision field.
 */
export const assessCoinbaseStarbaseMarketParity = (
  gateway: CoinbaseStarbasePublicInstrument,
): CoinbaseStarbaseMarketParityResult => ({
  status: 'blocked',
  gateway,
  blockers: ['amount-increment-unpublished', 'maximum-base-amount-unpublished'],
});

export const normalizeCoinbaseStarbaseTicker = (value: unknown): CoinbaseStarbasePublicTicker => {
  const raw = requireRecord(value, 'Starbase ticker');
  const stats = requireRecord(raw.stats, 'Starbase ticker stats');
  const instrumentName = requireString(raw.instrument_name, 'Starbase ticker instrument_name');
  parseStarbasePerpetualName(instrumentName);

  const bid = requirePositive(raw.best_bid_price, 'Starbase ticker best_bid_price');
  const ask = requirePositive(raw.best_ask_price, 'Starbase ticker best_ask_price');
  const percentage =
    stats.price_change === undefined || stats.price_change === null
      ? undefined
      : requireFinite(stats.price_change, 'Starbase ticker stats.price_change', -Infinity);
  const volumeUsd =
    stats.volume_usd === undefined ? undefined : requireFinite(stats.volume_usd, 'Starbase ticker stats.volume_usd');
  const fundingRate =
    raw.current_funding === undefined
      ? undefined
      : requireFinite(raw.current_funding, 'Starbase ticker current_funding', -Infinity);
  const fundingRate8h =
    raw.funding_8h === undefined ? undefined : requireFinite(raw.funding_8h, 'Starbase ticker funding_8h', -Infinity);

  return {
    instrumentName,
    legacySavedSymbol: toCoinbaseLegacyIntxSymbol(instrumentName),
    timestampMs: requireTimestamp(raw.timestamp, 'Starbase ticker timestamp'),
    state: parseBookState(raw.state, 'Starbase ticker state'),
    bid,
    bidBaseAmount: requireFinite(raw.best_bid_amount, 'Starbase ticker best_bid_amount'),
    ask,
    askBaseAmount: requireFinite(raw.best_ask_amount, 'Starbase ticker best_ask_amount'),
    last: requirePositive(raw.last_price, 'Starbase ticker last_price'),
    mark: requirePositive(raw.mark_price, 'Starbase ticker mark_price'),
    index: requirePositive(raw.index_price, 'Starbase ticker index_price'),
    ...(percentage === undefined ? {} : { percentage }),
    ...(fundingRate === undefined ? {} : { fundingRate }),
    ...(fundingRate8h === undefined ? {} : { fundingRate8h }),
    volumeBase: requireFinite(stats.volume, 'Starbase ticker stats.volume'),
    ...(volumeUsd === undefined ? {} : { volumeUsd }),
    openInterestBase: requireFinite(raw.open_interest, 'Starbase ticker open_interest'),
  };
};

export const normalizeCoinbaseStarbaseTickerPayload = (value: unknown): CoinbaseStarbasePublicTicker => {
  const payload = requireRecord(value, 'Starbase ticker subscription payload');
  return normalizeCoinbaseStarbaseTicker(payload.data);
};

const parseRestBookLevels = (value: unknown, context: string): OrderBook['bids'] => {
  if (!Array.isArray(value)) protocolError(`Invalid ${context}: expected an array`);
  return (value as unknown[]).map((entry, index) => {
    if (!Array.isArray(entry) || entry.length !== 2) protocolError(`Invalid ${context}[${index}]`);
    const level = entry as unknown[];
    const price = requirePositive(level[0], `${context}[${index}].price`);
    return {
      price,
      amount: requireFinite(level[1], `${context}[${index}].amount`),
      total: 0,
    };
  });
};

export const normalizeCoinbaseStarbaseOrderBookSnapshot = (value: unknown): OrderBook => {
  const raw = requireRecord(value, 'Starbase order book');
  const book: OrderBook = {
    bids: parseRestBookLevels(raw.bids, 'Starbase order book bids'),
    asks: parseRestBookLevels(raw.asks, 'Starbase order book asks'),
  };
  sortOrderBook(book);
  calcOrderBookTotal(book);
  return book;
};

type StarbaseBookAction = 'new' | 'change' | 'delete';
type ParsedStarbaseBookLevel = {
  action: StarbaseBookAction;
  amount: number;
  price: number;
};

const parseDeltaLevels = (
  value: unknown,
  frameType: 'snapshot' | 'change',
  context: string,
): readonly ParsedStarbaseBookLevel[] => {
  if (!Array.isArray(value)) protocolError(`Invalid ${context}: expected an array`);
  return (value as unknown[]).map((entry, index) => {
    if (!Array.isArray(entry) || entry.length !== 3) protocolError(`Invalid ${context}[${index}]`);
    const level = entry as unknown[];
    const action = level[0] as StarbaseBookAction;
    if (!['new', 'change', 'delete'].includes(action)) protocolError(`Invalid ${context}[${index}] action`);
    if (frameType === 'snapshot' && action !== 'new') protocolError(`Invalid ${context}[${index}] snapshot action`);
    const price = requirePositive(level[1], `${context}[${index}].price`);
    const amount = requireFinite(level[2], `${context}[${index}].amount`);
    if (action === 'delete' && amount !== 0) protocolError(`Invalid ${context}[${index}] delete amount`);
    if (action !== 'delete' && amount === 0) protocolError(`Invalid ${context}[${index}] ${action} amount`);
    return {
      action,
      amount,
      price,
    };
  });
};

export class CoinbaseStarbaseSequencedOrderBook {
  readonly orderBook: OrderBook = { bids: [], asks: [] };
  private awaitingSnapshot = false;
  private hasSnapshot = false;
  private readonly sequencedBook = new SequencedDeltaOrderBook(this.orderBook);

  applyPayload(value: unknown): SequencedBookResult {
    const payload = requireRecord(value, 'Starbase book subscription payload');
    const data = requireRecord(payload.data, 'Starbase book subscription data');
    const frameType = data.type;
    if (frameType !== 'snapshot' && frameType !== 'change') {
      protocolError('Invalid Starbase book subscription type');
    }
    parseStarbasePerpetualName(data.instrument_name);
    if (this.awaitingSnapshot && frameType === 'change') {
      return { status: 'ignored', reason: 'awaiting-snapshot' };
    }
    const sequence = requireTimestamp(data.change_id, 'Starbase book change_id');
    const validatedFrameType = frameType as 'snapshot' | 'change';
    const bids = parseDeltaLevels(data.bids, validatedFrameType, 'Starbase book bids');
    const asks = parseDeltaLevels(data.asks, validatedFrameType, 'Starbase book asks');
    const previousSequence =
      validatedFrameType === 'change'
        ? requireTimestamp(data.prev_change_id, 'Starbase book prev_change_id')
        : undefined;
    const bidLevels = bids.map(({ amount, price }) => [price, amount] as const);
    const askLevels = asks.map(({ amount, price }) => [price, amount] as const);
    if (validatedFrameType === 'change' && !this.hasSnapshot) {
      return this.sequencedBook.applyDelta({
        sequence,
        previousSequence: previousSequence!,
        bids: bidLevels,
        asks: askLevels,
      });
    }
    if (
      !this.actionsMatchBook('bids', bids, validatedFrameType) ||
      !this.actionsMatchBook('asks', asks, validatedFrameType)
    ) {
      this.sequencedBook.reset();
      this.awaitingSnapshot = true;
      this.hasSnapshot = false;
      return { status: 'recover', reason: 'invalid-level' };
    }

    const result =
      validatedFrameType === 'snapshot'
        ? this.sequencedBook.applySnapshot({ sequence, bids: bidLevels, asks: askLevels })
        : this.sequencedBook.applyDelta({
            sequence,
            previousSequence: previousSequence!,
            bids: bidLevels,
            asks: askLevels,
          });
    if (result.status === 'applied') {
      this.awaitingSnapshot = false;
      this.hasSnapshot = true;
      calcOrderBookTotal(this.orderBook);
    } else if (result.status === 'recover') {
      this.hasSnapshot = false;
    }
    return result;
  }

  onReconnect(): void {
    this.sequencedBook.reset();
    this.awaitingSnapshot = false;
    this.hasSnapshot = false;
  }

  private actionsMatchBook(
    side: 'bids' | 'asks',
    levels: readonly ParsedStarbaseBookLevel[],
    frameType: 'snapshot' | 'change',
  ): boolean {
    const prices = new Set(frameType === 'snapshot' ? [] : this.orderBook[side].map(({ price }) => price));
    for (const level of levels) {
      const exists = prices.has(level.price);
      if (level.action === 'new') {
        if (exists) return false;
        prices.add(level.price);
      } else {
        if (!exists) return false;
        if (level.action === 'delete') prices.delete(level.price);
      }
    }
    return true;
  }
}

const parseCandle = (raw: Record<string, unknown>, context: string): Candle => ({
  timestamp: requireTimestamp(raw.tick, `${context}.tick`) / 1000,
  open: requirePositive(raw.open, `${context}.open`),
  high: requirePositive(raw.high, `${context}.high`),
  low: requirePositive(raw.low, `${context}.low`),
  close: requirePositive(raw.close, `${context}.close`),
  volume: requireFinite(raw.volume, `${context}.volume`),
});

export const normalizeCoinbaseStarbaseChartPayload = (value: unknown): Candle => {
  const payload = requireRecord(value, 'Starbase chart subscription payload');
  return parseCandle(requireRecord(payload.data, 'Starbase chart subscription data'), 'Starbase chart data');
};

export const normalizeCoinbaseStarbaseChartData = (value: unknown): Candle[] => {
  const raw = requireRecord(value, 'Starbase chart data');
  if (raw.status === 'no_data') return [];
  if (raw.status !== 'ok') protocolError('Invalid Starbase chart status');

  const fields = ['ticks', 'open', 'high', 'low', 'close', 'volume'] as const;
  const arrays: unknown[][] = fields.map((field) => {
    const entries = raw[field];
    if (!Array.isArray(entries)) protocolError(`Invalid Starbase chart ${field}`);
    return entries as unknown[];
  });
  if (arrays.some((entries) => entries.length !== arrays[0].length)) {
    protocolError('Invalid Starbase chart data: parallel arrays have different lengths');
  }

  return arrays[0].map((_, index) =>
    parseCandle(
      {
        tick: arrays[0][index],
        open: arrays[1][index],
        high: arrays[2][index],
        low: arrays[3][index],
        close: arrays[4][index],
        volume: arrays[5][index],
      },
      `Starbase chart data[${index}]`,
    ),
  );
};

export const COINBASE_STARBASE_TIMEFRAME_RESOLUTION: Partial<Record<Timeframe, CoinbaseStarbaseChartResolution>> =
  Object.freeze({
    '1m': '1',
    '3m': '3',
    '5m': '5',
    '15m': '15',
    '30m': '30',
    '1h': '60',
    '2h': '120',
    '6h': '360',
    '12h': '720',
    '1d': '1D',
  });

const resolveChartResolution = (timeframe: Timeframe): CoinbaseStarbaseChartResolution => {
  const resolution = COINBASE_STARBASE_TIMEFRAME_RESOLUTION[timeframe];
  if (!resolution) protocolError(`Unsupported Starbase chart timeframe: ${timeframe}`);
  return resolution!;
};

export const buildCoinbaseStarbaseGetInstruments = (): CoinbaseStarbasePublicRequest<
  'public/get_instruments',
  { currency: 'USDC'; expired: false; kind: 'future' }
> => ({
  method: 'public/get_instruments',
  params: { currency: 'USDC', expired: false, kind: 'future' },
});

export const buildCoinbaseStarbaseTicker = (
  symbol: string,
): CoinbaseStarbasePublicRequest<'public/ticker', { instrument_name: string }> => ({
  method: 'public/ticker',
  params: { instrument_name: toCoinbaseStarbaseInstrumentName(symbol) },
});

export const buildCoinbaseStarbaseOrderBook = (
  symbol: string,
  depth: CoinbaseStarbaseOrderBookDepth,
): CoinbaseStarbasePublicRequest<
  'public/get_order_book',
  { depth: CoinbaseStarbaseOrderBookDepth; instrument_name: string }
> => {
  if (!STARBASE_BOOK_DEPTHS.includes(depth)) protocolError('Unsupported Starbase order book depth');
  return {
    method: 'public/get_order_book',
    params: { depth, instrument_name: toCoinbaseStarbaseInstrumentName(symbol) },
  };
};

export const buildCoinbaseStarbaseChartData = (
  symbol: string,
  timeframe: Timeframe,
  startTimestamp: number,
  endTimestamp: number,
): CoinbaseStarbasePublicRequest<
  'public/get_tradingview_chart_data',
  {
    end_timestamp: number;
    instrument_name: string;
    resolution: CoinbaseStarbaseChartResolution;
    start_timestamp: number;
  }
> => {
  const start = requireTimestamp(startTimestamp, 'Starbase chart start_timestamp');
  const end = requireTimestamp(endTimestamp, 'Starbase chart end_timestamp');
  if (end < start) protocolError('Starbase chart end_timestamp precedes start_timestamp');
  return {
    method: 'public/get_tradingview_chart_data',
    params: {
      end_timestamp: end,
      instrument_name: toCoinbaseStarbaseInstrumentName(symbol),
      resolution: resolveChartResolution(timeframe),
      start_timestamp: start,
    },
  };
};

export const buildCoinbaseStarbaseTickerChannel = (
  symbol: string,
  interval: CoinbaseStarbaseBookInterval = '100ms',
): CoinbaseStarbasePublicChannel => {
  if (!STARBASE_BOOK_INTERVALS.includes(interval)) protocolError('Unsupported Starbase ticker interval');
  return `ticker.${toCoinbaseStarbaseInstrumentName(symbol)}.${interval}`;
};

export const buildCoinbaseStarbaseBookChannel = (
  symbol: string,
  interval: CoinbaseStarbaseBookInterval = '100ms',
): CoinbaseStarbasePublicChannel => {
  if (!STARBASE_BOOK_INTERVALS.includes(interval)) protocolError('Unsupported Starbase book interval');
  return `book.${toCoinbaseStarbaseInstrumentName(symbol)}.${interval}`;
};

export const buildCoinbaseStarbaseChartChannel = (
  symbol: string,
  timeframe: Timeframe,
): CoinbaseStarbasePublicChannel =>
  `chart.trades.${toCoinbaseStarbaseInstrumentName(symbol)}.${resolveChartResolution(timeframe)}`;

export const buildCoinbaseStarbaseInstrumentStateChannel = (
  currency: CoinbaseStarbaseBaseCurrency | 'USDC' | 'any',
): CoinbaseStarbasePublicChannel => {
  if (![...STARBASE_BASE_CURRENCIES, 'USDC', 'any'].includes(currency)) {
    protocolError('Unsupported Starbase instrument-state currency');
  }
  return `instrument.state.future.${currency}`;
};

const buildSubscriptionRequest = <Method extends 'public/subscribe' | 'public/unsubscribe'>(
  method: Method,
  channels: readonly CoinbaseStarbasePublicChannel[],
): CoinbaseStarbasePublicRequest<Method, { channels: CoinbaseStarbasePublicChannel[] }> => {
  if (channels.length === 0 || new Set(channels).size !== channels.length) {
    protocolError(`Invalid ${method} channels`);
  }
  return { method, params: { channels: [...channels] } };
};

export class CoinbaseStarbasePublicSubscriptions {
  private readonly desiredChannels = new Set<CoinbaseStarbasePublicChannel>();

  subscribe(channels: readonly CoinbaseStarbasePublicChannel[]) {
    const request = buildSubscriptionRequest('public/subscribe', channels);
    channels.forEach((channel) => this.desiredChannels.add(channel));
    return request;
  }

  unsubscribe(channels: readonly CoinbaseStarbasePublicChannel[]) {
    const request = buildSubscriptionRequest('public/unsubscribe', channels);
    channels.forEach((channel) => this.desiredChannels.delete(channel));
    return request;
  }

  buildReconnectRequest() {
    if (this.desiredChannels.size === 0) return undefined;
    return buildSubscriptionRequest('public/subscribe', [...this.desiredChannels]);
  }

  validateAcknowledgement(value: unknown, requested: readonly CoinbaseStarbasePublicChannel[]): void {
    if (!Array.isArray(value) || value.some((channel) => typeof channel !== 'string')) {
      protocolError('Invalid Starbase subscription acknowledgement');
    }
    const acknowledged = value as string[];
    if (acknowledged.length !== requested.length || requested.some((channel) => !acknowledged.includes(channel))) {
      protocolError('Incomplete Starbase subscription acknowledgement');
    }
  }
}
