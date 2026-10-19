// Node accounts WebAssembly linear memory separately from arrayBuffers.
// Received frame buffers still belong to JS and must count alongside its heap.
export const retainedMemory = (before, after, wasmBytes = 0) =>
  after.heapUsed - before.heapUsed + after.arrayBuffers - before.arrayBuffers + wasmBytes;

// Immediate-only samples can agree before delayed backing-store cleanup runs.
export const settledMemory = async ({
  collect = () => globalThis.gc?.(),
  sample = () => process.memoryUsage(),
  pause = () => new Promise((resolve) => setTimeout(resolve, 10)),
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
  while (now() <= deadline) {
    collect();
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
      `Backing-buffer memory did not settle (samples=${sampleCount}, transitions=${transitionCount}, lastArrayBuffers=${lastArrayBuffers}, elapsedMs=${elapsedMs})`,
    ),
    { sampleCount, transitionCount, lastArrayBuffers, elapsedMs },
  );
};
