import type { Candle, Market, OrderBook, OrderBookOrders, Ticker } from '../../../types';
import type { WooV3FuturesStats, WooV3KlineRow, WooV3MappedOrderBookSnapshot } from './woo.v3.public';

import { mapWooV3Candles } from './woo.v3.public';

/**
 * Exact, transport-free canonicalizers and diagnostics for the inert WOO V3
 * public-read shadow. No function in this module owns a store or grants V3
 * authority. Every comparison returns the legacy snapshot by identity.
 */

export type WooV3PublicParityRouteId = 'markets' | 'tickers' | 'orderbook' | 'recent-klines' | 'historical-klines';

export type WooV3PublicParityMismatch = Readonly<{
  kind: 'value' | 'missing-in-v3' | 'unexpected-in-v3' | 'duplicate-key';
  path: string;
  legacyValue: unknown;
  v3Value: unknown;
  provenance: Readonly<{ legacy: string; v3: string }>;
}>;

export type WooV3PublicParityBlocker = Readonly<{
  code:
    | 'v3-futures-rest-has-no-live-quotes'
    | 'invalid-request-context'
    | 'request-symbol-mismatch'
    | 'request-interval-mismatch'
    | 'unsupported-kline-interval'
    | 'v3-kline-row-symbol-mismatch'
    | 'v3-kline-row-interval-mismatch'
    | 'v3-kline-row-end-timestamp-mismatch';
  detail: string;
  path?: string;
  expected?: unknown;
  actual?: unknown;
}>;

export type WooV3PublicRequestContext = Readonly<{ symbol: string }>;
export type WooV3KlineRequestContext = Readonly<{ symbol: string; interval: string }>;

export type WooV3PublicParityResult<TLegacy, TV3> = Readonly<{
  status: 'match' | 'mismatch' | 'blocked';
  routeId: WooV3PublicParityRouteId;
  authoritative: TLegacy;
  observed: TV3;
  legacyCanonical: string;
  v3Canonical: string;
  mismatches: readonly WooV3PublicParityMismatch[];
  blockers: readonly WooV3PublicParityBlocker[];
  storeAction: 'retain-legacy';
}>;

type FieldComparison<TLegacy, TV3> = Readonly<{
  path: string;
  legacy: (value: TLegacy) => unknown;
  v3: (value: TV3) => unknown;
  legacyProvenance: string;
  v3Provenance: string;
}>;

const canonicalString = (value: unknown): string => JSON.stringify(value);

const blocker = (
  code: WooV3PublicParityBlocker['code'],
  detail: string,
  context: Pick<WooV3PublicParityBlocker, 'path' | 'expected' | 'actual'> = {},
): WooV3PublicParityBlocker => Object.freeze({ code, detail, ...context });

const mismatch = (
  kind: WooV3PublicParityMismatch['kind'],
  path: string,
  legacyValue: unknown,
  v3Value: unknown,
  legacy: string,
  v3: string,
): WooV3PublicParityMismatch =>
  Object.freeze({
    kind,
    path,
    legacyValue,
    v3Value,
    provenance: Object.freeze({ legacy, v3 }),
  });

const compareFields = <TLegacy, TV3>(
  legacy: TLegacy,
  v3: TV3,
  prefix: string,
  fields: readonly FieldComparison<TLegacy, TV3>[],
): WooV3PublicParityMismatch[] =>
  fields.flatMap((field) => {
    const legacyValue = field.legacy(legacy);
    const v3Value = field.v3(v3);
    return Object.is(legacyValue, v3Value)
      ? []
      : [
          mismatch(
            'value',
            `${prefix}.${field.path}`,
            legacyValue,
            v3Value,
            field.legacyProvenance,
            field.v3Provenance,
          ),
        ];
  });

const compareKeyed = <TLegacy, TV3>(
  input: Readonly<{
    path: string;
    legacy: readonly TLegacy[];
    v3: readonly TV3[];
    legacyKey: (value: TLegacy) => string;
    v3Key: (value: TV3) => string;
    fields: readonly FieldComparison<TLegacy, TV3>[];
    legacyProvenance: string;
    v3Provenance: string;
  }>,
): WooV3PublicParityMismatch[] => {
  const mismatches: WooV3PublicParityMismatch[] = [];
  const legacyByKey = new Map<string, TLegacy>();
  const v3ByKey = new Map<string, TV3>();

  for (const value of input.legacy) {
    const key = input.legacyKey(value);
    if (legacyByKey.has(key)) {
      mismatches.push(
        mismatch('duplicate-key', `${input.path}[${key}]`, key, undefined, input.legacyProvenance, input.v3Provenance),
      );
    } else {
      legacyByKey.set(key, value);
    }
  }

  for (const value of input.v3) {
    const key = input.v3Key(value);
    if (v3ByKey.has(key)) {
      mismatches.push(
        mismatch('duplicate-key', `${input.path}[${key}]`, undefined, key, input.legacyProvenance, input.v3Provenance),
      );
    } else {
      v3ByKey.set(key, value);
    }
  }

  for (const [key, legacyValue] of legacyByKey) {
    const v3Value = v3ByKey.get(key);
    if (v3Value === undefined) {
      mismatches.push(
        mismatch('missing-in-v3', `${input.path}[${key}]`, key, undefined, input.legacyProvenance, input.v3Provenance),
      );
    } else {
      mismatches.push(...compareFields(legacyValue, v3Value, `${input.path}[${key}]`, input.fields));
    }
  }

  for (const [key] of v3ByKey) {
    if (!legacyByKey.has(key)) {
      mismatches.push(
        mismatch(
          'unexpected-in-v3',
          `${input.path}[${key}]`,
          undefined,
          key,
          input.legacyProvenance,
          input.v3Provenance,
        ),
      );
    }
  }

  return mismatches;
};

const result = <TLegacy, TV3>(
  input: Readonly<{
    routeId: WooV3PublicParityRouteId;
    authoritative: TLegacy;
    observed: TV3;
    legacyCanonical: string;
    v3Canonical: string;
    mismatches: readonly WooV3PublicParityMismatch[];
    blockers?: readonly WooV3PublicParityBlocker[];
  }>,
): WooV3PublicParityResult<TLegacy, TV3> => {
  const blockers = Object.freeze([...(input.blockers ?? [])]);
  const mismatches = Object.freeze([...input.mismatches]);
  return Object.freeze({
    status: blockers.length > 0 ? 'blocked' : mismatches.length > 0 ? 'mismatch' : 'match',
    routeId: input.routeId,
    authoritative: input.authoritative,
    observed: input.observed,
    legacyCanonical: input.legacyCanonical,
    v3Canonical: input.v3Canonical,
    mismatches,
    blockers,
    storeAction: 'retain-legacy',
  });
};

const canonicalMarket = (market: Market) => ({
  id: market.id,
  symbol: market.symbol,
  base: market.base,
  quote: market.quote,
  active: market.active,
  precision: { amount: market.precision.amount, price: market.precision.price },
  limits: {
    amount: { min: market.limits.amount.min, max: market.limits.amount.max },
    leverage: { min: market.limits.leverage.min, max: market.limits.leverage.max },
  },
});

export const canonicalizeWooV3Markets = (markets: readonly Market[]): string =>
  canonicalString([...markets].sort((a, b) => a.symbol.localeCompare(b.symbol)).map(canonicalMarket));

const marketFields: readonly FieldComparison<Market, Market>[] = [
  {
    path: 'id',
    legacy: (market) => market.id,
    v3: (market) => market.id,
    legacyProvenance: 'v1 symbol split + USDT contract ID',
    v3Provenance: 'V3 baseAsset/quoteAsset',
  },
  {
    path: 'symbol',
    legacy: (market) => market.symbol,
    v3: (market) => market.symbol,
    legacyProvenance: 'v1 symbol',
    v3Provenance: 'V3 symbol/baseAsset/quoteAsset',
  },
  {
    path: 'base',
    legacy: (market) => market.base,
    v3: (market) => market.base,
    legacyProvenance: 'v1 symbol base',
    v3Provenance: 'V3 baseAsset',
  },
  {
    path: 'quote',
    legacy: (market) => market.quote,
    v3: (market) => market.quote,
    legacyProvenance: 'v1 symbol quote',
    v3Provenance: 'V3 quoteAsset',
  },
  {
    path: 'active',
    legacy: (market) => market.active,
    v3: (market) => market.active,
    legacyProvenance: 'v1 supported PERP row',
    v3Provenance: 'V3 status when present',
  },
  {
    path: 'precision.amount',
    legacy: (market) => market.precision.amount,
    v3: (market) => market.precision.amount,
    legacyProvenance: 'v1 base_tick',
    v3Provenance: 'V3 baseTick',
  },
  {
    path: 'precision.price',
    legacy: (market) => market.precision.price,
    v3: (market) => market.precision.price,
    legacyProvenance: 'v1 quote_tick',
    v3Provenance: 'V3 quoteTick',
  },
  {
    path: 'limits.amount.min',
    legacy: (market) => market.limits.amount.min,
    v3: (market) => market.limits.amount.min,
    legacyProvenance: 'v1 base_min',
    v3Provenance: 'V3 baseMin',
  },
  {
    path: 'limits.amount.max',
    legacy: (market) => market.limits.amount.max,
    v3: (market) => market.limits.amount.max,
    legacyProvenance: 'v1 base_max',
    v3Provenance: 'V3 baseMax',
  },
  {
    path: 'limits.leverage.min',
    legacy: (market) => market.limits.leverage.min,
    v3: (market) => market.limits.leverage.min,
    legacyProvenance: 'safe-cex legacy constant',
    v3Provenance: 'safe-cex V3 constant',
  },
  {
    path: 'limits.leverage.max',
    legacy: (market) => market.limits.leverage.max,
    v3: (market) => market.limits.leverage.max,
    legacyProvenance: 'floor(1 / v1 base_imr)',
    v3Provenance: 'floor(1 / V3 baseIMR)',
  },
];

export const compareWooV3Markets = (
  legacyMarkets: readonly Market[],
  v3Markets: readonly Market[],
): WooV3PublicParityResult<readonly Market[], readonly Market[]> =>
  result({
    routeId: 'markets',
    authoritative: legacyMarkets,
    observed: v3Markets,
    legacyCanonical: canonicalizeWooV3Markets(legacyMarkets),
    v3Canonical: canonicalizeWooV3Markets(v3Markets),
    mismatches: compareKeyed({
      path: 'markets',
      legacy: legacyMarkets,
      v3: v3Markets,
      legacyKey: (market) => market.symbol,
      v3Key: (market) => market.symbol,
      fields: marketFields,
      legacyProvenance: 'GET /v1/public/info mapped perpetual rows',
      v3Provenance: 'GET /v3/public/instruments mapped perpetual rows',
    }),
  });

const canonicalLegacyTickerStats = (ticker: Ticker) => ({
  id: ticker.id,
  symbol: ticker.symbol,
  index: ticker.index,
  mark: ticker.mark,
  percentage: ticker.percentage,
  openInterest: ticker.openInterest,
  fundingRate: ticker.fundingRate,
  fundingTime: ticker.fundingTime,
  volume: ticker.volume,
  quoteVolume: ticker.quoteVolume,
});

const canonicalV3TickerStats = (stats: WooV3FuturesStats) => ({
  id: stats.id,
  symbol: stats.symbol,
  index: stats.index,
  mark: stats.mark,
  percentage: stats.percentage24h,
  openInterest: stats.openInterest,
  fundingRate: stats.estimatedFundingRate,
  fundingTime: stats.fundingTime,
  volume: stats.volume24h,
  quoteVolume: stats.quoteVolume24h,
});

export const canonicalizeWooLegacyTickerStats = (tickers: readonly Ticker[]): string =>
  canonicalString([...tickers].sort((a, b) => a.symbol.localeCompare(b.symbol)).map(canonicalLegacyTickerStats));

export const canonicalizeWooV3TickerStats = (stats: readonly WooV3FuturesStats[]): string =>
  canonicalString([...stats].sort((a, b) => a.symbol.localeCompare(b.symbol)).map(canonicalV3TickerStats));

const tickerFields: readonly FieldComparison<Ticker, WooV3FuturesStats>[] = [
  [
    'id',
    (ticker: Ticker) => ticker.id,
    (stats: WooV3FuturesStats) => stats.id,
    'legacy mapped market',
    'V3 mapped market',
  ],
  ['symbol', (ticker: Ticker) => ticker.symbol, (stats: WooV3FuturesStats) => stats.symbol, 'v1 symbol', 'V3 symbol'],
  [
    'index',
    (ticker: Ticker) => ticker.index,
    (stats: WooV3FuturesStats) => stats.index,
    'v1 index_price',
    'V3 indexPrice',
  ],
  ['mark', (ticker: Ticker) => ticker.mark, (stats: WooV3FuturesStats) => stats.mark, 'v1 mark_price', 'V3 markPrice'],
  [
    'percentage',
    (ticker: Ticker) => ticker.percentage,
    (stats: WooV3FuturesStats) => stats.percentage24h,
    'v1 24h_open/24h_close',
    'V3 24hOpen/24hClose',
  ],
  [
    'openInterest',
    (ticker: Ticker) => ticker.openInterest,
    (stats: WooV3FuturesStats) => stats.openInterest,
    'v1 open_interest',
    'V3 openInterest',
  ],
  [
    'fundingRate',
    (ticker: Ticker) => ticker.fundingRate,
    (stats: WooV3FuturesStats) => stats.estimatedFundingRate,
    'legacy safe-cex fundingRate from estfundingrate stream',
    'V3 estFundingRate',
  ],
  [
    'fundingTime',
    (ticker: Ticker) => ticker.fundingTime,
    (stats: WooV3FuturesStats) => stats.fundingTime,
    'v1 next_funding_time',
    'V3 nextFundingTime',
  ],
  [
    'volume',
    (ticker: Ticker) => ticker.volume,
    (stats: WooV3FuturesStats) => stats.volume24h,
    'v1 24h_volume',
    'V3 24hVolume',
  ],
  [
    'quoteVolume',
    (ticker: Ticker) => ticker.quoteVolume,
    (stats: WooV3FuturesStats) => stats.quoteVolume24h,
    'v1 24h_amount',
    'V3 24hAmount',
  ],
].map(([path, legacy, v3, legacyProvenance, v3Provenance]) => ({
  path: path as string,
  legacy: legacy as (ticker: Ticker) => unknown,
  v3: v3 as (stats: WooV3FuturesStats) => unknown,
  legacyProvenance: legacyProvenance as string,
  v3Provenance: v3Provenance as string,
}));

export const compareWooV3TickerStats = (
  legacyTickers: readonly Ticker[],
  v3Stats: readonly WooV3FuturesStats[],
): WooV3PublicParityResult<readonly Ticker[], readonly WooV3FuturesStats[]> =>
  result({
    routeId: 'tickers',
    authoritative: legacyTickers,
    observed: v3Stats,
    legacyCanonical: canonicalizeWooLegacyTickerStats(legacyTickers),
    v3Canonical: canonicalizeWooV3TickerStats(v3Stats),
    mismatches: compareKeyed({
      path: 'tickers',
      legacy: legacyTickers,
      v3: v3Stats,
      legacyKey: (ticker) => ticker.symbol,
      v3Key: (stats) => stats.symbol,
      fields: tickerFields,
      legacyProvenance: 'GET /v1/public/futures mapped ticker',
      v3Provenance: 'GET /v3/public/futures mapped statistics',
    }),
    blockers: [
      Object.freeze({
        code: 'v3-futures-rest-has-no-live-quotes',
        detail:
          'Legacy REST fills bid/ask/last from 24h_close, while V3 REST has no best-bid, best-ask, or last-trade fields; V3 ticker authority requires coherent BBO and Trade streams.',
      }),
    ],
  });

const canonicalOrderBookLevel = (level: OrderBookOrders) => ({
  price: level.price,
  amount: level.amount,
  total: level.total,
});

const canonicalizeOrderBookValues = (request: WooV3PublicRequestContext, orderBook: OrderBook): string => {
  if (!request.symbol) throw new Error('WOO orderbook request symbol must be non-empty');
  return canonicalString({
    symbol: request.symbol,
    bids: orderBook.bids.map(canonicalOrderBookLevel),
    asks: orderBook.asks.map(canonicalOrderBookLevel),
  });
};

export const canonicalizeWooV3OrderBook = (
  request: WooV3PublicRequestContext,
  snapshot: WooV3MappedOrderBookSnapshot,
): string => {
  // REST snapshots are fetched independently, so their response timestamps
  // are recovery provenance rather than value-parity fields. The typed V3
  // boundary still carries the timestamp for a future WS prevTs join.
  return canonicalizeOrderBookValues(request, snapshot);
};

const compareOrderBookSide = (
  side: 'bids' | 'asks',
  legacyLevels: OrderBook['bids'],
  v3Levels: OrderBook['bids'],
): WooV3PublicParityMismatch[] => {
  const fields: readonly (keyof OrderBookOrders)[] = ['price', 'amount', 'total'];
  const mismatches: WooV3PublicParityMismatch[] = [];
  const length = Math.max(legacyLevels.length, v3Levels.length);
  for (let index = 0; index < length; index += 1) {
    const legacyLevel = legacyLevels[index];
    const v3Level = v3Levels[index];
    if (legacyLevel === undefined || v3Level === undefined) {
      mismatches.push(
        mismatch(
          legacyLevel === undefined ? 'unexpected-in-v3' : 'missing-in-v3',
          `orderbook.${side}[${index}]`,
          legacyLevel === undefined ? undefined : canonicalOrderBookLevel(legacyLevel),
          v3Level === undefined ? undefined : canonicalOrderBookLevel(v3Level),
          'v1 price/quantity normalized with cumulative total',
          'V3 price/quantity normalized with cumulative total',
        ),
      );
      continue;
    }
    for (const field of fields) {
      if (!Object.is(legacyLevel[field], v3Level[field])) {
        mismatches.push(
          mismatch(
            'value',
            `orderbook.${side}[${index}].${field}`,
            legacyLevel[field],
            v3Level[field],
            `v1 ${side}[].${field === 'amount' ? 'quantity' : field}`,
            `V3 ${side}[].${field === 'amount' ? 'quantity' : field}`,
          ),
        );
      }
    }
  }
  return mismatches;
};

export const compareWooV3OrderBook = (
  input: Readonly<{
    legacyRequest: WooV3PublicRequestContext;
    v3Request: WooV3PublicRequestContext;
    legacyOrderBook: OrderBook;
    v3OrderBook: WooV3MappedOrderBookSnapshot;
  }>,
): WooV3PublicParityResult<OrderBook, WooV3MappedOrderBookSnapshot> => {
  const blockers: WooV3PublicParityBlocker[] = [];
  if (!input.legacyRequest.symbol || !input.v3Request.symbol) {
    blockers.push(
      blocker('invalid-request-context', 'Orderbook request symbols must be non-empty', {
        path: 'orderbook.request.symbol',
        expected: 'non-empty exact WOO symbol',
        actual: { legacy: input.legacyRequest.symbol, v3: input.v3Request.symbol },
      }),
    );
  } else if (input.legacyRequest.symbol !== input.v3Request.symbol) {
    blockers.push(
      blocker('request-symbol-mismatch', 'Legacy and V3 orderbook requests target different symbols', {
        path: 'orderbook.request.symbol',
        expected: input.legacyRequest.symbol,
        actual: input.v3Request.symbol,
      }),
    );
  }

  return result({
    routeId: 'orderbook',
    authoritative: input.legacyOrderBook,
    observed: input.v3OrderBook,
    legacyCanonical: input.legacyRequest.symbol
      ? canonicalizeOrderBookValues(input.legacyRequest, input.legacyOrderBook)
      : canonicalString({ request: input.legacyRequest }),
    v3Canonical: input.v3Request.symbol
      ? canonicalizeWooV3OrderBook(input.v3Request, input.v3OrderBook)
      : canonicalString({ request: input.v3Request }),
    mismatches: [
      ...compareOrderBookSide('bids', input.legacyOrderBook.bids, input.v3OrderBook.bids),
      ...compareOrderBookSide('asks', input.legacyOrderBook.asks, input.v3OrderBook.asks),
    ],
    blockers,
  });
};

const canonicalCandle = (candle: Candle) => ({
  timestamp: candle.timestamp,
  open: candle.open,
  high: candle.high,
  low: candle.low,
  close: candle.close,
  volume: candle.volume,
});

const WOO_V3_FIXED_KLINE_INTERVAL_MS = Object.freeze({
  '1m': 60_000,
  '3m': 180_000,
  '5m': 300_000,
  '15m': 900_000,
  '30m': 1_800_000,
  '1h': 3_600_000,
  '2h': 7_200_000,
  '4h': 14_400_000,
  '6h': 21_600_000,
  '12h': 43_200_000,
  '1d': 86_400_000,
  '1w': 604_800_000,
});

type WooV3FixedKlineInterval = keyof typeof WOO_V3_FIXED_KLINE_INTERVAL_MS;
type NormalizedKlineInterval =
  | Readonly<{ status: 'ready'; interval: WooV3FixedKlineInterval; durationMs: number }>
  | Readonly<{ status: 'blocked'; normalized: string; reason: 'variable-calendar-duration' | 'unsupported' }>;

const normalizeKlineInterval = (interval: string): NormalizedKlineInterval => {
  const normalized = interval === '1M' ? '1mon' : interval;
  if (normalized === '1mon' || normalized === '1y') {
    return Object.freeze({ status: 'blocked', normalized, reason: 'variable-calendar-duration' });
  }
  if (Object.hasOwn(WOO_V3_FIXED_KLINE_INTERVAL_MS, normalized)) {
    const fixedInterval = normalized as WooV3FixedKlineInterval;
    return Object.freeze({
      status: 'ready',
      interval: fixedInterval,
      durationMs: WOO_V3_FIXED_KLINE_INTERVAL_MS[fixedInterval],
    });
  }
  return Object.freeze({ status: 'blocked', normalized, reason: 'unsupported' });
};

const canonicalizeCandlesForRequest = (
  request: Readonly<{ symbol: string; interval: WooV3FixedKlineInterval }>,
  candles: readonly Candle[],
): string =>
  canonicalString({
    symbol: request.symbol,
    interval: request.interval,
    candles: [...candles].sort((a, b) => a.timestamp - b.timestamp).map(canonicalCandle),
  });

const unsupportedIntervalBlocker = (
  side: 'legacy' | 'v3',
  interval: string,
  normalized: Extract<NormalizedKlineInterval, { status: 'blocked' }>,
): WooV3PublicParityBlocker =>
  blocker(
    'unsupported-kline-interval',
    normalized.reason === 'variable-calendar-duration'
      ? `WOO ${normalized.normalized} uses variable calendar boundaries; fixed endTimestamp parity is intentionally blocked`
      : `Unsupported WOO kline interval ${interval}`,
    {
      path: `kline.request.${side}.interval`,
      expected: Object.keys(WOO_V3_FIXED_KLINE_INTERVAL_MS),
      actual: interval,
    },
  );

const validateKlineContext = (
  input: Readonly<{
    legacyRequest: WooV3KlineRequestContext;
    v3Request: WooV3KlineRequestContext;
    v3Rows: readonly WooV3KlineRow[];
  }>,
): Readonly<{
  blockers: readonly WooV3PublicParityBlocker[];
  legacyInterval: NormalizedKlineInterval;
  v3Interval: NormalizedKlineInterval;
}> => {
  const blockers: WooV3PublicParityBlocker[] = [];
  const legacyInterval = normalizeKlineInterval(input.legacyRequest.interval);
  const v3Interval = normalizeKlineInterval(input.v3Request.interval);

  if (!input.legacyRequest.symbol || !input.v3Request.symbol) {
    blockers.push(
      blocker('invalid-request-context', 'Kline request symbols must be non-empty', {
        path: 'kline.request.symbol',
        expected: 'non-empty exact WOO symbol',
        actual: { legacy: input.legacyRequest.symbol, v3: input.v3Request.symbol },
      }),
    );
  } else if (input.legacyRequest.symbol !== input.v3Request.symbol) {
    blockers.push(
      blocker('request-symbol-mismatch', 'Legacy and V3 kline requests target different symbols', {
        path: 'kline.request.symbol',
        expected: input.legacyRequest.symbol,
        actual: input.v3Request.symbol,
      }),
    );
  }

  if (legacyInterval.status === 'blocked') {
    blockers.push(unsupportedIntervalBlocker('legacy', input.legacyRequest.interval, legacyInterval));
  }
  if (v3Interval.status === 'blocked') {
    blockers.push(unsupportedIntervalBlocker('v3', input.v3Request.interval, v3Interval));
  }
  if (
    legacyInterval.status === 'ready' &&
    v3Interval.status === 'ready' &&
    legacyInterval.interval !== v3Interval.interval
  ) {
    blockers.push(
      blocker('request-interval-mismatch', 'Legacy and V3 kline requests target different intervals', {
        path: 'kline.request.interval',
        expected: legacyInterval.interval,
        actual: v3Interval.interval,
      }),
    );
  }

  for (const [index, row] of input.v3Rows.entries()) {
    if (row.symbol !== input.v3Request.symbol) {
      blockers.push(
        blocker('v3-kline-row-symbol-mismatch', 'V3 kline row does not match its request symbol', {
          path: `kline.v3Rows[${index}].symbol`,
          expected: input.v3Request.symbol,
          actual: row.symbol,
        }),
      );
    }
    if (v3Interval.status === 'ready' && row.type !== v3Interval.interval) {
      blockers.push(
        blocker('v3-kline-row-interval-mismatch', 'V3 kline row does not match its normalized request interval', {
          path: `kline.v3Rows[${index}].type`,
          expected: v3Interval.interval,
          actual: row.type,
        }),
      );
    }
    if (v3Interval.status === 'ready') {
      const expectedEndTimestamp = row.startTimestamp + v3Interval.durationMs;
      if (!Number.isSafeInteger(expectedEndTimestamp) || row.endTimestamp !== expectedEndTimestamp) {
        blockers.push(
          blocker(
            'v3-kline-row-end-timestamp-mismatch',
            'V3 kline endTimestamp must equal startTimestamp plus the fixed requested interval',
            {
              path: `kline.v3Rows[${index}].endTimestamp`,
              expected: expectedEndTimestamp,
              actual: row.endTimestamp,
            },
          ),
        );
      }
    }
  }

  return Object.freeze({ blockers: Object.freeze(blockers), legacyInterval, v3Interval });
};

export const canonicalizeWooLegacyCandles = (request: WooV3KlineRequestContext, candles: readonly Candle[]): string => {
  const interval = normalizeKlineInterval(request.interval);
  if (!request.symbol || interval.status === 'blocked') {
    throw new Error('WOO legacy kline request cannot form an exact fixed-interval canonical');
  }
  return canonicalizeCandlesForRequest({ symbol: request.symbol, interval: interval.interval }, candles);
};

export const canonicalizeWooV3KlineRows = (
  request: WooV3KlineRequestContext,
  rows: readonly WooV3KlineRow[],
): string => {
  const validation = validateKlineContext({ legacyRequest: request, v3Request: request, v3Rows: rows });
  if (validation.blockers.length > 0 || validation.v3Interval.status === 'blocked') {
    throw new Error(
      `WOO V3 kline rows cannot form an exact canonical: ${validation.blockers.map((item) => item.code).join(',')}`,
    );
  }
  return canonicalizeCandlesForRequest(
    { symbol: request.symbol, interval: validation.v3Interval.interval },
    mapWooV3Candles(rows),
  );
};

const candleFields: readonly FieldComparison<Candle, Candle>[] = (
  ['open', 'high', 'low', 'close', 'volume'] as const
).map((field) => ({
  path: field,
  legacy: (candle) => candle[field],
  v3: (candle) => candle[field],
  legacyProvenance: `v1 ${field}`,
  v3Provenance: `V3 ${field}`,
}));

export const compareWooV3Candles = (
  input: Readonly<{
    routeId: 'recent-klines' | 'historical-klines';
    legacyRequest: WooV3KlineRequestContext;
    v3Request: WooV3KlineRequestContext;
    legacyCandles: readonly Candle[];
    v3Rows: readonly WooV3KlineRow[];
  }>,
): WooV3PublicParityResult<readonly Candle[], readonly WooV3KlineRow[]> => {
  const validation = validateKlineContext(input);
  const v3Candles = mapWooV3Candles(input.v3Rows);
  const legacyCanonical =
    validation.legacyInterval.status === 'ready'
      ? canonicalizeCandlesForRequest(
          { symbol: input.legacyRequest.symbol, interval: validation.legacyInterval.interval },
          input.legacyCandles,
        )
      : canonicalString({ request: input.legacyRequest, candles: input.legacyCandles.map(canonicalCandle) });
  const v3Canonical =
    validation.v3Interval.status === 'ready' && validation.blockers.length === 0
      ? canonicalizeCandlesForRequest(
          { symbol: input.v3Request.symbol, interval: validation.v3Interval.interval },
          v3Candles,
        )
      : canonicalString({
          request: input.v3Request,
          rows: input.v3Rows.map((row) => ({
            symbol: row.symbol,
            type: row.type,
            startTimestamp: row.startTimestamp,
            endTimestamp: row.endTimestamp,
          })),
        });

  return result({
    routeId: input.routeId,
    authoritative: input.legacyCandles,
    observed: input.v3Rows,
    legacyCanonical,
    v3Canonical,
    mismatches: compareKeyed({
      path: 'candles',
      legacy: input.legacyCandles,
      v3: v3Candles,
      legacyKey: (candle) => String(candle.timestamp),
      v3Key: (candle) => String(candle.timestamp),
      fields: candleFields,
      legacyProvenance: input.routeId === 'recent-klines' ? 'GET /v1/kline rows' : 'GET legacy historical kline rows',
      v3Provenance:
        input.routeId === 'recent-klines' ? 'GET /v3/public/kline rows' : 'GET /v3/public/klineHistory rows',
    }),
    blockers: validation.blockers,
  });
};
