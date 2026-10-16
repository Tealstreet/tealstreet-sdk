import type { Candle } from '../../../safe-cex/src/types';
import type { CandleCacheBackend, CandleCacheChunkRecord, CandleCacheSparseChunk } from '../../../safe-cex/src/candleCache/types';

export interface IndexedDBCandleCacheBackendOptions {
  dbName?: string;
  dbVersion?: number;
  storeName?: string;
  maxAgeDays?: number;
  reportError?: (error: unknown) => void;
}

export class IndexedDBCandleCacheBackend implements CandleCacheBackend {
  private dbPromise: Promise<IDBDatabase> | null = null;
  private dbUnavailable = false;
  private readonly dbName: string;
  private readonly dbVersion: number;
  private readonly storeName: string;
  private readonly maxAgeDays: number;
  private readonly reportError?: (error: unknown) => void;

  constructor(options: IndexedDBCandleCacheBackendOptions = {}) {
    this.dbName = options.dbName ?? 'tealstreet-candles';
    this.dbVersion = options.dbVersion ?? 1;
    this.storeName = options.storeName ?? 'chunks';
    this.maxAgeDays = options.maxAgeDays ?? 7;
    this.reportError = options.reportError;
  }

  async loadAllChunks(
    cacheKey: string,
    options: {
      chunkSize: number;
      intervalSeconds: number;
    },
  ): Promise<Map<number, CandleCacheSparseChunk>> {
    const result = new Map<number, CandleCacheSparseChunk>();
    if (this.dbUnavailable || typeof indexedDB === 'undefined') {
      return result;
    }

    try {
      const db = await this.open();
      const tx = db.transaction(this.storeName, 'readonly');
      const store = tx.objectStore(this.storeName);
      const prefix = `${cacheKey}::`;
      const range = IDBKeyRange.bound(prefix, `${prefix}\uffff`);

      await new Promise<void>((resolve, reject) => {
        const request = store.openCursor(range);
        request.onsuccess = () => {
          const cursor = request.result;
          if (!cursor) {
            resolve();
            return;
          }

          const idbKey = cursor.key as string;
          const chunkKey = parseInt(idbKey.slice(prefix.length), 10);
          const record = cursor.value as CandleCacheChunkRecord;
          const sparseArray = new Array<Candle | undefined>(options.chunkSize);

          for (const candle of record.candles) {
            const timeInSeconds = candle.timestamp / 1000;
            const candleIndex = Math.floor(timeInSeconds / options.intervalSeconds);
            const offset = candleIndex % options.chunkSize;
            sparseArray[offset] = candle;
          }

          result.set(chunkKey, sparseArray);
          cursor.continue();
        };
        request.onerror = () => reject(request.error);
      });
    } catch (error) {
      this.handleError(error);
    }

    return result;
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
    if (this.dbUnavailable || typeof indexedDB === 'undefined' || chunkKeys.length === 0) {
      return result;
    }

    try {
      const db = await this.open();
      const tx = db.transaction(this.storeName, 'readonly');
      const store = tx.objectStore(this.storeName);

      await Promise.all(
        chunkKeys.map(
          (chunkKey) =>
            new Promise<void>((resolve, reject) => {
              const request = store.get(`${cacheKey}::${chunkKey}`);
              request.onsuccess = () => {
                const record = request.result as CandleCacheChunkRecord | undefined;
                if (!record) {
                  resolve();
                  return;
                }

                const sparseArray = new Array<Candle | undefined>(options.chunkSize);
                for (const candle of record.candles) {
                  const timeInSeconds = candle.timestamp / 1000;
                  const candleIndex = Math.floor(timeInSeconds / options.intervalSeconds);
                  const offset = candleIndex % options.chunkSize;
                  sparseArray[offset] = candle;
                }

                result.set(chunkKey, sparseArray);
                resolve();
              };
              request.onerror = () => reject(request.error);
            }),
        ),
      );
    } catch (error) {
      this.handleError(error);
    }

    return result;
  }

  async persistChunks(cacheKey: string, chunks: Map<number, CandleCacheSparseChunk>): Promise<void> {
    if (this.dbUnavailable || typeof indexedDB === 'undefined') {
      return;
    }

    try {
      const db = await this.open();
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);
      const now = Date.now();

      for (const [chunkKey, sparseArray] of chunks) {
        const compacted = sparseArray.filter((candle): candle is Candle => candle !== undefined);
        if (compacted.length === 0) {
          continue;
        }

        store.put({ candles: compacted, lastAccessed: now } satisfies CandleCacheChunkRecord, `${cacheKey}::${chunkKey}`);
      }

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });
    } catch (error) {
      this.handleError(error);
    }
  }

  async clear(cacheKey?: string): Promise<void> {
    if (this.dbUnavailable || typeof indexedDB === 'undefined') {
      return;
    }

    try {
      const db = await this.open();
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);

      if (!cacheKey) {
        store.clear();
      } else {
        const prefix = `${cacheKey}::`;
        const range = IDBKeyRange.bound(prefix, `${prefix}\uffff`);
        await new Promise<void>((resolve, reject) => {
          const request = store.openCursor(range);
          request.onsuccess = () => {
            const cursor = request.result;
            if (!cursor) {
              resolve();
              return;
            }

            cursor.delete();
            cursor.continue();
          };
          request.onerror = () => reject(request.error);
        });
      }

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });
    } catch (error) {
      this.handleError(error);
    }
  }

  private open(): Promise<IDBDatabase> {
    if (this.dbUnavailable || typeof indexedDB === 'undefined') {
      return Promise.reject(new Error('IndexedDB unavailable'));
    }

    if (this.dbPromise) {
      return this.dbPromise;
    }

    this.dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
      try {
        const request = indexedDB.open(this.dbName, this.dbVersion);

        request.onupgradeneeded = () => {
          const db = request.result;
          if (!db.objectStoreNames.contains(this.storeName)) {
            db.createObjectStore(this.storeName);
          }
        };

        request.onsuccess = () => {
          const db = request.result;
          resolve(db);
          setTimeout(() => this.evictOldChunks().catch(() => {}), 5000);
        };

        request.onerror = () => {
          this.dbUnavailable = true;
          this.dbPromise = null;
          this.handleError(request.error ?? new Error('IndexedDB open failed'));
          reject(request.error);
        };
      } catch (error) {
        this.dbUnavailable = true;
        this.dbPromise = null;
        this.handleError(error);
        reject(error);
      }
    });

    return this.dbPromise;
  }

  private async evictOldChunks(): Promise<void> {
    if (this.dbUnavailable || typeof indexedDB === 'undefined') {
      return;
    }

    const cutoff = Date.now() - this.maxAgeDays * 24 * 60 * 60 * 1000;
    try {
      const db = await this.open();
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);

      await new Promise<void>((resolve, reject) => {
        const request = store.openCursor();
        request.onsuccess = () => {
          const cursor = request.result;
          if (!cursor) {
            resolve();
            return;
          }

          const record = cursor.value as CandleCacheChunkRecord;
          if (record.lastAccessed < cutoff) {
            cursor.delete();
          }
          cursor.continue();
        };
        request.onerror = () => reject(request.error);
      });

      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
        tx.onabort = () => reject(tx.error);
      });
    } catch (error) {
      this.handleError(error);
    }
  }

  private handleError(error: unknown) {
    this.reportError?.(error);
  }
}
