import { setTimeout as realTimeout } from 'node:timers';

// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Received frame buffers still belong to JS and must count alongside its heap.
export const retainedMemory = (before, after, wasmBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes;

// Validate absolute samples, exact retained-memory arithmetic and each component.
// A negative component invalidates the before-import baseline; never clamp it.
export const memoryMetricErrors = (metrics) => {
  const errors = [];
  for (const field of [
    'heapBeforeBytes',
    'heapUsedBytes',
    'buffersBefore',
    'buffersAfter',
    'wasmBytes',
    'retainedBytes',
  ]) {
    const value = metrics?.[field];
    if (!Number.isFinite(value) || value < 0) errors.push(`invalid ${field} (${value})`);
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes']) {
    const value = metrics?.[field];
    if (!Number.isFinite(value) || value < 0) errors.push(`invalid ${field} (${value})`);
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

// Immediate-only samples can agree before delayed backing-store cleanup runs.
export const settledMemory = async ({
  collect = () => globalThis.gc?.(),
  sample = () => process.memoryUsage(),
  pause = () => new Promise((resolve) => realTimeout(resolve, 10)),
  now = () => performance.now(),
  quietMs = 100,
  timeoutMs = 5000,
} = {}) => {
  const started = now();
  const deadline = started + timeoutMs;
  let previous;
  let stableSince = now();
  let stable = 0;
  let sampleCount = 0;
  let transitionCount = 0;
  let lastArrayBuffers;
  let longestCollectionMs = 0;
  while (now() <= deadline) {
    const collectStarted = now();
    collect();
    longestCollectionMs = Math.max(longestCollectionMs, now() - collectStarted);
    await pause();
    const memory = sample();
    const time = now();
    sampleCount++;
    if (sampleCount > 1 && memory.arrayBuffers !== lastArrayBuffers) transitionCount++;
    lastArrayBuffers = memory.arrayBuffers;
    if (time > deadline) break;
    if (memory.arrayBuffers === previous) stable++;
    else {
      stable = 0;
      stableSince = time;
    }
    if (stable >= 2 && time - stableSince >= quietMs) return memory;
    previous = memory.arrayBuffers;
  }
  const elapsedMs = now() - started;
  throw Object.assign(
    new Error(
      `Backing-buffer memory did not settle (samples=${sampleCount}, transitions=${transitionCount}, lastArrayBuffers=${lastArrayBuffers}, elapsedMs=${elapsedMs}, longestGC=${longestCollectionMs.toFixed(1)}ms)`,
    ),
    { sampleCount, transitionCount, lastArrayBuffers, elapsedMs, longestCollectionMs },
  );
};
