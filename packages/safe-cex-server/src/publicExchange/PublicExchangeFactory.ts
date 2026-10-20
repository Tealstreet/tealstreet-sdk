import { createPublicExchange } from '@tealstreet/safe-cex/src';
import type { BaseExchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { RustEngineExchange } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import type { ExchangeName, Market, PublicExchangeOptions } from '@tealstreet/safe-cex/src/types';

import { DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE, getMarketCacheKeys, getUtcHourMarketBucket } from './buckets';
import { hydrateMarketStore } from './hydrateStore';
import { InProcessFallbackMarketLock } from './lock';
import {
  readLatestMarketCacheSnapshot,
  readMarketCacheSnapshot,
  type RedisLikeMarketCacheClient,
  writeMarketCacheSnapshot,
} from './marketCache';
import type { SerializedMarketCacheSnapshot } from './serialization';

export type PublicExchangeFactoryOptions = {
  redis?: RedisLikeMarketCacheClient;
  namespace?: string;
  forceAllProxiesOff?: boolean;
  marketTtlPolicy?: 'quantized-hour';
  staleFallbackMs?: number;
  now?: () => number;
  exchangeOptions?: Partial<Omit<PublicExchangeOptions, 'publicDataOnly' | 'forceAllProxiesOff'>>;
};

export type PublicExchangeHandle = {
  /** Re-read after refresh: a successful Rust metadata refresh replaces the engine. */
  exchange: BaseExchange;
  markets: Market[];
  marketsBucket: string;
  refreshedAt: number;
  forceRefreshMarkets(reason: string): Promise<Market[]>;
  dispose(): Promise<void>;
};

type MutablePublicExchangeHandle = PublicExchangeHandle;

export class PublicExchangeFactory {
  private readonly handles = new Map<ExchangeName, MutablePublicExchangeHandle>();
  private readonly creating = new Map<ExchangeName, Promise<MutablePublicExchangeHandle>>();
  private readonly refreshing = new Map<ExchangeName, { force: boolean; promise: Promise<Market[]> }>();
  private readonly generations = new Map<ExchangeName, number>();
  private readonly candidates = new Map<ExchangeName, Set<BaseExchange>>();
  private readonly closed = new WeakSet<PublicExchangeHandle>();
  private readonly inProcessLock = new InProcessFallbackMarketLock();
  private readonly namespace: string;

  constructor(private readonly options: PublicExchangeFactoryOptions = {}) {
    this.namespace = options.namespace ?? DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE;
  }

  async get(exchangeName: ExchangeName): Promise<PublicExchangeHandle> {
    const bucket = this.currentBucket();
    const handle = await this.getOrCreateHandle(exchangeName);
    this.assertOpen(handle);
    if (handle.marketsBucket === bucket && handle.markets.length > 0) {
      return handle;
    }

    const cached = await this.readCurrentBucket(exchangeName, bucket);
    this.assertOpen(handle);
    if (cached) {
      if (handle.marketsBucket === bucket && handle.markets.length > 0) return handle;
      this.hydrateHandle(handle, cached.markets, cached.bucket, cached.refreshedAt);
      return handle;
    }

    try {
      await this.refreshMarketsForCurrentBucket(exchangeName, false);
      this.assertOpen(handle);
      return handle;
    } catch (error) {
      this.assertOpen(handle);
      const latest = await this.readLatestFallback(exchangeName);
      if (!latest) {
        throw error;
      }
      this.hydrateHandle(handle, latest.markets, latest.bucket, latest.refreshedAt);
      return handle;
    }
  }

  async refreshMarkets(exchangeName: ExchangeName, reason: string): Promise<Market[]> {
    void reason;

    return this.refreshMarketsForCurrentBucket(exchangeName, true);
  }

  private refreshMarketsForCurrentBucket(exchangeName: ExchangeName, forceRefresh: boolean): Promise<Market[]> {
    const pending = this.refreshing.get(exchangeName);
    if (pending) {
      if (!forceRefresh || pending.force) return pending.promise;
      const generation = this.generations.get(exchangeName) ?? 0;
      return pending.promise
        .catch(() => undefined)
        .then(() => {
          if ((this.generations.get(exchangeName) ?? 0) !== generation) throw new Error('Public exchange disposed');
          return this.refreshMarketsForCurrentBucket(exchangeName, true);
        });
    }
    const refresh = this.doRefreshMarkets(exchangeName, forceRefresh).finally(() => {
      if (this.refreshing.get(exchangeName)?.promise === refresh) this.refreshing.delete(exchangeName);
    });
    this.refreshing.set(exchangeName, { force: forceRefresh, promise: refresh });
    return refresh;
  }

  private async doRefreshMarkets(exchangeName: ExchangeName, forceRefresh: boolean): Promise<Market[]> {
    const bucket = this.currentBucket();
    const handle = await this.getOrCreateHandle(exchangeName);
    const lockKey = getMarketCacheKeys(exchangeName, bucket, this.namespace).lockKey;

    return this.inProcessLock.runExclusive(lockKey, async () => {
      this.assertOpen(handle);
      const nativeExchange = handle.exchange instanceof RustEngineExchange ? handle.exchange : undefined;
      const native = Boolean(nativeExchange);
      const latestCurrentBucket = this.options.redis
        ? await readMarketCacheSnapshot(this.options.redis, exchangeName, bucket, this.namespace)
        : null;
      this.assertOpen(handle);
      if (latestCurrentBucket && !forceRefresh) {
        this.hydrateHandle(handle, latestCurrentBucket.markets, bucket, latestCurrentBucket.refreshedAt);
        return latestCurrentBucket.markets;
      }
      // Rust fetchMarkets reads native state. A replacement engine performs a
      // real metadata request while the last successful engine keeps serving.
      const replacement = nativeExchange?.hasStarted === true;
      let candidate: BaseExchange | undefined = replacement ? undefined : handle.exchange;
      try {
        if (!candidate) candidate = await this.createOwnedExchange(exchangeName);
        this.assertOpen(handle);
        if (native) {
          await candidate.start();
        }
        const markets = await candidate.fetchMarkets();
        this.assertOpen(handle);
        if (!markets.length) throw new Error(`No public markets returned for ${exchangeName}`);
        const refreshedAt = this.now();
        if (this.options.redis) {
          await writeMarketCacheSnapshot({
            client: this.options.redis,
            exchange: exchangeName,
            bucket,
            markets,
            namespace: this.namespace,
            refreshedAt,
          });
        }
        this.assertOpen(handle);
        if (replacement) {
          const previous = handle.exchange;
          handle.exchange = candidate;
          this.releaseCandidate(exchangeName, candidate);
          previous.dispose();
        }
        this.hydrateHandle(handle, markets, bucket, refreshedAt);
        return markets;
      } catch (error) {
        if (candidate && native) {
          candidate.dispose();
          this.releaseCandidate(exchangeName, candidate);
        }
        throw error;
      }
    });
  }

  async invalidate(exchangeName: ExchangeName): Promise<void> {
    const handle = this.handles.get(exchangeName);
    if (!handle) return;

    handle.markets = [];
    handle.marketsBucket = '';
    handle.refreshedAt = 0;
  }

  async dispose(exchangeName?: ExchangeName): Promise<void> {
    const names =
      exchangeName === undefined
        ? new Set([...this.handles.keys(), ...this.creating.keys(), ...this.candidates.keys()])
        : new Set([exchangeName]);
    for (const name of names) {
      this.generations.set(name, (this.generations.get(name) ?? 0) + 1);
      const handle = this.handles.get(name);
      if (handle) {
        this.closed.add(handle);
        handle.exchange.dispose();
        this.handles.delete(name);
      }
      for (const candidate of this.candidates.get(name) ?? []) candidate.dispose();
      this.candidates.delete(name);
      this.creating.delete(name);
      this.refreshing.delete(name);
    }
  }

  private getOrCreateHandle(exchangeName: ExchangeName): Promise<MutablePublicExchangeHandle> {
    const existing = this.handles.get(exchangeName);
    if (existing) return Promise.resolve(existing);
    const pending = this.creating.get(exchangeName);
    if (pending) return pending;
    const create = this.createHandle(exchangeName).finally(() => {
      if (this.creating.get(exchangeName) === create) this.creating.delete(exchangeName);
    });
    this.creating.set(exchangeName, create);
    return create;
  }

  private async createHandle(exchangeName: ExchangeName): Promise<MutablePublicExchangeHandle> {
    const exchange = await this.createOwnedExchange(exchangeName);
    if (exchange.isDisposed) throw new Error(`Public exchange disposed: ${exchangeName}`);
    const handle: MutablePublicExchangeHandle = {
      exchange,
      markets: [],
      marketsBucket: '',
      refreshedAt: 0,
      forceRefreshMarkets: (reason) => {
        this.assertOpen(handle);
        return this.refreshMarkets(exchangeName, reason);
      },
      dispose: async () => {
        if (!this.closed.has(handle)) await this.dispose(exchangeName);
      },
    };
    this.releaseCandidate(exchangeName, exchange);
    this.handles.set(exchangeName, handle);
    return handle;
  }

  private async createOwnedExchange(exchangeName: ExchangeName): Promise<BaseExchange> {
    const generation = this.generations.get(exchangeName) ?? 0;
    const exchange = await createPublicExchange(exchangeName, {
      ...this.options.exchangeOptions,
      forceAllProxiesOff: this.options.forceAllProxiesOff,
    });
    if ((this.generations.get(exchangeName) ?? 0) !== generation) {
      exchange.dispose();
      throw new Error(`Public exchange disposed: ${exchangeName}`);
    }
    const candidates = this.candidates.get(exchangeName) ?? new Set<BaseExchange>();
    candidates.add(exchange);
    this.candidates.set(exchangeName, candidates);
    return exchange;
  }

  private releaseCandidate(exchangeName: ExchangeName, exchange: BaseExchange): void {
    const candidates = this.candidates.get(exchangeName);
    candidates?.delete(exchange);
    if (!candidates?.size) this.candidates.delete(exchangeName);
  }

  private assertOpen(handle: PublicExchangeHandle): void {
    if (this.closed.has(handle)) throw new Error('Public exchange handle disposed');
  }

  private async readCurrentBucket(
    exchangeName: ExchangeName,
    bucket: string,
  ): Promise<SerializedMarketCacheSnapshot | null> {
    if (!this.options.redis) return null;

    return readMarketCacheSnapshot(this.options.redis, exchangeName, bucket, this.namespace);
  }

  private async readLatestFallback(exchangeName: ExchangeName): Promise<SerializedMarketCacheSnapshot | null> {
    if (!this.options.redis) return null;

    return readLatestMarketCacheSnapshot(this.options.redis, exchangeName, this.namespace);
  }

  private hydrateHandle(
    handle: MutablePublicExchangeHandle,
    markets: Market[],
    marketsBucket: string,
    refreshedAt: number,
  ): void {
    this.assertOpen(handle);
    hydrateMarketStore(handle.exchange.store, markets);
    handle.markets = markets;
    handle.marketsBucket = marketsBucket;
    handle.refreshedAt = refreshedAt;
  }

  private currentBucket(): string {
    return getUtcHourMarketBucket(this.now()).bucket;
  }

  private now(): number {
    return this.options.now?.() ?? Date.now();
  }
}
