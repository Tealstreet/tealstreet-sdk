import { setTimeout as realTimeout } from 'node:timers';

// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Subtract the idle-loaded module's static reservation, counting runtime growth
// exactly once alongside JS heap and retained transfer buffers. Negative JS
// deltas are genuine releases and remain signed rather than being clamped.
export const retainedMemory = (before, after, wasmBytes = 0, wasmBeforeBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes - wasmBeforeBytes;

const cpuSample = () => ({ thread: process.threadCpuUsage(), process: process.cpuUsage(), wall: performance.now() });
const milliseconds = (before, after) => (after.user - before.user + after.system - before.system) / 1000;
export const idleLoadedBaseline = async ({ baseline, load, sample = cpuSample }) => {
  const before = sample();
  const loaded = await load();
  const after = sample();
  const loadMetrics = {
    idleLoadCpuMs: milliseconds(before.thread, after.thread),
    idleLoadProcessCpuMs: milliseconds(before.process, after.process),
    idleLoadWallMs: after.wall - before.wall,
  };
  // Baseline collection is a separate interval: its GC is never counted as
  // implementation loading or runtime CPU, including background GC threads.
  return { loaded, loadMetrics, memory: await baseline() };
};

// Performance runs have no checkpoint audit. Freeze their CPU/memory measures
// before building diagnostic rows, detached snapshots or serialized output.
// Audited correctness runs retain finalizeReplay's opposite capture-first order.
export const measureBeforeCapture = async (capture, measure) => {
  const metrics = await measure();
  return { ...structuredClone(capture()), metrics };
};

// Validate absolute counters and exact delta arithmetic. Releases may make a
// heap/buffer/aggregate delta negative; this is not an invalid absolute sample.
export const memoryMetricErrors = (metrics) => {
  const errors = [];
  if (metrics?.baselineKind !== 'idle-loaded') errors.push('invalid baselineKind (expected idle-loaded)');
  if (!['performance', 'correctness'].includes(metrics?.measuredMode)) errors.push('invalid measuredMode');
  for (const field of [
    'cpuMs', 'processCpuMs', 'steadyCpuMs', 'wallMs',
    'idleLoadCpuMs', 'idleLoadProcessCpuMs', 'idleLoadWallMs',
    'runtimeCpuMs', 'runtimeProcessCpuMs', 'runtimeWallMs',
    'heapBeforeBytes', 'heapUsedBytes', 'buffersBefore', 'buffersAfter',
    'wasmBeforeBytes', 'wasmBytes', 'wasmRetainedBytes',
  ]) {
    const value = metrics?.[field];
    if (!Number.isFinite(value) || value < 0) errors.push(`invalid ${field} (${value})`);
  }
  for (const field of ['heapRetainedBytes', 'bufferRetainedBytes', 'retainedBytes']) {
    const value = metrics?.[field];
    if (!Number.isFinite(value)) errors.push(`invalid ${field} (${value})`);
  }
  if (errors.length) return errors;
  for (const [field, expected] of [
    ['cpuMs', metrics.idleLoadCpuMs + metrics.runtimeCpuMs],
    ['processCpuMs', metrics.idleLoadProcessCpuMs + metrics.runtimeProcessCpuMs],
    ['wallMs', metrics.idleLoadWallMs + metrics.runtimeWallMs],
    ['heapRetainedBytes', metrics.heapUsedBytes - metrics.heapBeforeBytes],
    ['bufferRetainedBytes', metrics.buffersAfter - metrics.buffersBefore],
    ['wasmRetainedBytes', metrics.wasmBytes - metrics.wasmBeforeBytes],
    ['retainedBytes', metrics.heapRetainedBytes + metrics.bufferRetainedBytes + metrics.wasmRetainedBytes],
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
  let heapLow = Infinity;
  let heapStableSince = now();
  let heapTransitionCount = 0;
  let lastHeapUsed;
  let longestCollectionMs = 0;
  while (now() <= deadline) {
    const collectStarted = now();
    await collect();
    longestCollectionMs = Math.max(longestCollectionMs, now() - collectStarted);
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
      `Heap/backing-buffer memory did not settle (samples=${sampleCount}, transitions=${transitionCount}, lastArrayBuffers=${lastArrayBuffers}, elapsedMs=${elapsedMs}, heapTransitions=${heapTransitionCount}, lastHeapUsed=${lastHeapUsed}, longestGC=${longestCollectionMs.toFixed(1)}ms)`,
    ),
    { sampleCount, transitionCount, lastArrayBuffers, elapsedMs, heapTransitionCount, lastHeapUsed, longestCollectionMs },
  );
};
