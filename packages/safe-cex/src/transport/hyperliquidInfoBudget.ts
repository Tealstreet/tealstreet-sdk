import { HttpPreparationError } from '@tealstreet/safe-cex-wasm/host';
import { diagnosticConsole } from '../utils/diagnosticRedaction';

// Leave 200 of the documented 1,200 weight/minute for exchange actions and other clients.
const MS_PER_WEIGHT = 60;
const BUCKET_WINDOW_MS = 60_000;
const MAX_PAUSE_MS = 300_000;
const STORAGE_TIMEOUT_MS = 1500;
const STORAGE_COOLDOWN_MS = 30_000;
const DISPATCH_POLL_MS = 100;
const DISPATCH_LEASE_MS = 2000;
const MAX_DISPATCH_WAITERS = 128;
const dispatchRealm = Math.random().toString(36).slice(2);
let dispatchSequence = 0;
export type InfoReadClass = 'critical' | 'reconciliation' | 'normal' | 'bulk';
const rank = (readClass: InfoReadClass = 'normal') =>
  readClass === 'critical' ? 0 : readClass === 'reconciliation' ? 1 : readClass === 'normal' ? 2 : 3;
const capacity = (readClass: InfoReadClass) =>
  readClass === 'critical' || readClass === 'reconciliation' ? 1000 : 800;
type DispatchWaiter = { id: string; until: number; readClass?: InfoReadClass };
let indexedDBOpenedInRealm = false;
type BudgetState = {
  nextAt: number;
  pausedUntil: number;
  generation: number;
  failures: number;
  dispatchQueue?: DispatchWaiter[];
};
const emptyState = (): BudgetState => ({ nextAt: 0, pausedUntil: 0, generation: 0, failures: 0 });
const mergeState = (a: BudgetState, b: BudgetState): BudgetState => ({
  nextAt: Math.max(a.nextAt, b.nextAt),
  pausedUntil: Math.max(a.pausedUntil, b.pausedUntil),
  generation: Math.max(a.generation, b.generation),
  failures: Math.max(a.failures, b.failures),
  dispatchQueue: a.dispatchQueue,
});
class StorageTimeoutError extends Error {}
let warned = false;
export function warnHyperliquidInfoBudgetUnavailable(): void {
  if (warned) return;
  warned = true;
  try {
    diagnosticConsole.warn('Hyperliquid /info budget storage is unavailable');
  } catch {
    /* Diagnostics cannot interrupt a venue read. */
  }
}

function boundedState(raw: Partial<BudgetState> = {}): BudgetState {
  const now = Date.now();
  const timestamp = (value: number | undefined, maximum: number) =>
    typeof value === 'number' && Number.isFinite(value) ? Math.max(now, Math.min(value, now + maximum)) : now;
  return {
    dispatchQueue: Array.isArray(raw.dispatchQueue)
      ? raw.dispatchQueue
          .filter(
            (row) =>
              row && typeof row.id === 'string' && row.id.length <= 64 && Number.isFinite(row.until) && row.until > now,
          )
          .slice(0, MAX_DISPATCH_WAITERS)
          .map((row) => ({
            id: row.id,
            until: Math.min(row.until, now + DISPATCH_LEASE_MS),
            readClass:
              row.readClass === 'critical' || row.readClass === 'reconciliation' || row.readClass === 'bulk'
                ? row.readClass
                : ('normal' as const),
          }))
      : [],
    nextAt: timestamp(raw.nextAt, MAX_PAUSE_MS + BUCKET_WINDOW_MS),
    pausedUntil: timestamp(raw.pausedUntil, MAX_PAUSE_MS),
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
    const timer = setTimeout(
      () => fail(new StorageTimeoutError('Hyperliquid budget storage timed out')),
      STORAGE_TIMEOUT_MS,
    );
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
  update<T>(key: string, change: (state: BudgetState, shared: boolean) => T): Promise<T>;
}

/** One origin database coordinates separate account workers without retaining account data. */
export class IndexedInfoBudgetStorage implements InfoBudgetStorage {
  private database?: Promise<IDBDatabase>;
  private retryAt = 0;
  private localRevision = 0;
  private readonly localKeys = new Set<string>();
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
        indexedDBOpenedInRealm = true;
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
  async update<T>(key: string, change: (state: BudgetState, shared: boolean) => T): Promise<T> {
    const local = () => {
      const snapshot = this.snapshots.get(key);
      if (snapshot) {
        memoryStorage.remember(key, snapshot);
        this.snapshots.delete(key);
      }
      this.localKeys.add(key);
      this.localRevision += 1;
      return memoryStorage.update(key, change);
    };
    this.retryAt = Math.min(this.retryAt, Date.now() + STORAGE_COOLDOWN_MS);
    if (this.retryAt > Date.now()) return local();
    for (let attempt = 0; ; attempt++) {
      let callbackFailed = false;
      let callbackError: unknown;
      try {
        const database = await this.open();
        if (this.retryAt > Date.now()) return local();
        return await storageAttempt<T>((done, fail) => {
          const transaction = database.transaction('budgets', 'readwrite');
          const store = transaction.objectStore('budgets');
          const read = store.get(key);
          let value: T;
          let saved: BudgetState | undefined;
          let expired = false;
          let mergedRevision = this.localRevision;
          read.onsuccess = () => {
            if (expired) return;
            try {
              let state = boundedState(read.result ?? emptyState());
              mergedRevision = this.localRevision;
              if (this.localKeys.has(key)) {
                state = mergeState(state, memoryStorage.snapshot(key));
                state.generation += 1;
              }
              const changed = { ...state };
              try {
                value = change(changed, true);
              } catch (error) {
                callbackFailed = true;
                callbackError = error;
              }
              // Commit clock normalization even when admission refuses this read.
              saved = callbackFailed ? state : changed;
              store.put(saved, key);
            } catch (error) {
              fail(error);
            }
          };
          transaction.oncomplete = () => {
            if (saved) this.snapshots.set(key, { ...saved });
            // An earlier transaction cannot clear newer fallback admissions.
            if (mergedRevision === this.localRevision) {
              this.retryAt = 0;
              this.localKeys.delete(key);
            }
            return callbackFailed ? fail(callbackError) : done(value);
          };
          transaction.onabort = transaction.onerror = () => fail(transaction.error);
          return () => {
            expired = true;
            transaction.abort();
          };
        });
      } catch (error) {
        // Deadline/abort refusals are admission decisions, never storage failures.
        if (callbackFailed && error === callbackError) throw error;
        void this.database?.then((database) => database.close()).catch(() => undefined);
        this.database = undefined;
        if (!callbackFailed && error instanceof StorageTimeoutError && attempt === 0) continue;
        this.retryAt = Date.now() + STORAGE_COOLDOWN_MS;
        warnHyperliquidInfoBudgetUnavailable();
        if (callbackFailed) throw callbackError;
        return local();
      }
    }
  }
}

class MemoryInfoBudgetStorage implements InfoBudgetStorage {
  private readonly states = new Map<string, BudgetState>();
  remember(key: string, snapshot: BudgetState): void {
    this.states.set(key, mergeState(this.snapshot(key), snapshot));
  }
  snapshot(key: string): BudgetState {
    return boundedState(this.states.get(key));
  }
  async update<T>(key: string, change: (state: BudgetState, shared: boolean) => T): Promise<T> {
    const state = this.snapshot(key);
    this.states.set(key, state);
    const changed = { ...state };
    const result = change(changed, false);
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
    // A denied global accessor can become usable after the same cooldown.
    return (sharedStorage = new IndexedInfoBudgetStorage({
      open: (name: string, version?: number) => indexedDB.open(name, version),
    } as IDBFactory));
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

function readClassFor(body: string | undefined, readClass: InfoReadClass): InfoReadClass {
  if (readClass === 'critical') return 'critical';
  try {
    const type: unknown = JSON.parse(body ?? '{}').type;
    if (type === 'candleSnapshot' || (typeof type === 'string' && LIST_READS.has(type))) return 'bulk';
  } catch {
    /* Malformed bodies get no priority. */
  }
  return readClass === 'reconciliation' ? 'reconciliation' : readClass === 'bulk' ? 'bulk' : 'normal';
}
export type InfoBudgetGrant = {
  generation: number;
  weight: number;
  readClass?: InfoReadClass;
  waiterId?: string;
  cancelWait?: () => void;
};

/** Admission precedes managed HTTP claim; only a physical handoff consumes tokens. */
export class HyperliquidInfoBudget {
  constructor(private readonly backend: InfoBudgetStorage = storage()) {}

  async wait(
    origin: string,
    body: string | undefined,
    deadline: number,
    signal: AbortSignal,
    readClass: InfoReadClass = 'normal',
  ): Promise<InfoBudgetGrant> {
    readClass = readClassFor(body, readClass);
    const weight = hyperliquidInfoWeight(body);
    let waiterId: string | undefined;
    let cancelled = false;
    const cancelWait = () => {
      if (cancelled) return;
      cancelled = true;
      signal.removeEventListener('abort', cancelWait);
      if (waiterId)
        void this.backend
          .update(origin, (state) => {
            state.dispatchQueue = state.dispatchQueue?.filter((row) => row.id !== waiterId);
          })
          .catch(() => warnHyperliquidInfoBudgetUnavailable());
    };
    try {
      for (;;) {
        signal.throwIfAborted();
        const grant = await this.backend.update(origin, (state) => {
          signal.throwIfAborted();
          const now = Date.now();
          const at = Math.max(
            now,
            state.nextAt + weight * MS_PER_WEIGHT - capacity(readClass) * MS_PER_WEIGHT,
            state.pausedUntil,
          );
          if (at >= deadline) throw new HttpPreparationError('timedOut');
          if (at > now || waiterId) {
            const queue = (state.dispatchQueue ?? []).filter((row) => row.until > now);
            if (
              !waiterId &&
              queue.length >= (readClass === 'critical' ? MAX_DISPATCH_WAITERS : readClass !== 'bulk' ? 96 : 64)
            )
              return { at: Math.min(deadline, now + DISPATCH_POLL_MS), ready: false, generation: state.generation };
            if (!waiterId) {
              waiterId = `${dispatchRealm}:${++dispatchSequence}`;
              signal.addEventListener('abort', cancelWait, { once: true });
            }
            const row = { id: waiterId, until: Math.min(deadline, now + DISPATCH_LEASE_MS), readClass };
            state.dispatchQueue = queue.some((entry) => entry.id === waiterId)
              ? queue.map((entry) => (entry.id === waiterId ? row : entry))
              : [...queue, row];
          }
          return { at: Math.min(at, now + DISPATCH_POLL_MS), ready: at <= now, generation: state.generation };
        });
        if (!grant.ready) {
          await waitUntil(grant.at, signal);
          continue;
        }
        signal.throwIfAborted();
        const current = await this.backend.update(origin, (state) => state.generation === grant.generation);
        if (Date.now() >= deadline) throw new HttpPreparationError('timedOut');
        if (current)
          return { generation: grant.generation, weight, readClass, ...(waiterId ? { waiterId, cancelWait } : {}) };
      }
    } catch (error) {
      cancelWait();
      throw error;
    }
  }

  async admitDispatch<T>(
    origin: string,
    grant: InfoBudgetGrant,
    deadline: number,
    signal: AbortSignal,
    send: () => T,
    wasSent: () => boolean = () => true,
  ): Promise<T> {
    const cost = grant.weight * MS_PER_WEIGHT;
    const readClass = grant.readClass ?? 'normal';
    const waiterId = grant.waiterId ?? `${dispatchRealm}:${++dispatchSequence}`;
    let charged = false;
    let dispatched = false;
    const active = () => {
      signal.throwIfAborted();
      if (Date.now() >= deadline) throw new HttpPreparationError('timedOut');
    };
    const retryStorageAt = () => Math.min(deadline, Date.now() + 1000);
    try {
      for (;;) {
        const reservation = await this.backend.update(origin, (state, shared) => {
          active();
          // Once this realm joined IDB, local-only sends could exceed the peers' budget.
          if (!shared && indexedDBOpenedInRealm) return { at: retryStorageAt() };
          const now = Date.now();
          const at = Math.max(now, state.nextAt + cost - capacity(readClass) * MS_PER_WEIGHT, state.pausedUntil);
          if (at >= deadline || state.generation !== grant.generation) throw new HttpPreparationError('timedOut');
          const queue = (state.dispatchQueue ?? []).filter((row) => row.until > now);
          const existing = queue.find((row) => row.id === waiterId);
          if (
            !existing &&
            queue.length >= (readClass === 'critical' ? MAX_DISPATCH_WAITERS : readClass !== 'bulk' ? 96 : 64)
          )
            return { at: Math.min(deadline, now + DISPATCH_POLL_MS) };
          // Leases order dispatch attempts across realms without charging queued work.
          state.dispatchQueue = existing
            ? queue.map((row) =>
                row.id === waiterId
                  ? { id: waiterId, until: Math.min(deadline, now + DISPATCH_LEASE_MS), readClass }
                  : row,
              )
            : [...queue, { id: waiterId, until: Math.min(deadline, now + DISPATCH_LEASE_MS), readClass }];
          // FIFO within each class; critical work can pass queued bulk without spending its tokens.
          state.dispatchQueue.sort((a, b) => rank(a.readClass) - rank(b.readClass));
          if (state.dispatchQueue[0]?.id !== waiterId) return { at: Math.min(deadline, now + DISPATCH_POLL_MS) };
          if (at > now) return { at: Math.min(at, now + DISPATCH_POLL_MS) };
          state.nextAt = Math.max(now, state.nextAt) + cost;
          state.dispatchQueue = state.dispatchQueue.filter((row) => row.id !== waiterId);
          return undefined;
        });
        if (!reservation) {
          charged = true;
          break;
        }
        await waitUntil(reservation.at, signal);
      }
      for (;;) {
        let handoffCalled = false;
        const decision = await new Promise<{ at: number } | { value: T }>((resolve, reject) => {
          void this.backend
            .update(origin, (state, shared) => {
              if (handoffCalled) return;
              active();
              if (!shared && indexedDBOpenedInRealm) return { at: retryStorageAt() };
              if (state.pausedUntil > Date.now() || state.generation !== grant.generation)
                throw new HttpPreparationError('timedOut');
              handoffCalled = true;
              try {
                resolve({ value: send() });
              } catch (error) {
                reject(error);
              } finally {
                dispatched = wasSent();
              }
            })
            .then((waiting) => {
              if (!handoffCalled && waiting) resolve(waiting);
            })
            .catch((error) => {
              if (!handoffCalled) reject(error);
              else warnHyperliquidInfoBudgetUnavailable();
            });
        });
        if ('value' in decision) return await decision.value;
        await waitUntil(decision.at, signal);
      }
    } finally {
      grant.cancelWait?.();
      if (!dispatched) {
        await this.backend
          .update(origin, (state) => {
            state.dispatchQueue = state.dispatchQueue?.filter((row) => row.id !== waiterId);
            // A 429 replaced the old token generation; refunds cannot weaken that floor.
            if (charged && state.generation === grant.generation)
              state.nextAt = Math.max(Date.now(), state.nextAt - cost);
          })
          .catch(() => warnHyperliquidInfoBudgetUnavailable());
      }
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
      // A rate limit empties the bucket; queued reads refill only after the pause.
      state.nextAt = state.pausedUntil + BUCKET_WINDOW_MS;
      state.generation += 1;
    });
  }
}
