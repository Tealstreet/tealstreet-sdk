import type { Candle, OHLCVOptions, Timeframe } from '../types';
import type { Exchange } from '../exchanges/base';

export type CandleCacheSparseChunk = Array<Candle | undefined>;

export interface CandleCacheChunkAccessOptions {
  chunkSize: number;
  intervalSeconds: number;
}

export interface CandleCacheBackend {
  loadAllChunks?(
    cacheKey: string,
    options: CandleCacheChunkAccessOptions,
  ): Promise<Map<number, CandleCacheSparseChunk>>;
  loadChunks?(
    cacheKey: string,
    chunkKeys: number[],
    options: CandleCacheChunkAccessOptions,
  ): Promise<Map<number, CandleCacheSparseChunk>>;
  persistChunks(
    cacheKey: string,
    chunks: Map<number, CandleCacheSparseChunk>,
    options: CandleCacheChunkAccessOptions,
  ): Promise<void>;
  clear?(cacheKey?: string): Promise<void>;
}

export type CandleCacheBackendReadMode = 'hydrate-once' | 'read-through';

export interface CandleCacheFetchRequest {
  cacheKey: string;
  interval: Timeframe;
  fromMs: number;
  toMs: number;
  limit: number;
  fetcher: (opts: Omit<OHLCVOptions, 'symbol'>) => Promise<Candle[]>;
  nowMs?: number;
  usePersistentBackend?: boolean;
}

export interface CandleCacheExchangeFetchRequest extends Omit<CandleCacheFetchRequest, 'fetcher'> {
  exchange: Pick<Exchange, 'name' | 'fetchOHLCV' | 'callMethodInWorker'>;
  symbol: string;
  useWorker?: boolean;
}

export interface CandleCacheRangeLookup {
  contiguousForwardCandles: Candle[];
  contiguousBehindCandles: Candle[];
  limit: number;
  from: number;
  to: number;
}

export interface CandleCacheOptions {
  backend?: CandleCacheBackend;
  backendReadMode?: CandleCacheBackendReadMode;
  chunkSize?: number;
  hydrationTimeoutMs?: number;
  maxConcurrentHydrations?: number;
  onHydrationTimeout?: (cacheKey: string) => void;
}

export interface CandleCacheKeyValueStore {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  listKeys(prefix: string): Promise<string[]>;
}

export interface CandleCacheChunkRecord {
  candles: Candle[];
  lastAccessed: number;
}
