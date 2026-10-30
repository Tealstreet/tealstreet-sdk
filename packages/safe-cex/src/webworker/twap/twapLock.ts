/** The subset of the Web Locks API (`navigator.locks`) a TWAP owner lock uses. */
export interface TwapLockManager {
  request(
    name: string,
    options: { ifAvailable: true },
    callback: (lock: unknown | null) => Promise<void> | undefined,
  ): Promise<unknown>;
}

let override: TwapLockManager | null | undefined;

/** Tests substitute a lock manager; `null` means the runtime has none. */
export const setTwapLockManager = (manager: TwapLockManager | null | undefined) => {
  override = manager;
};

const lockManager = (): TwapLockManager | null => {
  if (override !== undefined) return override;
  const locks = (globalThis as { navigator?: { locks?: TwapLockManager } }).navigator?.locks;
  return locks ?? null;
};

export const twapLockName = (accountName: string, id: string) => `tealstreet-twap:${accountName}:${id}`;

/**
 * Take the cross-tab owner lock of one TWAP: a duplicated tab copies the
 * sessionStorage rows, and only the tab holding the lock may run them. Resolves
 * to the release, or null when another tab or worker holds it. The browser
 * releases a dead worker's locks. A runtime without Web Locks (Node, React
 * Native) runs one worker per account, so it gets a no-op lock.
 */
export const acquireTwapLock = (name: string): Promise<(() => void) | null> => {
  const locks = lockManager();
  if (!locks) return Promise.resolve(() => {});
  return new Promise((resolve) => {
    let release: () => void = () => {};
    const held = new Promise<void>((done) => {
      release = done;
    });
    locks
      .request(name, { ifAvailable: true }, (lock) => {
        if (!lock) {
          resolve(null);
          return undefined;
        }
        resolve(release);
        return held;
      })
      .catch(() => resolve(null));
  });
};

/** How long a restore keeps asking for a lock its own reloaded tab may still hold. */
export const TWAP_RESTORE_LOCK_WAIT_MS = 5_000;
const TWAP_RESTORE_LOCK_RETRY_MS = 250;

/**
 * `acquireTwapLock`, retried for `waitMs`. A reloaded tab's previous worker
 * holds its locks until the browser tears it down, which can be after the new
 * worker restores; refusing on the first try would end a TWAP no tab runs.
 */
export const acquireTwapLockWithin = async (name: string, waitMs: number): Promise<(() => void) | null> => {
  const deadline = Date.now() + waitMs;
  for (;;) {
    const release = await acquireTwapLock(name);
    if (release || Date.now() >= deadline) return release;
    await new Promise((resolve) => setTimeout(resolve, TWAP_RESTORE_LOCK_RETRY_MS));
  }
};
