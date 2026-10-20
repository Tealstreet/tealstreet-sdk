import { setTimeout as realTimeout } from 'node:timers';

// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Received frame buffers still belong to JS and must count alongside its heap.
export const retainedMemory = (before, after, wasmBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes;

// Include verifier setup's full allocation/release lifecycle in both runners.
// Sampling after setup could subtract temporary loader objects from retention.
export const baselineBeforeSetup = async ({ baseline, initialize }) => {
  const before = await baseline();
  await initialize();
  return before;
};

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

// Complete code flushing as well as object collection before measuring retention.
// New heap lows restart the quiet window; return the actual current sample.
export const settledMemory = async ({
  collect = () => globalThis.gc?.({ type: 'major', execution: 'async', flavor: 'last-resort' }),
  sample = () => process.memoryUsage(),
  pause = () => new Promise((resolve) => realTimeout(resolve, 10)),
  now = () => performance.now(),
  quietMs = 100,
  timeoutMs = 1000,
} = {}) => {
  const started = now();
  const deadline = started + timeoutMs;
  let previous;
  let stableSince = now();
  let stable = 0;
  let sampleCount = 0;
  let transitionCount = 0;
  let lastArrayBuffers;
  let heapLow = Infinity;
  let heapStableSince = now();
  let heapTransitionCount = 0;
  let lastHeapUsed;
  while (now() <= deadline) {
    await collect();
    await pause();
    const memory = sample();
    const time = now();
    sampleCount++;
    if (sampleCount > 1 && memory.arrayBuffers !== lastArrayBuffers) transitionCount++;
    lastArrayBuffers = memory.arrayBuffers;
    lastHeapUsed = memory.heapUsed;
    if (time > deadline) break;
    if (memory.arrayBuffers === previous) stable++;
    else {
      stable = 0;
      stableSince = time;
    }
    if (memory.heapUsed < heapLow) {
      if (Number.isFinite(heapLow)) heapTransitionCount++;
      heapLow = memory.heapUsed;
      heapStableSince = time;
    }
    if (stable >= 2 && time - stableSince >= quietMs && time - heapStableSince >= quietMs) return memory;
    previous = memory.arrayBuffers;
  }
  const elapsedMs = now() - started;
  throw Object.assign(
    new Error(
      `Heap/backing-buffer memory did not settle (samples=${sampleCount}, transitions=${transitionCount}, lastArrayBuffers=${lastArrayBuffers}, elapsedMs=${elapsedMs}, heapTransitions=${heapTransitionCount}, lastHeapUsed=${lastHeapUsed})`,
    ),
    { sampleCount, transitionCount, lastArrayBuffers, elapsedMs, heapTransitionCount, lastHeapUsed },
  );
};
