import type { ExchangeName, Market, Ticker } from '../../../safe-cex/src/types';

import type {
  PublicdataMarketIndex,
  PublicdataTickerDeltaFrame,
  PublicdataTickerEntry,
  PublicdataTickerExchangeState,
  PublicdataTickerFrame,
  PublicdataTickerMetrics,
  PublicdataTickerSnapshotFrame,
  PublicdataTickerState,
  PublicdataWireTicker,
  PublicdataWireTickerMap,
} from './types';

export const buildPublicdataMarketIndex = (markets: readonly Market[]): PublicdataMarketIndex =>
  new Map(markets.map((market) => [market.id, market]));

export const createEmptyTickerExchangeState = (exchange: ExchangeName): PublicdataTickerExchangeState => ({
  exchange,
  tickersByNativeSymbol: new Map(),
  skippedMissingMarkets: 0,
  removedSymbols: [],
});

export const wireTickerToSafeCexTicker = (market: Market, raw: PublicdataWireTicker): Ticker => ({
  id: market.id,
  symbol: market.symbol,
  bid: raw.b ?? 0,
  ask: raw.a ?? 0,
  last: raw.l ?? 0,
  mark: raw.m ?? 0,
  index: raw.i ?? 0,
  percentage: raw.p ?? 0,
  openInterest: raw.o ?? 0,
  fundingRate: raw.f ?? 0,
  fundingTime: raw.t ?? 0,
  volume: raw.v ?? 0,
  quoteVolume: raw.q ?? 0,
});

export const wireTickerMetrics = (raw: PublicdataWireTicker): PublicdataTickerMetrics => ({
  tradesPerSecond: raw.n,
  tradesPerSecond30s: raw.n3,
  tradesPerSecond60s: raw.n6,
  openInterestUsd: raw.ou,
  openInterestUsdAggregate: raw.oa,
  volumeUsdAggregate: raw.va,
});

export const applyWireTickerDelta = (
  previous: PublicdataTickerEntry,
  raw: PublicdataWireTicker,
): PublicdataTickerEntry => {
  const ticker = previous.ticker;
  return {
    ticker: {
      ...ticker,
      bid: raw.b ?? ticker.bid,
      ask: raw.a ?? ticker.ask,
      last: raw.l ?? ticker.last,
      mark: raw.m ?? ticker.mark,
      index: raw.i ?? ticker.index,
      percentage: raw.p ?? ticker.percentage,
      openInterest: raw.o ?? ticker.openInterest,
      fundingRate: raw.f ?? ticker.fundingRate,
      fundingTime: raw.t ?? ticker.fundingTime,
      volume: raw.v ?? ticker.volume,
      quoteVolume: raw.q ?? ticker.quoteVolume,
    },
    metrics: {
      ...previous.metrics,
      ...(raw.n === undefined ? null : { tradesPerSecond: raw.n }),
      ...(raw.n3 === undefined ? null : { tradesPerSecond30s: raw.n3 }),
      ...(raw.n6 === undefined ? null : { tradesPerSecond60s: raw.n6 }),
      ...(raw.ou === undefined ? null : { openInterestUsd: raw.ou }),
      ...(raw.oa === undefined ? null : { openInterestUsdAggregate: raw.oa }),
      ...(raw.va === undefined ? null : { volumeUsdAggregate: raw.va }),
    },
  };
};

export const applyTickerSnapshotForExchange = ({
  exchange,
  payload,
  marketIndex,
  timestamp,
}: {
  readonly exchange: ExchangeName;
  readonly payload: PublicdataWireTickerMap;
  readonly marketIndex: PublicdataMarketIndex;
  readonly timestamp?: number;
}): PublicdataTickerExchangeState => {
  const tickersByNativeSymbol = new Map<string, PublicdataTickerEntry>();
  let skippedMissingMarkets = 0;

  for (const [nativeSymbol, raw] of Object.entries(payload)) {
    const market = marketIndex.get(nativeSymbol);
    if (!market) {
      skippedMissingMarkets += 1;
      continue;
    }
    tickersByNativeSymbol.set(nativeSymbol, {
      ticker: wireTickerToSafeCexTicker(market, raw),
      metrics: wireTickerMetrics(raw),
    });
  }

  return {
    exchange,
    tickersByNativeSymbol,
    skippedMissingMarkets,
    removedSymbols: [],
    lastSnapshotTime: timestamp,
  };
};

export const applyTickerDeltaForExchange = ({
  previous,
  exchange,
  payload,
  removedSymbols = [],
  marketIndex,
  timestamp,
}: {
  readonly previous?: PublicdataTickerExchangeState;
  readonly exchange: ExchangeName;
  readonly payload: PublicdataWireTickerMap;
  readonly removedSymbols?: readonly string[];
  readonly marketIndex: PublicdataMarketIndex;
  readonly timestamp?: number;
}): PublicdataTickerExchangeState => {
  const base = previous ?? createEmptyTickerExchangeState(exchange);
  const tickersByNativeSymbol = new Map(base.tickersByNativeSymbol);
  let skippedMissingMarkets = base.skippedMissingMarkets;

  for (const [nativeSymbol, raw] of Object.entries(payload)) {
    const existing = tickersByNativeSymbol.get(nativeSymbol);
    if (existing) {
      tickersByNativeSymbol.set(nativeSymbol, applyWireTickerDelta(existing, raw));
      continue;
    }

    const market = marketIndex.get(nativeSymbol);
    if (!market) {
      skippedMissingMarkets += 1;
      continue;
    }
    tickersByNativeSymbol.set(nativeSymbol, {
      ticker: wireTickerToSafeCexTicker(market, raw),
      metrics: wireTickerMetrics(raw),
    });
  }

  for (const nativeSymbol of removedSymbols) {
    tickersByNativeSymbol.delete(nativeSymbol);
  }

  return {
    ...base,
    tickersByNativeSymbol,
    skippedMissingMarkets,
    removedSymbols,
    lastDeltaTime: timestamp,
  };
};

export const applyTickerSnapshotFrame = ({
  state,
  frame,
  exchangeByPublicdataExchange,
  marketIndexes,
}: {
  readonly state: PublicdataTickerState;
  readonly frame: PublicdataTickerSnapshotFrame;
  readonly exchangeByPublicdataExchange: ReadonlyMap<string, ExchangeName>;
  readonly marketIndexes: ReadonlyMap<ExchangeName, PublicdataMarketIndex>;
}): PublicdataTickerState => {
  const next = new Map(state);
  for (const [publicdataExchange, payload] of Object.entries(frame.data)) {
    const exchange = exchangeByPublicdataExchange.get(publicdataExchange);
    if (!exchange) continue;

    const marketIndex = marketIndexes.get(exchange);
    if (!marketIndex) continue;

    next.set(
      exchange,
      applyTickerSnapshotForExchange({
        exchange,
        payload,
        marketIndex,
        timestamp: frame.timestamp,
      }),
    );
  }
  return next;
};

export const applyTickerDeltaFrame = ({
  state,
  frame,
  exchangeByPublicdataExchange,
  marketIndexes,
}: {
  readonly state: PublicdataTickerState;
  readonly frame: PublicdataTickerDeltaFrame;
  readonly exchangeByPublicdataExchange: ReadonlyMap<string, ExchangeName>;
  readonly marketIndexes: ReadonlyMap<ExchangeName, PublicdataMarketIndex>;
}): PublicdataTickerState => {
  const next = new Map(state);
  const publicdataExchangeNames = new Set([...Object.keys(frame.data), ...Object.keys(frame.removed ?? {})]);

  for (const publicdataExchange of publicdataExchangeNames) {
    const exchange = exchangeByPublicdataExchange.get(publicdataExchange);
    if (!exchange) continue;

    const marketIndex = marketIndexes.get(exchange);
    if (!marketIndex) continue;

    next.set(
      exchange,
      applyTickerDeltaForExchange({
        previous: next.get(exchange),
        exchange,
        payload: frame.data[publicdataExchange] ?? {},
        removedSymbols: frame.removed?.[publicdataExchange] ?? [],
        marketIndex,
        timestamp: frame.timestamp,
      }),
    );
  }

  return next;
};

export const applyTickerFrame = ({
  state,
  frame,
  exchangeByPublicdataExchange,
  marketIndexes,
}: {
  readonly state: PublicdataTickerState;
  readonly frame: PublicdataTickerFrame;
  readonly exchangeByPublicdataExchange: ReadonlyMap<string, ExchangeName>;
  readonly marketIndexes: ReadonlyMap<ExchangeName, PublicdataMarketIndex>;
}): PublicdataTickerState => {
  if (frame.type === 'snapshot') {
    return applyTickerSnapshotFrame({
      state,
      frame,
      exchangeByPublicdataExchange,
      marketIndexes,
    });
  }

  return applyTickerDeltaFrame({
    state,
    frame,
    exchangeByPublicdataExchange,
    marketIndexes,
  });
};
