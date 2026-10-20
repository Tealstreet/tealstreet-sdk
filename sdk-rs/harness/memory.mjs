import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';

// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Received frame buffers still belong to JS and must count alongside its heap.
export const retainedMemory = (before, after, wasmBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes;

// Retention is net growth from loaded code, not allocation volume. Keep signed
// deltas exact while validating absolute counters and their total.
export const memoryMetricErrors = (metrics) => {
  const errors = [];
  for (const field of ['heapBeforeBytes', 'heapUsedBytes', 'buffersBefore', 'buffersAfter', 'wasmBytes', 'retainedBytes']) {
    const value = metrics?.[field];
    if (!Number.isFinite(value) || value < 0) errors.push(`invalid ${field} (${value})`);
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes']) {
    const value = metrics?.[field];
    if (!Number.isFinite(value)) errors.push(`invalid ${field} (${value})`);
  }
  if (errors.length) return errors;

  for (const [field, expected] of [
    ['heapRetainedBytes', metrics.heapUsedBytes - metrics.heapBeforeBytes],
    ['bufferRetainedBytes', metrics.buffersAfter - metrics.buffersBefore],
    ['retainedBytes', metrics.heapRetainedBytes + metrics.bufferRetainedBytes + metrics.wasmBytes],
  ]) {
    if (metrics[field] !== expected) errors.push(`inconsistent ${field} (${metrics[field]}; expected ${expected})`);
  }
  return errors;
};

// Async major GC runs at a clean task boundary; synchronous GC can retain dead
// loader objects through conservative stack roots. Backing stores still need
// the same quiet window after collection completes.
export const settledMemory = async ({
  collect = () => globalThis.gc?.({ type: 'major', execution: 'async' }),
  sample = () => process.memoryUsage(),
  pause = () => new Promise((resolve) => realTimeout(resolve, 10)),
  now = () => performance.now(),
  quietMs = 100,
  timeoutMs = 1000,
} = {}) => {
  const deadline = now() + timeoutMs;
  let previous;
  let stableSince = now();
  let stable = 0;
  let stopped = false;
  let timer;
  const failure = () => new Error('Backing-buffer memory did not settle');
  const expired = new Promise((_, reject) => {
    timer = realTimeout(() => { stopped = true; reject(failure()); }, timeoutMs);
  });
  const run = async () => {
    while (!stopped && now() <= deadline) {
      await collect();
      if (stopped || now() > deadline) break;
      await pause();
      if (stopped || now() > deadline) break;
      const memory = sample();
      const time = now();
      if (time > deadline) break;
      if (memory.arrayBuffers === previous) stable++;
      else {
        stable = 0;
        stableSince = time;
      }
      if (stable >= 2 && time - stableSince >= quietMs) return memory;
      previous = memory.arrayBuffers;
    }
    throw failure();
  };
  try { return await Promise.race([run(), expired]); }
  finally { stopped = true; realClearTimeout(timer); }
};
