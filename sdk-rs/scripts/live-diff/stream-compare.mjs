import { differences } from './compare.mjs';

export function frameTopics(tape) {
  const counts = {};
  for (const frame of tape.frames) {
    let topic = frame.kind;
    if (frame.kind === 'message') {
      try {
        const data = JSON.parse(Buffer.from(frame.base64, 'base64').toString());
        topic =
          (Array.isArray(data)
            ? data[0]?.e
            : (data.e ?? data.topic?.split('.')[0] ?? data.arg?.channel ?? data.channel)) ?? 'ack/other';
      } catch {
        topic = frame.binary ? 'binary' : 'unparsed';
      }
    }
    counts[topic] = (counts[topic] ?? 0) + 1;
  }
  return counts;
}

export function channels(outputs) {
  const result = { combined: outputs, store: [], orderbook: [], candles: [] };
  for (const output of outputs) {
    const key = output.kind === 'event' ? `event:${output.event}` : output.kind;
    (result[key] ??= []).push(output.data);
    if (output.kind === 'store') {
      for (const [slice, data] of Object.entries(output.data)) (result[`store:${slice}`] ??= []).push(data);
    }
  }
  return result;
}

// Preserve order, repeated values, NaN, missing fields and clock IDs. No sorting,
// timestamp removal, sequence alignment, coalescing or JSON round-trip.
export function compareSequences(left, right) {
  const a = channels(left),
    b = channels(right);
  return Object.fromEntries(
    [...new Set([...Object.keys(a), ...Object.keys(b)])].map((channel) => {
      const x = a[channel] ?? [],
        y = b[channel] ?? [];
      const diffs = differences(x, y);
      return [
        channel,
        {
          status: diffs.length ? 'DIFF' : x.length ? 'PASS' : 'NO_OUTPUT',
          leftCount: x.length,
          rightCount: y.length,
          diffs: diffs.slice(0, 8),
        },
      ];
    }),
  );
}

export function qualifyComparisons(parity, repeatability, { inputDiff = false, incomplete = false } = {}) {
  for (const [channel, comparison] of Object.entries(parity)) {
    comparison.qualifiedStatus = inputDiff
      ? 'INPUT_DIFF'
      : incomplete
        ? 'INCOMPLETE'
        : [repeatability.ts[channel], repeatability.rust[channel]].some((c) => c?.status === 'DIFF')
          ? 'NONDETERMINISTIC'
          : comparison.status;
  }
  return parity;
}
