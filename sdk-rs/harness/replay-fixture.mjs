import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

// Common fixture storage must have the same lifetime at both memory snapshots.
// This corrects input ownership; it does not identify a particular V8 heap owner.
const activeFixtures = new Map();

export function acquireReplayFixture(path, { read = readFileSync } = {}) {
  const compressedBytes = read(path);
  const decodedText = (path.endsWith('.gz') ? gunzipSync(compressedBytes) : compressedBytes).toString('utf8');
  const lines = decodedText.split('\n');
  const filteredLines = lines.filter(Boolean);
  let urls = new Map();
  const records = filteredLines.map((line) => {
    const record = JSON.parse(line);
    if (record.effectiveAt !== undefined) record.availableAfter ??= record.effectiveAt;
    // Share exact input URLs before sampling; later V8 string thinning must not
    // subtract duplicate fixture storage from implementation retained memory.
    if (typeof record.url === 'string') {
      const previous = urls.get(record.url);
      if (previous === undefined) urls.set(record.url, record.url);
      else record.url = previous;
    }
    return record;
  });
  // Returned lease closures may share this activation; drop the parse-only pool.
  urls = undefined;
  // Register only a fully parsed fixture, so read/decode/parse failures leak no lease.
  const token = Symbol('replay fixture');
  const inputs = Object.freeze({ compressedBytes, decodedText, lines, filteredLines, records });
  activeFixtures.set(token, inputs);
  return Object.freeze({
    get inputs() { return activeFixtures.get(token); },
    get records() { return activeFixtures.get(token)?.records; },
    release() { activeFixtures.delete(token); },
  });
}
