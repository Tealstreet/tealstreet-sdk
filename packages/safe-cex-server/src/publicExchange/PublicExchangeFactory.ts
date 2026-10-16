import { createPublicExchange } from '@tealstreet/safe-cex/src';
import type { BaseExchange } from '@tealstreet/safe-cex/src/exchanges/base';
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
  private readonly inProcessLock = new InProcessFallbackMarketLock();
  private readonly namespace: string;

  constructor(private readonly options: PublicExchangeFactoryOptions = {}) {
    this.namespace = options.namespace ?? DEFAULT_PUBLIC_EXCHANGE_CACHE_NAMESPACE;
  }

  async get(exchangeName: ExchangeName): Promise<PublicExchangeHandle> {
    const bucket = this.currentBucket();
    const handle = await this.getOrCreateHandle(exchangeName);
    if (handle.marketsBucket === bucket && handle.markets.length > 0) {
      return handle;
    }

    const cached = await this.readCurrentBucket(exchangeName, bucket);
    if (cached) {
      this.hydrateHandle(handle, cached.markets, cached.bucket, cached.refreshedAt);
      return handle;
    }

    try {
      await this.refreshMarketsForCurrentBucket(exchangeName, false);
      return handle;
    } catch (error) {
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

  private async refreshMarketsForCurrentBucket(exchangeName: ExchangeName, forceRefresh: boolean): Promise<Market[]> {
    const bucket = this.currentBucket();
    const handle = await this.getOrCreateHandle(exchangeName);
    const lockKey = getMarketCacheKeys(exchangeName, bucket, this.namespace).lockKey;

    const refreshed = await this.inProcessLock.runExclusive(lockKey, async () => {
      const latestCurrentBucket = this.options.redis
        ? await readMarketCacheSnapshot(this.options.redis, exchangeName, bucket, this.namespace)
        : null;
      if (latestCurrentBucket && !forceRefresh) {
        return {
          markets: latestCurrentBucket.markets,
          refreshedAt: latestCurrentBucket.refreshedAt,
        };
      }

      const fetchedMarkets = await handle.exchange.fetchMarkets();
      const refreshedAt = this.now();
      if (this.options.redis) {
        await writeMarketCacheSnapshot({
          client: this.options.redis,
          exchange: exchangeName,
          bucket,
          markets: fetchedMarkets,
          namespace: this.namespace,
          refreshedAt,
        });
      }
      return {
        markets: fetchedMarkets,
        refreshedAt,
      };
    });

    this.hydrateHandle(handle, refreshed.markets, bucket, refreshed.refreshedAt);
    return refreshed.markets;
  }

  async invalidate(exchangeName: ExchangeName): Promise<void> {
    const handle = this.handles.get(exchangeName);
    if (!handle) return;

    handle.markets = [];
    handle.marketsBucket = '';
    handle.refreshedAt = 0;
  }

  async dispose(exchangeName?: ExchangeName): Promise<void> {
    const handles =
      exchangeName === undefined
        ? Array.from(this.handles.entries())
        : [[exchangeName, this.handles.get(exchangeName)] as const];

    for (const [name, handle] of handles) {
      if (!handle) continue;
      handle.exchange.dispose();
      this.handles.delete(name);
    }
  }

  private async getOrCreateHandle(exchangeName: ExchangeName): Promise<MutablePublicExchangeHandle> {
    const existing = this.handles.get(exchangeName);
    if (existing) return existing;

    const exchange = await createPublicExchange(exchangeName, {
      ...this.options.exchangeOptions,
      forceAllProxiesOff: this.options.forceAllProxiesOff,
    });

    const handle: MutablePublicExchangeHandle = {
      exchange,
      markets: [],
      marketsBucket: '',
      refreshedAt: 0,
      forceRefreshMarkets: (reason) => this.refreshMarkets(exchangeName, reason),
      dispose: async () => {
        exchange.dispose();
        this.handles.delete(exchangeName);
      },
    };
    this.handles.set(exchangeName, handle);
    return handle;
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
