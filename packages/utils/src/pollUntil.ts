/**
 * Poll a condition function until it returns true, with a timeout.
 * Useful for waiting on async state (e.g., markets loaded) without blocking the event loop.
 *
 * @param condition - Function that returns true when the wait is over
 * @param options - Polling interval (ms) and timeout (ms)
 * @returns Promise that resolves when condition is true, rejects on timeout
 *
 * @example
 * await pollUntil(() => exchange.store.loaded?.markets === true, { timeoutMs: 15000 });
 */
export function pollUntil(
  condition: () => boolean,
  options?: { intervalMs?: number; timeoutMs?: number; timeoutMessage?: string },
): Promise<void> {
  const { intervalMs = 100, timeoutMs = 15000, timeoutMessage } = options ?? {};

  if (condition()) return Promise.resolve();

  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(timeoutMessage ?? `pollUntil timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    const check = () => {
      if (condition()) {
        clearTimeout(timeout);
        resolve();
      } else {
        setTimeout(check, intervalMs);
      }
    };
    check();
  });
}
