import uniqBy from 'lodash/uniqBy';

import { getExchangeMetadata } from '../exchangeMetadata';
import type { ExchangeName, Timeframe } from '../types';
import { timeframeToSeconds } from '../types';
import { getCandleStartTime } from '../utils/getCandleStartTime';
import { getNumCandlesInRange } from '../utils/getNumCandlesInRange';
import { normalizeEpochToSeconds } from '../utils/normalizeEpochToSeconds';
import type {
  CandleCacheBackendReadMode,
  CandleCacheExchangeFetchRequest,
  CandleCacheFetchRequest,
  CandleCacheOptions,
  CandleCacheRangeLookup,
  CandleCacheSparseChunk,
} from './types';
import type { CandleCacheBackend } from './types';

export const cleanCandles = <TCandle extends { timestamp: number }>(candles: TCandle[]): TCandle[] =>
  uniqBy(candles, (candle) => candle.timestamp).sort((a, b) => a.timestamp - b.timestamp);

/**
 * Cache key for a candle series.
 *
 * `testnet` is part of the key because a testnet account and a live account
 * share an `ExchangeName` — the environment is a per-account flag, not a
 * separate exchange. Without it, testnet candles and live candles collide in
 * one entry and the chart can render the wrong market's history. This affects
 * every venue offering the testnet checkbox (binance, binancepm, hyperliquid,
 * grvt, bitmex, paradex, asterdex, ...), not just one.
 *
 * Live keys keep their existing shape, so no cached entry is invalidated by
 * introducing this; only testnet accounts get a new namespace.
 */
export const getCandleCacheKey = (
  exchangeName: ExchangeName,
  symbol: string,
  interval: Timeframe,
  testnet?: boolean,
): string => `${exchangeName}${testnet ? '-testnet' : ''}-${symbol}-${interval}`;

export const fetchOHLCVWithExchange = async (
  request: Pick<CandleCacheExchangeFetchRequest, 'exchange' | 'symbol' | 'useWorker'> & {
    interval: Timeframe;
    fromMs: number;
    toMs: number;
    limit: number;
  },
) => {
  const { exchange, symbol, useWorker } = request;
  const ohlcvOptions = {
    symbol,
    interval: request.interval,
    from: request.fromMs,
    to: request.toMs,
    limit: request.limit,
  } as const;
  const shouldUseWorker = useWorker ?? getExchangeMetadata(exchange.name).fetchOHLCVInWorker;

  if (shouldUseWorker) {
    return exchange.callMethodInWorker('fetchOHLCV', ohlcvOptions);
  }

  return exchange.fetchOHLCV(ohlcvOptions);
};

export const createExchangeCandleFetcher = (
  request: Pick<CandleCacheExchangeFetchRequest, 'exchange' | 'symbol' | 'useWorker'>,
) => {
  return (opts: {
    interval: Timeframe;
    from?: number;
    to?: number;
    limit?: number;
  }) =>
    fetchOHLCVWithExchange({
      ...request,
      interval: opts.interval,
      fromMs: opts.from ?? 0,
      toMs: opts.to ?? 0,
      limit: opts.limit ?? 0,
    });
};

export class CandleCache {
  private readonly options: CandleCacheOptions;
  private readonly chunkSize: number;
  private readonly hydrationTimeoutMs: number;
  private readonly maxConcurrentHydrations: number;
  private readonly backend?: CandleCacheBackend;
  private readonly backendReadMode: CandleCacheBackendReadMode;
  private readonly chunkedCandleCache = new Map<string, Map<number, CandleCacheSparseChunk>>();
  private readonly hydrationQueue: Array<() => void> = [];
  private readonly hydratedCacheKeys = new Set<string>();
  private hydrationRunning = 0;
  private cacheVersion = 0;

  constructor(options: CandleCacheOptions = {}) {
    this.options = options;
    this.backend = options.backend;
    this.backendReadMode = options.backendReadMode ?? 'hydrate-once';
    this.chunkSize = options.chunkSize ?? 100;
    this.hydrationTimeoutMs = options.hydrationTimeoutMs ?? 2000;
    this.maxConcurrentHydrations = options.maxConcurrentHydrations ?? 2;
  }

  getVersion(): number {
    return this.cacheVersion;
  }

  reset(options: { clearPersisted?: boolean; cacheKey?: string } = {}): void {
    const { clearPersisted = false, cacheKey } = options;

    if (cacheKey) {
      this.chunkedCandleCache.delete(cacheKey);
      this.hydratedCacheKeys.delete(cacheKey);
    } else {
      this.chunkedCandleCache.clear();
      this.hydratedCacheKeys.clear();
    }

    this.cacheVersion++;

    if (clearPersisted && this.backend?.clear) {
      this.backend.clear(cacheKey).catch(() => {});
    }
  }

  getCachedCandles(
    cacheKey: string,
    interval: Timeframe,
    fromMs: number,
    toMs: number,
    limit: number,
    nowMs = Date.now(),
  ): CandleCacheRangeLookup {
    fromMs = getCandleStartTime(interval, normalizeEpochToSeconds(fromMs)) * 1000;

    const to = getCandleStartTime(interval, normalizeEpochToSeconds(toMs));
    const maxTo = getCandleStartTime(interval, nowMs / 1000);
    if (to > maxTo) {
      // getNumCandlesInRange(from, to) is inclusive of both endpoints. We still request
      // the maxTo boundary candle itself, so only candles strictly past maxTo are clipped.
      // Previously this passed (to, maxTo) — with from > to the helper short-circuits to
      // 0, silently disabling the limit cap and over-counting future-clipped candles.
      limit = limit - (getNumCandlesInRange(interval, maxTo, to) - 1);
      toMs = maxTo * 1000;
    } else {
      toMs = to * 1000;
    }

    const chunkMap = this.chunkedCandleCache.get(cacheKey);
    if (!chunkMap || chunkMap.size === 0) {
      return {
        contiguousForwardCandles: [],
        contiguousBehindCandles: [],
        limit,
        from: fromMs,
        to: toMs,
      };
    }

    const intervalSeconds = timeframeToSeconds[interval];
    const contiguousBehindCandles: CandleCacheRangeLookup['contiguousBehindCandles'] = [];

    let currentMs = fromMs;
    while (currentMs <= toMs && limit > 0) {
      const { chunkKey, offset } = this.getChunkInfo(currentMs / 1000, intervalSeconds);
      const chunk = chunkMap.get(chunkKey);
      const cachedCandle = chunk?.[offset];

      if (!cachedCandle) {
        break;
      }

      contiguousBehindCandles.push(cachedCandle);
      currentMs += intervalSeconds * 1000;
      limit -= 1;
    }

    const fromRemaining = currentMs;
    const contiguousForwardCandles: CandleCacheRangeLookup['contiguousForwardCandles'] = [];

    if (currentMs <= toMs && limit > 0) {
      const forwardCandles: CandleCacheRangeLookup['contiguousForwardCandles'] = [];
      let endMs = toMs;

      while (endMs >= currentMs && limit > 0) {
        const { chunkKey, offset } = this.getChunkInfo(endMs / 1000, intervalSeconds);
        const chunk = chunkMap.get(chunkKey);
        const cachedCandle = chunk?.[offset];

        if (!cachedCandle) {
          break;
        }

        forwardCandles.push(cachedCandle);
        endMs -= intervalSeconds * 1000;
        limit -= 1;
      }

      contiguousForwardCandles.push(...forwardCandles.reverse());
      toMs = endMs;
    }

    return {
      contiguousForwardCandles,
      contiguousBehindCandles,
      limit,
      from: fromRemaining,
      to: toMs,
    };
  }

  async fetch(request: CandleCacheFetchRequest) {
    const { cacheKey, interval, fetcher, nowMs = Date.now(), usePersistentBackend = true } = request;
    const intervalSeconds = timeframeToSeconds[interval];
    const normalizedRange = this.normalizeRequestRange(interval, request.fromMs, request.toMs, request.limit, nowMs);

    if (usePersistentBackend && this.backend) {
      await this.ensureBackendStateForRequest(cacheKey, interval, normalizedRange.from, normalizedRange.to);
    }

    const { contiguousForwardCandles, contiguousBehindCandles, limit, from, to } = this.getCachedCandles(
      cacheKey,
      interval,
      request.fromMs,
      request.toMs,
      request.limit,
      nowMs,
    );

    const newCandles =
      limit <= 0 || from > to
        ? []
        : await fetcher({
            interval,
            from,
            to,
            limit,
          });

    if (newCandles.length > 0) {
      const currentCandleStart = getCandleStartTime(interval, nowMs / 1000);
      let chunkMap = this.chunkedCandleCache.get(cacheKey);
      const touchedChunks = new Map<number, CandleCacheSparseChunk>();

      if (!chunkMap) {
        chunkMap = new Map<number, CandleCacheSparseChunk>();
        this.chunkedCandleCache.set(cacheKey, chunkMap);
      }

      for (const candle of newCandles) {
        const timeInSeconds = normalizeEpochToSeconds(candle.timestamp);
        if (timeInSeconds >= currentCandleStart) {
          continue;
        }

        const { chunkKey, offset } = this.getChunkInfo(timeInSeconds, intervalSeconds);
        let chunk = chunkMap.get(chunkKey);
        if (!chunk) {
          chunk = new Array(this.chunkSize);
          chunkMap.set(chunkKey, chunk);
        }

        chunk[offset] = {
          ...candle,
          timestamp: timeInSeconds * 1000,
        };
        touchedChunks.set(chunkKey, chunk);
      }

      if (usePersistentBackend && this.backend && touchedChunks.size > 0) {
        this.backend
          .persistChunks(cacheKey, touchedChunks, {
            chunkSize: this.chunkSize,
            intervalSeconds,
          })
          .catch(() => {});
      }
    }

    return cleanCandles([...contiguousBehindCandles, ...newCandles, ...contiguousForwardCandles]);
  }

  async fetchWithExchange(request: CandleCacheExchangeFetchRequest) {
    const { exchange, symbol, useWorker, ...cacheRequest } = request;

    return this.fetch({
      ...cacheRequest,
      fetcher: (opts) =>
        fetchOHLCVWithExchange({
          exchange,
          symbol,
          useWorker,
          interval: opts.interval,
          fromMs: opts.from ?? 0,
          toMs: opts.to ?? 0,
          limit: opts.limit ?? 0,
        }),
    });
  }

  private getChunkInfo(timestampInSeconds: number, intervalSeconds: number): { chunkKey: number; offset: number } {
    const candleIndex = Math.floor(timestampInSeconds / intervalSeconds);
    const chunkKey = Math.floor(candleIndex / this.chunkSize);
    const offset = candleIndex % this.chunkSize;
    return { chunkKey, offset };
  }

  private normalizeRequestRange(interval: Timeframe, fromMs: number, toMs: number, limit: number, nowMs = Date.now()) {
    fromMs = getCandleStartTime(interval, normalizeEpochToSeconds(fromMs)) * 1000;

    const to = getCandleStartTime(interval, normalizeEpochToSeconds(toMs));
    const maxTo = getCandleStartTime(interval, nowMs / 1000);
    if (to > maxTo) {
      // See getCachedCandles for the (maxTo, to) ordering rationale: getNumCandlesInRange
      // is inclusive of both endpoints, and maxTo itself remains a requested boundary candle.
      limit = limit - (getNumCandlesInRange(interval, maxTo, to) - 1);
      toMs = maxTo * 1000;
    } else {
      toMs = to * 1000;
    }

    return { from: fromMs, to: toMs, limit };
  }

  private getChunkKeysForRange(fromMs: number, toMs: number, intervalSeconds: number) {
    if (fromMs > toMs) {
      return [];
    }

    const startChunk = this.getChunkInfo(fromMs / 1000, intervalSeconds).chunkKey;
    const endChunk = this.getChunkInfo(toMs / 1000, intervalSeconds).chunkKey;
    const chunkKeys: number[] = [];

    for (let chunkKey = startChunk; chunkKey <= endChunk; chunkKey += 1) {
      chunkKeys.push(chunkKey);
    }

    return chunkKeys;
  }

  private mergeChunks(
    cacheKey: string,
    chunks: Map<number, CandleCacheSparseChunk>,
    options: { existingCandlesWin?: boolean } = {},
  ) {
    if (chunks.size === 0) {
      return;
    }

    let existing = this.chunkedCandleCache.get(cacheKey);
    if (!existing) {
      existing = new Map<number, CandleCacheSparseChunk>();
      this.chunkedCandleCache.set(cacheKey, existing);
    }

    for (const [chunkKey, chunk] of chunks) {
      const existingChunk = existing.get(chunkKey);
      if (!options.existingCandlesWin || !existingChunk) {
        existing.set(chunkKey, chunk);
        continue;
      }

      for (let offset = 0; offset < chunk.length; offset += 1) {
        if (existingChunk[offset] === undefined && chunk[offset] !== undefined) {
          existingChunk[offset] = chunk[offset];
        }
      }
    }
  }

  private async ensureBackendStateForRequest(cacheKey: string, interval: Timeframe, fromMs: number, toMs: number): Promise<void> {
    if (!this.backend) {
      return;
    }

    if (this.backendReadMode === 'read-through') {
      const intervalSeconds = timeframeToSeconds[interval];
      const chunkKeys = this.getChunkKeysForRange(fromMs, toMs, intervalSeconds);
      if (chunkKeys.length === 0) {
        return;
      }

      const chunks = await this.loadChunksFromBackend(cacheKey, chunkKeys, intervalSeconds);
      this.mergeChunks(cacheKey, chunks);
      return;
    }

    await this.queuedHydrate(cacheKey, interval);
  }

  private async loadChunksFromBackend(cacheKey: string, chunkKeys: number[], intervalSeconds: number) {
    if (!this.backend || chunkKeys.length === 0) {
      return new Map<number, CandleCacheSparseChunk>();
    }

    if (this.backend.loadChunks) {
      return this.backend.loadChunks(cacheKey, chunkKeys, {
        chunkSize: this.chunkSize,
        intervalSeconds,
      });
    }

    if (this.backend.loadAllChunks) {
      const allChunks = await this.backend.loadAllChunks(cacheKey, {
        chunkSize: this.chunkSize,
        intervalSeconds,
      });
      return new Map(chunkKeys.flatMap((chunkKey) => {
        const chunk = allChunks.get(chunkKey);
        return chunk ? [[chunkKey, chunk] as const] : [];
      }));
    }

    return new Map<number, CandleCacheSparseChunk>();
  }

  private async hydrateFromBackend(cacheKey: string, interval: Timeframe): Promise<void> {
    if (!this.backend || this.hydratedCacheKeys.has(cacheKey) || !this.backend.loadAllChunks) {
      return;
    }

    const intervalSeconds = timeframeToSeconds[interval];
    const chunks = await this.backend.loadAllChunks(cacheKey, {
      chunkSize: this.chunkSize,
      intervalSeconds,
    });

    if (chunks.size > 0) {
      // A timed-out hydration can finish after fetch() has populated this cache key.
      // Preserve those newer in-memory candles while filling any remaining gaps.
      this.mergeChunks(cacheKey, chunks, { existingCandlesWin: true });
    }

    this.hydratedCacheKeys.add(cacheKey);
  }

  private drainHydrationQueue() {
    while (this.hydrationRunning < this.maxConcurrentHydrations && this.hydrationQueue.length > 0) {
      const next = this.hydrationQueue.shift();
      next?.();
    }
  }

  private async queuedHydrate(cacheKey: string, interval: Timeframe): Promise<void> {
    if (!this.backend || this.chunkedCandleCache.has(cacheKey)) {
      return;
    }

    await new Promise<void>((resolve) => {
      const run = () => {
        this.hydrationRunning++;

        let resolved = false;
        let finished = false;

        const resolvePromise = () => {
          if (resolved) return;
          resolved = true;
          resolve();
        };

        const finish = () => {
          if (finished) return;
          finished = true;
          this.hydrationRunning--;
          this.drainHydrationQueue();
        };

        const timer = setTimeout(() => {
          this.options.onHydrationTimeout?.(cacheKey);
          resolvePromise();
          finish();
        }, this.hydrationTimeoutMs);

        this.hydrateFromBackend(cacheKey, interval)
          .then(() => {
            clearTimeout(timer);
            resolvePromise();
            finish();
          })
          .catch(() => {
            clearTimeout(timer);
            resolvePromise();
            finish();
          });
      };

      if (this.hydrationRunning < this.maxConcurrentHydrations) {
        run();
      } else {
        this.hydrationQueue.push(run);
      }
    });
  }
}
