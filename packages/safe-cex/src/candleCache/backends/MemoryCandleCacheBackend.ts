import type { CandleCacheBackend, CandleCacheSparseChunk } from '../types';

export class MemoryCandleCacheBackend implements CandleCacheBackend {
  private readonly chunksByCacheKey = new Map<string, Map<number, CandleCacheSparseChunk>>();

  async loadAllChunks(cacheKey: string): Promise<Map<number, CandleCacheSparseChunk>> {
    const existing = this.chunksByCacheKey.get(cacheKey);
    if (!existing) {
      return new Map();
    }

    return new Map(Array.from(existing.entries(), ([chunkKey, chunk]) => [chunkKey, [...chunk]]));
  }

  async loadChunks(cacheKey: string, chunkKeys: number[]): Promise<Map<number, CandleCacheSparseChunk>> {
    const existing = this.chunksByCacheKey.get(cacheKey);
    if (!existing) {
      return new Map();
    }

    return new Map(
      chunkKeys.flatMap((chunkKey) => {
        const chunk = existing.get(chunkKey);
        return chunk ? [[chunkKey, [...chunk]] as const] : [];
      }),
    );
  }

  async persistChunks(cacheKey: string, chunks: Map<number, CandleCacheSparseChunk>): Promise<void> {
    const existing = this.chunksByCacheKey.get(cacheKey) ?? new Map<number, CandleCacheSparseChunk>();

    for (const [chunkKey, chunk] of chunks) {
      existing.set(chunkKey, [...chunk]);
    }

    this.chunksByCacheKey.set(cacheKey, existing);
  }

  async clear(cacheKey?: string): Promise<void> {
    if (cacheKey) {
      this.chunksByCacheKey.delete(cacheKey);
      return;
    }

    this.chunksByCacheKey.clear();
  }
}
