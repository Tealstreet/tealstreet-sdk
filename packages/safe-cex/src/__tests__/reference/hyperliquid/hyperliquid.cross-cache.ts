import type { HyperliquidPlacementRecord } from '../../../exchanges/hyperliquid/hyperliquid.cloid';

import { OrderSide, OrderType } from '../../../types';

// ============================================================================
// CROSS-CONTEXT CACHE FOR HIP-3 ORDERS
// Web workers cannot access sessionStorage, so we use a hybrid approach:
// 1. sessionStorage for main thread (fast, synchronous)
// 2. BroadcastChannel to sync data to workers (async but reliable)
// 3. In-memory fallback cache for workers
// ============================================================================

// Type definitions for cache entries
// `cloid` lets the cloid-record path in the WS handler consume exactly this entry.
export type PrePlacementEntry = { type: OrderType; price: number; amount: number; timestamp: number; cloid?: string };
export type PendingOrderEntry = { type: OrderType; price: number; timestamp: number };

// Storage keys
const PRE_PLACEMENT_STORAGE_KEY = 'tealstreet_hip3_prePlacement';
const PENDING_ORDERS_STORAGE_KEY = 'tealstreet_hip3_pendingOrders';
const PLACEMENT_RECORDS_STORAGE_KEY = 'tealstreet_hl_placementRecords';
// A record is read on the order's first frame, which arrives within a second of
// placement; it lives this long only so a slow or reconnecting socket still finds it.
export const PLACEMENT_RECORD_TTL_MS = 30 * 60 * 1000;
const MAX_PLACEMENT_RECORDS = 500;
const MAX_SEEN_FILLS = 500; // Keep last 500 fill IDs

/**
 * Cross-context cache for HIP-3 order type tracking and fill deduplication.
 * Uses sessionStorage + BroadcastChannel for cross-tab/worker synchronization.
 */
export class HyperliquidCrossCache {
  // In-memory fallback caches for workers (populated via BroadcastChannel)
  private workerPendingOrdersCache = new Map<string, PendingOrderEntry>();
  private workerPrePlacementCache = new Map<string, PrePlacementEntry[]>();
  private workerSeenFillsSet = new Set<string>(); // For fill deduplication
  private placementRecords = new Map<string, HyperliquidPlacementRecord>();

  // BroadcastChannel for cross-context communication
  private orderCacheBroadcast: BroadcastChannel | null = null;

  constructor() {
    this.initializeBroadcastChannel();
    this.clearStaleSessionStorage();
  }

  /**
   * Initialize BroadcastChannel for cross-context communication.
   * Listens for cache updates from other instances and syncs to in-memory caches.
   */
  private initializeBroadcastChannel(): void {
    try {
      if (typeof BroadcastChannel !== 'undefined') {
        this.orderCacheBroadcast = new BroadcastChannel('tealstreet_hip3_order_cache');
        this.orderCacheBroadcast.onmessage = (event) => {
          const { action, key, data } = event.data;
          if (action === 'addPending' && key && data) {
            this.workerPendingOrdersCache.set(key, data);
          } else if (action === 'removePending' && key) {
            this.workerPendingOrdersCache.delete(key);
          } else if (action === 'addPrePlacement' && key && data) {
            // data is the full entries array for this key
            this.workerPrePlacementCache.set(key, Array.isArray(data) ? data : [data]);
          } else if (action === 'removePrePlacement' && key) {
            this.workerPrePlacementCache.delete(key);
          } else if (action === 'addPlacementRecord' && key && data) {
            this.placementRecords.set(key, data);
          } else if (action === 'removePlacementRecord' && key) {
            this.placementRecords.delete(key);
          } else if (action === 'seenFill' && key) {
            // Handle fill deduplication broadcast
            this.workerSeenFillsSet.add(key);
            // Limit size
            if (this.workerSeenFillsSet.size > MAX_SEEN_FILLS) {
              const iterator = this.workerSeenFillsSet.values();
              this.workerSeenFillsSet.delete(iterator.next().value!);
            }
          }
        };
      }
    } catch (_e) {
      // BroadcastChannel not available (e.g., in some worker contexts)
    }
  }

  /**
   * Clear stale caches on startup (prevents issues from leftover data after refresh).
   */
  private clearStaleSessionStorage(): void {
    try {
      const storage = this.getSessionStorage();
      if (storage) {
        // Clear any old fill deduplication data (should be session-scoped, not persistent)
        storage.removeItem('tealstreet_hip3_seenFills');
        // Clear order caches on fresh load (will be repopulated as needed)
        storage.removeItem(PRE_PLACEMENT_STORAGE_KEY);
        storage.removeItem(PENDING_ORDERS_STORAGE_KEY);
        storage.removeItem(PLACEMENT_RECORDS_STORAGE_KEY);
      }
    } catch {
      // Ignore cleanup errors
    }
  }

  /**
   * Safely access sessionStorage (may not exist in workers).
   */
  private getSessionStorage(): Storage | null {
    try {
      return typeof sessionStorage !== 'undefined' ? sessionStorage : null;
    } catch {
      return null;
    }
  }

  /**
   * Broadcast cache update to other instances via BroadcastChannel.
   */
  private broadcastCacheUpdate(action: string, key: string, data?: unknown): void {
    if (this.orderCacheBroadcast) {
      try {
        this.orderCacheBroadcast.postMessage({ action, key, data });
      } catch {
        // Broadcast failed, not critical
      }
    }
  }

  // ============================================================================
  // PRE-PLACEMENT CACHE
  // Stores order details before REST call returns, as a list per coin|side.
  // Matching uses WS data (limitPx, amount) to disambiguate when multiple
  // orders of different types exist for the same coin+side.
  // ============================================================================

  /**
   * Create pre-placement cache key from coin and side.
   */
  private getPrePlacementKey(coin: string, side: OrderSide): string {
    return `${coin}|${side}`;
  }

  /**
   * Read pre-placement cache from sessionStorage (with in-memory fallback for workers).
   */
  private readPrePlacementCache(): Record<string, PrePlacementEntry[]> {
    const storage = this.getSessionStorage();
    if (storage) {
      try {
        const data = storage.getItem(PRE_PLACEMENT_STORAGE_KEY);
        return data ? JSON.parse(data) : {};
      } catch {
        return {};
      }
    }
    // Workers can't access sessionStorage - use in-memory cache populated via broadcast
    const result: Record<string, PrePlacementEntry[]> = {};
    for (const [key, entries] of this.workerPrePlacementCache) {
      result[key] = [...entries];
    }
    return result;
  }

  /**
   * Write pre-placement cache to sessionStorage.
   */
  private writePrePlacementCache(cache: Record<string, PrePlacementEntry[]>): void {
    const storage = this.getSessionStorage();
    if (storage) {
      try {
        storage.setItem(PRE_PLACEMENT_STORAGE_KEY, JSON.stringify(cache));
      } catch {
        // Storage full or unavailable
      }
    }
  }

  /**
   * Add to pre-placement cache before REST call.
   * This allows WS handlers to match incoming orders to their correct type.
   */
  addPrePlacement(coin: string, side: OrderSide, type: OrderType, price: number, amount: number, cloid?: string): void {
    const key = this.getPrePlacementKey(coin, side);
    const entry: PrePlacementEntry = { type, price, amount, timestamp: Date.now(), ...(cloid ? { cloid } : {}) };

    const cache = this.readPrePlacementCache();

    // Append to list for this coin|side
    if (!cache[key]) {
      cache[key] = [];
    }
    cache[key].push(entry);

    // Clean up stale entries across all keys (older than 30 seconds)
    const now = Date.now();
    for (const k of Object.keys(cache)) {
      cache[k] = cache[k].filter((e) => now - e.timestamp <= 30000);
      if (cache[k].length === 0) {
        delete cache[k];
      }
    }

    this.writePrePlacementCache(cache);

    // Also update local in-memory cache and broadcast to workers
    const memEntries = this.workerPrePlacementCache.get(key) || [];
    memEntries.push(entry);
    this.workerPrePlacementCache.set(key, memEntries);
    this.broadcastCacheUpdate('addPrePlacement', key, cache[key]);
  }

  /**
   * Match incoming order to pre-placement cache using WS data for disambiguation.
   *
   * Matching strategy (in priority order):
   * 1. Price match: WS limitPx === cached price (works for Limit orders where limitPx is the order price)
   * 2. Amount match: WS origSz === cached amount (disambiguates TP/SL when amounts differ)
   * 3. FIFO: oldest entry (fallback when price/amount don't uniquely identify)
   *
   * @param coin - The coin identifier (e.g., "VNTLS:VKRAKEN")
   * @param side - Order side (Buy/Sell)
   * @param wsLimitPx - The limitPx from the WS order update (if available)
   * @param wsAmount - The origSz from the WS order update (if available)
   */
  matchPrePlacement(
    coin: string,
    side: OrderSide,
    wsLimitPx?: number,
    wsAmount?: number,
  ): { type: OrderType; price: number } | undefined {
    const key = this.getPrePlacementKey(coin, side);
    const cache = this.readPrePlacementCache();
    const entries = cache[key];
    if (!entries || entries.length === 0) return undefined;

    const now = Date.now();
    // Filter to non-stale entries only
    const validEntries = entries.filter((e) => now - e.timestamp < 30000);
    if (validEntries.length === 0) {
      delete cache[key];
      this.writePrePlacementCache(cache);
      this.workerPrePlacementCache.delete(key);
      this.broadcastCacheUpdate('removePrePlacement', key);
      return undefined;
    }

    let matchIndex = -1;

    // Strategy 1: Price match — for Limit orders, WS limitPx equals the placed price
    if (wsLimitPx !== undefined && matchIndex === -1) {
      matchIndex = validEntries.findIndex((e) => e.price === wsLimitPx);
    }

    // Strategy 2: Amount match — disambiguates when price doesn't match
    // (e.g., TP/SL where cached price is triggerPx but WS limitPx is slippage price)
    if (wsAmount !== undefined && matchIndex === -1) {
      // Only use amount match if it uniquely identifies one entry
      const amountMatches = validEntries.filter((e) => e.amount === wsAmount);
      if (amountMatches.length === 1) {
        matchIndex = validEntries.indexOf(amountMatches[0]);
      }
    }

    // Strategy 3: FIFO — oldest entry (sorted by timestamp, which is insertion order)
    if (matchIndex === -1) {
      matchIndex = 0;
    }

    const matched = validEntries[matchIndex];

    // Remove matched entry from cache
    validEntries.splice(matchIndex, 1);
    if (validEntries.length === 0) {
      delete cache[key];
    } else {
      cache[key] = validEntries;
    }
    this.writePrePlacementCache(cache);

    // Update in-memory cache
    if (validEntries.length === 0) {
      this.workerPrePlacementCache.delete(key);
      this.broadcastCacheUpdate('removePrePlacement', key);
    } else {
      this.workerPrePlacementCache.set(key, validEntries);
      this.broadcastCacheUpdate('addPrePlacement', key, validEntries);
    }

    return { type: matched.type, price: matched.price };
  }

  /**
   * Strict variant for core-perp STOP ENTRIES. `orderUpdates` never carries
   * trigger fields, so a non-reduce-only trigger order's first frame is
   * indistinguishable from a plain limit except through this cache. Unlike
   * `matchPrePlacement` there is no price match (the WS `limitPx` is the
   * slippage cap, never the trigger price) and no FIFO fallback: a plain
   * limit frame must NOT be able to claim a cached stop. Match only a
   * trigger-type entry whose amount equals the frame's `origSz`, and only
   * when exactly one does. Consumes the entry on a hit.
   */
  matchPrePlacementTrigger(coin: string, side: OrderSide, wsAmount: number): { type: OrderType; price: number } | undefined {
    const key = this.getPrePlacementKey(coin, side);
    const cache = this.readPrePlacementCache();
    const entries = cache[key];
    if (!entries || entries.length === 0) return undefined;

    const now = Date.now();
    const validEntries = entries.filter((e) => now - e.timestamp < 30000);
    const candidates = validEntries.filter(
      (e) => (e.type === OrderType.StopLoss || e.type === OrderType.TakeProfit) && e.amount === wsAmount,
    );
    if (candidates.length !== 1) return undefined;

    const matched = candidates[0]!;
    validEntries.splice(validEntries.indexOf(matched), 1);
    if (validEntries.length === 0) {
      delete cache[key];
      this.workerPrePlacementCache.delete(key);
      this.broadcastCacheUpdate('removePrePlacement', key);
    } else {
      cache[key] = validEntries;
      this.workerPrePlacementCache.set(key, validEntries);
      this.broadcastCacheUpdate('addPrePlacement', key, validEntries);
    }
    this.writePrePlacementCache(cache);

    return { type: matched.type, price: matched.price };
  }

  /**
   * Drops the one entry placed with `cloid`, once the WS handler has identified
   * that order from its cloid record. Without this the entry would linger for
   * 30s and could be claimed by a different, cloid-less frame (FIFO fallback).
   */
  removePrePlacementByCloid(coin: string, side: OrderSide, cloid: string): void {
    const key = this.getPrePlacementKey(coin, side);
    const cache = this.readPrePlacementCache();
    const entries = cache[key];
    if (!entries || !entries.some((e) => e.cloid === cloid)) return;

    const remaining = entries.filter((e) => e.cloid !== cloid);
    if (remaining.length === 0) {
      delete cache[key];
      this.workerPrePlacementCache.delete(key);
      this.broadcastCacheUpdate('removePrePlacement', key);
    } else {
      cache[key] = remaining;
      this.workerPrePlacementCache.set(key, remaining);
      this.broadcastCacheUpdate('addPrePlacement', key, remaining);
    }
    this.writePrePlacementCache(cache);
  }

  /**
   * Clear all pre-placement entries for a given coin+side.
   * Called when orders are cancelled to prevent stale entries from poisoning future matches.
   */
  clearPrePlacement(coin: string, side: OrderSide): void {
    const key = this.getPrePlacementKey(coin, side);
    const cache = this.readPrePlacementCache();

    if (cache[key]) {
      delete cache[key];
      this.writePrePlacementCache(cache);
    }

    this.workerPrePlacementCache.delete(key);
    this.broadcastCacheUpdate('removePrePlacement', key);
  }

  // ============================================================================
  // PENDING ORDERS CACHE
  // Stores order details after REST call returns, keyed by order ID
  // ============================================================================

  /**
   * Read pending orders cache from sessionStorage (with in-memory fallback for workers).
   */
  private readPendingOrdersCache(): Record<string, PendingOrderEntry> {
    const storage = this.getSessionStorage();
    if (storage) {
      try {
        const data = storage.getItem(PENDING_ORDERS_STORAGE_KEY);
        return data ? JSON.parse(data) : {};
      } catch {
        return {};
      }
    }
    // Workers can't access sessionStorage - use in-memory cache populated via broadcast
    const result: Record<string, PendingOrderEntry> = {};
    for (const [key, value] of this.workerPendingOrdersCache) {
      result[key] = value;
    }
    return result;
  }

  /**
   * Write pending orders cache to sessionStorage.
   */
  private writePendingOrdersCache(cache: Record<string, PendingOrderEntry>): void {
    const storage = this.getSessionStorage();
    if (!storage) return;
    try {
      storage.setItem(PENDING_ORDERS_STORAGE_KEY, JSON.stringify(cache));
    } catch {
      // Storage full or unavailable
    }
  }

  /**
   * Get pending order from sessionStorage cache.
   * Cleans up stale entries (older than 60 seconds) while reading.
   */
  getPendingOrder(orderId: string): PendingOrderEntry | undefined {
    const cache = this.readPendingOrdersCache();
    const entry = cache[orderId];

    // Clean up old entries while we're here (older than 60 seconds)
    let needsWrite = false;
    const now = Date.now();
    for (const k of Object.keys(cache)) {
      if (now - cache[k].timestamp > 60000) {
        delete cache[k];
        needsWrite = true;
      }
    }
    if (needsWrite) {
      this.writePendingOrdersCache(cache);
    }

    return entry;
  }

  /**
   * Add pending order to sessionStorage cache.
   * Called after REST API returns order ID, before WS confirms.
   */
  addPendingOrder(orderId: string, type: OrderType, price: number): void {
    const entry = { type, price, timestamp: Date.now() };

    // Update sessionStorage (main thread only)
    const cache = this.readPendingOrdersCache();
    cache[orderId] = entry;
    this.writePendingOrdersCache(cache);

    // Also update local in-memory cache and broadcast to workers
    this.workerPendingOrdersCache.set(orderId, entry);
    this.broadcastCacheUpdate('addPending', orderId, entry);
  }

  /**
   * Remove pending order from sessionStorage cache.
   * Called when WS confirms order and it's added to store.
   */
  removePendingOrder(orderId: string): void {
    // Update sessionStorage (main thread only)
    const cache = this.readPendingOrdersCache();
    delete cache[orderId];
    this.writePendingOrdersCache(cache);

    // Also update local in-memory cache and broadcast removal
    this.workerPendingOrdersCache.delete(orderId);
    this.broadcastCacheUpdate('removePending', orderId);
  }

  // ============================================================================
  // PLACEMENT RECORDS (keyed by cloid)
  // Every order we place carries a cloid, and `orderUpdates` echoes it. The
  // record is written before the REST call, so the WS handler identifies the
  // order exactly instead of inferring its type and trigger price. The
  // in-memory map is authoritative in this context (own writes + broadcasts
  // from other contexts); sessionStorage backs a same-tab context created
  // after the write.
  // ============================================================================

  private readPlacementRecordStorage(): Record<string, HyperliquidPlacementRecord> {
    const storage = this.getSessionStorage();
    if (!storage) return {};
    try {
      const data = storage.getItem(PLACEMENT_RECORDS_STORAGE_KEY);
      return data ? JSON.parse(data) : {};
    } catch {
      return {};
    }
  }

  private writePlacementRecordStorage(records: Record<string, HyperliquidPlacementRecord>): void {
    const storage = this.getSessionStorage();
    if (!storage) return;
    try {
      storage.setItem(PLACEMENT_RECORDS_STORAGE_KEY, JSON.stringify(records));
    } catch {
      // Storage full or unavailable
    }
  }

  private isPlacementRecordLive(record: HyperliquidPlacementRecord, now: number): boolean {
    return now - record.placedAt <= PLACEMENT_RECORD_TTL_MS;
  }

  addPlacementRecord(cloid: string, record: HyperliquidPlacementRecord): void {
    const now = Date.now();
    this.placementRecords.set(cloid, record);
    for (const [key, value] of this.placementRecords) {
      if (this.placementRecords.size <= MAX_PLACEMENT_RECORDS && this.isPlacementRecordLive(value, now)) break;
      this.placementRecords.delete(key);
    }

    const stored = this.readPlacementRecordStorage();
    stored[cloid] = record;
    for (const key of Object.keys(stored)) {
      if (!this.isPlacementRecordLive(stored[key], now)) delete stored[key];
    }
    const keys = Object.keys(stored);
    for (let i = 0; i < keys.length - MAX_PLACEMENT_RECORDS; i++) delete stored[keys[i]];
    this.writePlacementRecordStorage(stored);

    this.broadcastCacheUpdate('addPlacementRecord', cloid, record);
  }

  getPlacementRecord(cloid: string): HyperliquidPlacementRecord | undefined {
    const record = this.placementRecords.get(cloid) ?? this.readPlacementRecordStorage()[cloid];
    if (!record) return undefined;
    return this.isPlacementRecordLive(record, Date.now()) ? record : undefined;
  }

  removePlacementRecord(cloid: string): void {
    const inMemory = this.placementRecords.delete(cloid);
    const stored = this.readPlacementRecordStorage();
    if (stored[cloid]) {
      delete stored[cloid];
      this.writePlacementRecordStorage(stored);
    }
    if (inMemory) this.broadcastCacheUpdate('removePlacementRecord', cloid);
  }

  // ============================================================================
  // FILL DEDUPLICATION
  // Prevents duplicate fill notifications across instances within same session
  // NOTE: We do NOT use sessionStorage for fills because it persists across
  // page refreshes, which would cause fills to be incorrectly skipped.
  // We only use in-memory + broadcast (session-scoped, not persistent).
  // ============================================================================

  /**
   * Check if fill has been seen (in-memory only, shared via broadcast).
   */
  hasSeenFill(fillId: string): boolean {
    return this.workerSeenFillsSet.has(fillId);
  }

  /**
   * Mark fill as seen (in-memory + broadcast to workers, NOT persisted).
   */
  markFillSeen(fillId: string): void {
    // Add to in-memory cache
    this.workerSeenFillsSet.add(fillId);

    // Limit in-memory cache size
    if (this.workerSeenFillsSet.size > MAX_SEEN_FILLS) {
      const iterator = this.workerSeenFillsSet.values();
      this.workerSeenFillsSet.delete(iterator.next().value!);
    }

    // Broadcast to other instances (they'll add to their workerSeenFillsSet)
    this.broadcastCacheUpdate('seenFill', fillId);
  }

  /**
   * Clean up resources (close BroadcastChannel).
   */
  dispose(): void {
    if (this.orderCacheBroadcast) {
      try {
        this.orderCacheBroadcast.close();
      } catch {
        // Ignore errors during cleanup
      }
      this.orderCacheBroadcast = null;
    }
  }
}

// ============================================================================
// SINGLETON INSTANCE
// Ensures only one BroadcastChannel per window and shared cache state
// ============================================================================

let singletonInstance: HyperliquidCrossCache | null = null;

/**
 * Get singleton cache instance.
 * Creates instance on first call, returns same instance thereafter.
 */
export function getCrossCache(): HyperliquidCrossCache {
  if (!singletonInstance) {
    singletonInstance = new HyperliquidCrossCache();
  }
  return singletonInstance;
}

// ============================================================================
// BACKWARD COMPATIBILITY EXPORTS
// Maintain existing function API for seamless migration
// ============================================================================

/**
 * Check if fill has been seen (backward compatibility wrapper).
 */
export function hasSeenFill(fillId: string): boolean {
  return getCrossCache().hasSeenFill(fillId);
}

/**
 * Mark fill as seen (backward compatibility wrapper).
 */
export function markFillSeen(fillId: string): void {
  getCrossCache().markFillSeen(fillId);
}
