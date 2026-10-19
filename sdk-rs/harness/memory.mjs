// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Received frame buffers still belong to JS and must count alongside its heap.
export const retainedMemory = (before, after, wasmBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes;

// This measures net growth from loaded code, not allocation volume. Either
// signed component can shrink as setup storage is released during replay.
// Validate its absolute counters and arithmetic without discarding that release.
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
