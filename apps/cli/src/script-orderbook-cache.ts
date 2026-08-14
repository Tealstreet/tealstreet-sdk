import type { ExchangeLike } from '@tealstreet/cli-core';
import type { OrderBook } from '@tealstreet/safe-cex/src/types';

import type { ExchangeManager } from './exchange-manager';

const DEFAULT_FIRST_SNAPSHOT_TIMEOUT_MS = 1_500;
const DEFAULT_IDLE_TTL_MS = 60_000;

type CacheEntry = {
  readonly account: string;
  readonly symbol: string;
  readonly exchange: ExchangeLike;
  orderbook?: OrderBook;
  unsubscribe?: () => void;
  idleTimer?: ReturnType<typeof setTimeout>;
  firstSnapshot?: Promise<void>;
  resolveFirstSnapshot?: () => void;
};

const trimOrderbook = (orderbook: OrderBook, depth?: number): OrderBook => {
  const limit = depth && depth > 0 ? Math.floor(depth) : undefined;
  const trimSide = (side: OrderBook['bids']): OrderBook['bids'] => {
    const levels = limit ? side.slice(0, limit) : side.slice();
    return levels.map((level) => ({ ...level }));
  };
  return {
    bids: trimSide(orderbook.bids),
    asks: trimSide(orderbook.asks),
  };
};

const getOrderbookListener = (exchange: ExchangeLike, account: string): NonNullable<ExchangeLike['listenOrderBook']> => {
  if (!exchange.listenOrderBook) {
    throw new Error(`getOrderbook: account '${account}' does not support orderbook subscriptions`);
  }
  return exchange.listenOrderBook.bind(exchange);
};

export class ScriptOrderbookCache {
  private readonly entries = new Map<string, CacheEntry>();

  constructor(
    private readonly exchangeManager: Pick<ExchangeManager, 'getExchange'>,
    private readonly opts: {
      readonly firstSnapshotTimeoutMs?: number;
      readonly idleTtlMs?: number;
    } = {},
  ) {}

  async get(symbol: string, opts: { account: string; depth?: number }): Promise<OrderBook | undefined> {
    const exchange = this.exchangeManager.getExchange(opts.account);
    if (!exchange) return undefined;

    const entry = this.ensureEntry(exchange, opts.account, symbol);
    this.scheduleIdleCleanup(entry);

    if (!entry.orderbook) {
      await this.waitForFirstSnapshot(entry);
    }

    return entry.orderbook ? trimOrderbook(entry.orderbook, opts.depth) : undefined;
  }

  disposeAll(): void {
    for (const entry of this.entries.values()) this.disposeEntry(entry);
    this.entries.clear();
  }

  private ensureEntry(exchange: ExchangeLike, account: string, symbol: string): CacheEntry {
    const key = `${account}\0${symbol}`;
    const existing = this.entries.get(key);
    if (existing) {
      if (existing.exchange === exchange) return existing;
      this.disposeEntry(existing);
      this.entries.delete(key);
    }

    const entry: CacheEntry = { account, symbol, exchange };
    entry.firstSnapshot = new Promise((resolve) => {
      entry.resolveFirstSnapshot = resolve;
    });

    const listen = getOrderbookListener(exchange, account);
    entry.unsubscribe = listen(symbol, (orderbook) => {
      entry.orderbook = trimOrderbook(orderbook);
      entry.resolveFirstSnapshot?.();
      entry.resolveFirstSnapshot = undefined;
      entry.firstSnapshot = undefined;
    });

    this.entries.set(key, entry);
    return entry;
  }

  private async waitForFirstSnapshot(entry: CacheEntry): Promise<void> {
    const firstSnapshot = entry.firstSnapshot;
    if (!firstSnapshot) return;

    const timeoutMs = this.opts.firstSnapshotTimeoutMs ?? DEFAULT_FIRST_SNAPSHOT_TIMEOUT_MS;
    await Promise.race([
      firstSnapshot,
      new Promise<void>((resolve) => {
        setTimeout(resolve, timeoutMs);
      }),
    ]);
  }

  private scheduleIdleCleanup(entry: CacheEntry): void {
    if (entry.idleTimer) clearTimeout(entry.idleTimer);
    const ttlMs = this.opts.idleTtlMs ?? DEFAULT_IDLE_TTL_MS;
    entry.idleTimer = setTimeout(() => {
      this.entries.delete(`${entry.account}\0${entry.symbol}`);
      this.disposeEntry(entry);
    }, ttlMs);
  }

  private disposeEntry(entry: CacheEntry): void {
    if (entry.idleTimer) clearTimeout(entry.idleTimer);
    entry.unsubscribe?.();
    entry.resolveFirstSnapshot?.();
    entry.unsubscribe = undefined;
    entry.resolveFirstSnapshot = undefined;
    entry.firstSnapshot = undefined;
  }
}

const caches = new WeakMap<Pick<ExchangeManager, 'getExchange'>, ScriptOrderbookCache>();

export const getScriptOrderbookCache = (
  exchangeManager: Pick<ExchangeManager, 'getExchange'>,
): ScriptOrderbookCache => {
  let cache = caches.get(exchangeManager);
  if (!cache) {
    cache = new ScriptOrderbookCache(exchangeManager);
    caches.set(exchangeManager, cache);
  }
  return cache;
};
