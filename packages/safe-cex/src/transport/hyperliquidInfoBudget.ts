import { HttpPreparationError } from '@tealstreet/safe-cex-wasm/host';

// Leave 200 of the documented 1,200 weight/minute for exchange actions and other clients.
const MS_PER_WEIGHT = 60;
const MAX_PAUSE_MS = 3_600_000;
type BudgetState = { nextAt: number; pausedUntil: number; generation: number; failures: number };
const emptyState = (): BudgetState => ({ nextAt: 0, pausedUntil: 0, generation: 0, failures: 0 });

export interface InfoBudgetStorage {
  update<T>(key: string, change: (state: BudgetState) => T): Promise<T>;
}

/** One origin database coordinates separate account workers without retaining account data. */
export class IndexedInfoBudgetStorage implements InfoBudgetStorage {
  private database?: Promise<IDBDatabase>;
  constructor(private readonly factory: IDBFactory) {}
  private open(): Promise<IDBDatabase> {
    return (this.database ??= new Promise((resolve, reject) => {
      const open = this.factory.open('safe-cex-hyperliquid-info-budget', 1);
      open.onupgradeneeded = () => open.result.createObjectStore('budgets');
      open.onsuccess = () => resolve(open.result);
      open.onerror = () => {
        this.database = undefined;
        reject(open.error);
      };
    }));
  }
  async update<T>(key: string, change: (state: BudgetState) => T): Promise<T> {
    const database = await this.open();
    return new Promise((resolve, reject) => {
      const transaction = database.transaction('budgets', 'readwrite');
      const store = transaction.objectStore('budgets');
      const read = store.get(key);
      let value: T;
      let failure: unknown;
      read.onsuccess = () => {
        try {
          const state: BudgetState = read.result ?? emptyState();
          value = change(state);
          store.put(state, key);
        } catch (error) {
          failure = error;
          transaction.abort();
        }
      };
      transaction.oncomplete = () => resolve(value);
      transaction.onabort = transaction.onerror = () => reject(failure ?? transaction.error);
    });
  }
}

class MemoryInfoBudgetStorage implements InfoBudgetStorage {
  private readonly states = new Map<string, BudgetState>();
  async update<T>(key: string, change: (state: BudgetState) => T): Promise<T> {
    const state = this.states.get(key) ?? emptyState();
    const result = change(state);
    this.states.set(key, state);
    return result;
  }
}
let sharedStorage: InfoBudgetStorage | undefined;
const storage = () =>
  (sharedStorage ??=
    typeof indexedDB === 'undefined' ? new MemoryInfoBudgetStorage() : new IndexedInfoBudgetStorage(indexedDB));

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
