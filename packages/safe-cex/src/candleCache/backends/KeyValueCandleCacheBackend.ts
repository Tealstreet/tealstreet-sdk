import type { Candle } from '../../types';
import type {
  CandleCacheBackend,
  CandleCacheChunkRecord,
  CandleCacheKeyValueStore,
  CandleCacheSparseChunk,
} from '../types';

export class KeyValueCandleCacheBackend implements CandleCacheBackend {
  constructor(
    private readonly store: CandleCacheKeyValueStore,
    private readonly namespace = 'tealstreet-candles',
  ) {}

  async loadAllChunks(
    cacheKey: string,
    options: {
      chunkSize: number;
      intervalSeconds: number;
    },
  ): Promise<Map<number, CandleCacheSparseChunk>> {
    const prefix = this.getChunkPrefix(cacheKey);
    const keys = await this.store.listKeys(prefix);

    const chunkKeys = keys
      .map((key) => parseInt(key.slice(prefix.length), 10))
      .filter((chunkKey) => Number.isFinite(chunkKey));

    return this.loadChunks(cacheKey, chunkKeys, options);
  }

  async loadChunks(
    cacheKey: string,
    chunkKeys: number[],
    options: {
      chunkSize: number;
      intervalSeconds: number;
    },
  ): Promise<Map<number, CandleCacheSparseChunk>> {
    const result = new Map<number, CandleCacheSparseChunk>();

    await Promise.all(
      chunkKeys.map(async (chunkKey) => {
        const raw = await this.store.get(`${this.getChunkPrefix(cacheKey)}${chunkKey}`);
        if (!raw) {
          return;
        }

        let record: CandleCacheChunkRecord;
        try {
          record = JSON.parse(raw) as CandleCacheChunkRecord;
        } catch {
          return;
        }

        if (!record || !Array.isArray(record.candles)) {
          return;
        }

        const sparseArray = new Array<Candle | undefined>(options.chunkSize);
        let hasMatchingCandle = false;

        for (const candle of record.candles) {
          const timeInSeconds = candle.timestamp / 1000;
          const candleIndex = Math.floor(timeInSeconds / options.intervalSeconds);
          const recomputedChunkKey = Math.floor(candleIndex / options.chunkSize);
          if (recomputedChunkKey !== chunkKey) {
            continue;
          }
          const offset = candleIndex % options.chunkSize;
          sparseArray[offset] = candle;
          hasMatchingCandle = true;
        }

        if (hasMatchingCandle) {
          result.set(chunkKey, sparseArray);
        }
      }),
    );

    return result;
  }

  async persistChunks(cacheKey: string, chunks: Map<number, CandleCacheSparseChunk>): Promise<void> {
    const writes = Array.from(chunks.entries()).map(async ([chunkKey, sparseArray]) => {
      const compacted = sparseArray.filter((candle): candle is Candle => candle !== undefined);
      if (compacted.length === 0) {
        return;
      }

      const key = `${this.getChunkPrefix(cacheKey)}${chunkKey}`;
      const record: CandleCacheChunkRecord = {
        candles: compacted,
        lastAccessed: Date.now(),
      };

      await this.store.set(key, JSON.stringify(record));
    });

    await Promise.all(writes);
  }

  async clear(cacheKey?: string): Promise<void> {
    const prefix = cacheKey ? this.getChunkPrefix(cacheKey) : `${this.namespace}::`;
    const keys = await this.store.listKeys(prefix);
    await Promise.all(keys.map((key) => this.store.delete(key)));
  }

  private getChunkPrefix(cacheKey: string) {
    return `${this.namespace}::${cacheKey}::`;
  }
}
