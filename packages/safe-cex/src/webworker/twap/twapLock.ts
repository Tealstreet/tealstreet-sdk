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

/** Lock attempts a restore makes before it decides another tab owns the TWAP. */
export const TWAP_LOCK_ATTEMPTS = 4;
export const TWAP_LOCK_RETRY_MS = 500;

/**
 * A worker recreated in the same tab can ask before the browser has released
 * its predecessor's locks (release on termination is asynchronous), so a
 * restore retries briefly before reporting the TWAP as another tab's.
 */
export const acquireTwapLockForRestore = async (name: string): Promise<(() => void) | null> => {
  for (let attempt = 1; ; attempt++) {
    const release = await acquireTwapLock(name);
    if (release || attempt >= TWAP_LOCK_ATTEMPTS) return release;
    await new Promise((resolve) => setTimeout(resolve, TWAP_LOCK_RETRY_MS));
  }
};
