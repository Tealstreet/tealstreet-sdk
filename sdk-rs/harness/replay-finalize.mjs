export function captureReplayState(snapshot, audit, { final = false } = {}) {
  return { ...snapshot, protocolErrors: final ? audit?.errors() ?? [] : [] };
}

/**
 * Capture the logical end of a replay before memory collection yields to live
 * timers. Both implementations include reconstruction in the CPU window and
 * retain a detached snapshot throughout memory settling; neither can observe
 * venue updates from that later window, including a synchronous GC callback.
 */
export async function finalizeReplay(capture, measure) {
  const snapshot = structuredClone(capture());
  return { ...snapshot, metrics: await measure() };
}
