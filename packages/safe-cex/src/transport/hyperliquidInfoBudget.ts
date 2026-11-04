import { HttpPreparationError } from '@tealstreet/safe-cex-wasm/host';

// Leave 200 of the documented 1,200 weight/minute for exchange actions and other clients.
const MS_PER_WEIGHT = 60;
const MAX_PAUSE_MS = 300_000;
const STORAGE_TIMEOUT_MS = 1500;
type BudgetState = { nextAt: number; pausedUntil: number; generation: number; failures: number };
const emptyState = (): BudgetState => ({ nextAt: 0, pausedUntil: 0, generation: 0, failures: 0 });
let warned = false;
export function warnHyperliquidInfoBudgetUnavailable(): void {
  if (warned) return;
  warned = true;
  try {
    console.warn('Hyperliquid /info budget storage is unavailable');
  } catch {
    /* Diagnostics cannot interrupt a venue read. */
  }
}

function boundedState(raw: Partial<BudgetState> = {}): BudgetState {
  const now = Date.now();
  const timestamp = (value: number | undefined) =>
    typeof value === 'number' && Number.isFinite(value) ? Math.max(now, Math.min(value, now + MAX_PAUSE_MS)) : now;
  return {
    nextAt: timestamp(raw.nextAt),
    pausedUntil: timestamp(raw.pausedUntil),
    generation: Number.isSafeInteger(raw.generation) && raw.generation! >= 0 ? raw.generation! : 0,
    failures:
      typeof raw.failures === 'number' && Number.isFinite(raw.failures) ? Math.max(0, Math.min(5, raw.failures)) : 0,
  };
}

function storageAttempt<T>(
  start: (done: (value: T) => boolean, fail: (error?: unknown) => void) => (() => void) | void,
): Promise<T> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let cancel: (() => void) | void;
    const fail = (error?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try {
        cancel?.();
      } catch {
        /* A failed storage operation may already be closed. */
      }
      reject(error ?? new Error('Hyperliquid budget storage unavailable'));
    };
    const timer = setTimeout(() => fail(), STORAGE_TIMEOUT_MS);
    try {
      cancel = start((value) => {
        if (settled) return false;
        settled = true;
        clearTimeout(timer);
        resolve(value);
        return true;
      }, fail);
    } catch (error) {
      fail(error);
    }
  });
}

export interface InfoBudgetStorage {
  update<T>(key: string, change: (state: BudgetState) => T): Promise<T>;
}

/** One origin database coordinates separate account workers without retaining account data. */
export class IndexedInfoBudgetStorage implements InfoBudgetStorage {
  private database?: Promise<IDBDatabase>;
  private degraded = false;
  private readonly snapshots = new Map<string, BudgetState>();
  constructor(private readonly factory: IDBFactory) {}
  private open(): Promise<IDBDatabase> {
    if (this.database) return this.database;
    const attempt = storageAttempt<IDBDatabase>((done, fail) => {
      const open = this.factory.open('safe-cex-hyperliquid-info-budget', 1);
      let expired = false;
      open.onupgradeneeded = () => {
        try {
          if (expired) open.transaction?.abort();
          else open.result.createObjectStore('budgets');
        } catch (error) {
          fail(error);
        }
      };
      open.onsuccess = () => {
        if (!done(open.result)) open.result.close();
      };
      open.onerror = () => fail(open.error);
      open.onblocked = () => fail();
      return () => {
        expired = true;
      };
    });
    this.database = attempt;
    void attempt.catch(() => {
      if (this.database === attempt) this.database = undefined;
    });
    return attempt;
  }
  async update<T>(key: string, change: (state: BudgetState) => T): Promise<T> {
    const local = () => {
      const snapshot = this.snapshots.get(key);
      if (snapshot) {
        memoryStorage.remember(key, snapshot);
        this.snapshots.delete(key);
      }
      return memoryStorage.update(key, change);
    };
    if (this.degraded) return local();
    let callbackFailed = false;
    let callbackError: unknown;
    try {
      const database = await this.open();
      if (this.degraded) return local();
      return await storageAttempt<T>((done, fail) => {
        const transaction = database.transaction('budgets', 'readwrite');
        const store = transaction.objectStore('budgets');
        const read = store.get(key);
        let value: T;
        let expired = false;
        read.onsuccess = () => {
          if (expired) return;
          try {
            const state = boundedState(read.result ?? emptyState());
            const changed = { ...state };
            try {
              value = change(changed);
            } catch (error) {
              callbackFailed = true;
              callbackError = error;
            }
            // Commit clock normalization even when admission refuses this read.
            const saved = callbackFailed ? state : changed;
            this.snapshots.set(key, { ...saved });
            store.put(saved, key);
          } catch (error) {
            fail(error);
          }
        };
        transaction.oncomplete = () => (callbackFailed ? fail(callbackError) : done(value));
        transaction.onabort = transaction.onerror = () => fail(transaction.error);
        return () => {
          expired = true;
          transaction.abort();
        };
      });
    } catch (error) {
      // Deadline/abort refusals are admission decisions, never storage failures.
      if (callbackFailed && error === callbackError) throw error;
      this.degraded = true;
      void this.database?.then((database) => database.close()).catch(() => undefined);
      this.database = undefined;
      warnHyperliquidInfoBudgetUnavailable();
      if (callbackFailed) throw callbackError;
      return local();
    }
  }
}

class MemoryInfoBudgetStorage implements InfoBudgetStorage {
  private readonly states = new Map<string, BudgetState>();
  remember(key: string, snapshot: BudgetState): void {
    const previous = this.states.get(key) ?? emptyState();
    this.states.set(key, {
      nextAt: Math.max(previous.nextAt, snapshot.nextAt),
      pausedUntil: Math.max(previous.pausedUntil, snapshot.pausedUntil),
      generation: Math.max(previous.generation, snapshot.generation),
      failures: Math.max(previous.failures, snapshot.failures),
    });
  }
  async update<T>(key: string, change: (state: BudgetState) => T): Promise<T> {
    const state = boundedState(this.states.get(key));
    this.states.set(key, state);
    const changed = { ...state };
    const result = change(changed);
    this.states.set(key, changed);
    return result;
  }
}
const memoryStorage = new MemoryInfoBudgetStorage();
let sharedStorage: InfoBudgetStorage | undefined;
const storage = () => {
  if (sharedStorage) return sharedStorage;
  try {
    return (sharedStorage = typeof indexedDB === 'undefined' ? memoryStorage : new IndexedInfoBudgetStorage(indexedDB));
  } catch {
    warnHyperliquidInfoBudgetUnavailable();
    return (sharedStorage = memoryStorage);
  }
};

const LOW_WEIGHT = new Set([
  'l2Book',
  'allMids',
  'clearinghouseState',
  'orderStatus',
  'spotClearinghouseState',
  'exchangeStatus',
]);
const LIST_READS = new Set([
  'recentTrades',
  'historicalOrders',
  'userFills',
  'userFillsByTime',
  'fundingHistory',
  'userFunding',
  'userNonFundingLedgerUpdates',
  'nonUserFundingUpdates',
  'twapHistory',
  'userTwapSliceFills',
  'userTwapSliceFillsByTime',
  'delegatorHistory',
  'delegatorRewards',
  'validatorStats',
]);

export function hyperliquidInfoWeight(body: string | undefined): number {
  let type: unknown;
  try {
    type = JSON.parse(body ?? '{}').type;
  } catch {
    return 20;
  }
  if (type === 'userRole') return 60;
  if (typeof type === 'string' && LOW_WEIGHT.has(type)) return 2;
  // Reserve the SDK's maximum 2,000-row history page or 5,000-candle reply before sending.
  if (typeof type === 'string' && LIST_READS.has(type)) return 120;
  if (type === 'candleSnapshot') return 104;
  return 20;
}

function waitUntil(at: number, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  const delay = at - Date.now();
  if (delay <= 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const abort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', abort);
      resolve();
    }, delay);
    signal.addEventListener('abort', abort, { once: true });
  });
}

/** Admission precedes managed HTTP claim; a queued read never acquires send evidence. */
export class HyperliquidInfoBudget {
  constructor(private readonly backend: InfoBudgetStorage = storage()) {}

  async wait(origin: string, body: string | undefined, deadline: number, signal: AbortSignal): Promise<void> {
    const weight = hyperliquidInfoWeight(body);
    for (;;) {
      signal.throwIfAborted();
      const grant = await this.backend.update(origin, (state) => {
        signal.throwIfAborted();
        const at = Math.max(Date.now(), state.nextAt, state.pausedUntil);
        if (at >= deadline) throw new HttpPreparationError('timedOut');
        state.nextAt = at + weight * MS_PER_WEIGHT;
        return { at, generation: state.generation };
      });
      await waitUntil(grant.at, signal);
      signal.throwIfAborted();
      const current = await this.backend.update(origin, (state) => state.generation === grant.generation);
      if (Date.now() >= deadline) throw new HttpPreparationError('timedOut');
      if (current) return;
    }
  }

  async response(origin: string, status: number, headers: string): Promise<void> {
    if (status >= 200 && status < 300) {
      await this.backend.update(origin, (state) => {
        if (Date.now() >= state.pausedUntil) state.failures = 0;
      });
      return;
    }
    if (status !== 429) return;
    const value = headers
      .split(/\r?\n/)
      .find((line) => /^retry-after\s*:/i.test(line))
      ?.split(':')
      .slice(1)
      .join(':')
      .trim();
    const seconds = value === undefined ? NaN : Number(value);
    const delay = seconds > 0 ? Math.min(seconds * 1000, MAX_PAUSE_MS) : value ? Date.parse(value) - Date.now() : NaN;
    await this.backend.update(origin, (state) => {
      state.failures = Math.min((state.failures ?? 0) + 1, 5);
      const fallback = Math.min(60_000, 5000 * 2 ** (state.failures - 1));
      const pause = Number.isFinite(delay) && delay > 0 ? Math.min(delay, MAX_PAUSE_MS) : fallback;
      state.pausedUntil = Math.max(state.pausedUntil, Date.now() + pause);
      state.nextAt = state.pausedUntil;
      state.generation += 1;
    });
  }
}
