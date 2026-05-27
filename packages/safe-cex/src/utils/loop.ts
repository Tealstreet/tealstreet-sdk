import { sleep } from '@tealstreet/sleep/src/sleep';

// Tripwire: in practice every caller is a per-exchange tick that should have a
// real minInterval from metadata. If we hit this fallback it means a metadata
// path is broken upstream — log loudly so the regression doesn't sit silent
// (the symptom is a tight network-bound REST loop hammering the exchange).
const FALLBACK_MIN_INTERVAL_MS = 5_000;
const loggedMissingIntervalCallers = new Set<string>();
const warnMissingInterval = (fn: () => void) => {
  // Dedupe by function source so a hot tick loop doesn't spam the console.
  const key = fn.toString().slice(0, 200);
  if (loggedMissingIntervalCallers.has(key)) return;
  loggedMissingIntervalCallers.add(key);
  console.warn(
    `[safe-cex] loop() called without a minInterval — defaulting to ${FALLBACK_MIN_INTERVAL_MS}ms. ` +
      `This is a safety fallback; the upstream caller should pass a real interval. Caller: ${key}`,
  );
};

export const loop = async (fn: () => void, minInterval?: number) => {
  const effectiveInterval =
    minInterval && minInterval > 0 ? minInterval : (warnMissingInterval(fn), FALLBACK_MIN_INTERVAL_MS);

  await sleep(effectiveInterval);

  if (typeof window !== 'undefined' && typeof document !== 'undefined' && document.visibilityState === 'visible') {
    requestAnimationFrame(() => fn());
  } else {
    setTimeout(() => fn(), 0);
  }
};
