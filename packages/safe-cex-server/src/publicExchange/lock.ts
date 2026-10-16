export type MarketLockAcquisition = {
  acquired: boolean;
  waitedMs: number;
  release: () => void;
};

export type InProcessFallbackLockOptions = {
  maxWaitMs?: number;
  retryDelayMs?: number;
};

type LockSlot = {
  done: Promise<void>;
  release: () => void;
};

const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

export class InProcessFallbackMarketLock {
  private readonly active = new Map<string, LockSlot>();

  async acquire(key: string, options: InProcessFallbackLockOptions = {}): Promise<MarketLockAcquisition> {
    const startedAt = Date.now();
    const maxWaitMs = options.maxWaitMs ?? Number.POSITIVE_INFINITY;
    const retryDelayMs = options.retryDelayMs ?? 25;

    while (this.active.has(key)) {
      if (Date.now() - startedAt >= maxWaitMs) {
        return {
          acquired: false,
          waitedMs: Date.now() - startedAt,
          release: () => undefined,
        };
      }

      const slot = this.active.get(key);
      if (slot) {
        await Promise.race([slot.done, sleep(retryDelayMs)]);
      }
    }

    let resolveDone: () => void = () => undefined;
    const done = new Promise<void>((resolve) => {
      resolveDone = resolve;
    });
    let released = false;
    const slot: LockSlot = {
      done,
      release: () => {
        if (released) return;
        released = true;
        if (this.active.get(key) === slot) {
          this.active.delete(key);
        }
        resolveDone();
      },
    };

    this.active.set(key, slot);

    return {
      acquired: true,
      waitedMs: Date.now() - startedAt,
      release: slot.release,
    };
  }

  async runExclusive<T>(key: string, fn: () => Promise<T>, options?: InProcessFallbackLockOptions): Promise<T> {
    const lock = await this.acquire(key, options);
    if (!lock.acquired) {
      throw new Error(`Timed out waiting for in-process market lock: ${key}`);
    }

    try {
      return await fn();
    } finally {
      lock.release();
    }
  }
}
