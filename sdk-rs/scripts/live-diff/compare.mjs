import { isDeepStrictEqual } from 'node:util';

// Do not JSON-round-trip exchange results: undefined, NaN and -0 are SDK values.
export function differences(ts, rust, path = '$', result = []) {
  if (isDeepStrictEqual(ts, rust)) return result;
  if (result.length >= 100) return result;
  // Recurse only through plain records and arrays. Map/Set/Date/buffers have
  // non-enumerable state: Object.keys would silently turn a real diff into PASS.
  const plain = (value) =>
    value &&
    (Array.isArray(value) ||
      Object.getPrototypeOf(value) === Object.prototype ||
      Object.getPrototypeOf(value) === null);
  if (plain(ts) && plain(rust) && Array.isArray(ts) === Array.isArray(rust)) {
    for (const key of new Set([...Object.keys(ts), ...Object.keys(rust)])) {
      if (Object.hasOwn(ts, key) !== Object.hasOwn(rust, key)) {
        result.push({
          path: `${path}.${key}`,
          ts: Object.hasOwn(ts, key) ? display(ts[key]) : '<absent>',
          rust: Object.hasOwn(rust, key) ? display(rust[key]) : '<absent>',
        });
      } else differences(ts[key], rust[key], `${path}.${key}`, result);
      if (result.length >= 100) break;
    }
    if (Array.isArray(ts) && ts.length !== rust.length && result.length < 100) {
      result.push({ path: `${path}.length`, ts: ts.length, rust: rust.length });
    }
  } else result.push({ path, ts: display(ts), rust: display(rust) });
  return result;
}

function display(value) {
  if (value === undefined) return '<undefined>';
  if (typeof value === 'number' && !Number.isFinite(value)) return String(value);
  if (Object.is(value, -0)) return '-0';
  if (typeof value === 'bigint') return `${value}n`;
  // Missing whole records can contain credential-shaped raw `info` data.
  if (value && typeof value === 'object') return Array.isArray(value) ? `<array:${value.length}>` : '<object>';
  return value;
}

export function byteDifference(ts, rust) {
  const a = Buffer.from(ts),
    b = Buffer.from(rust);
  let offset = 0;
  while (offset < Math.min(a.length, b.length) && a[offset] === b[offset]) offset++;
  return a.equals(b)
    ? null
    : {
        offset,
        tsLength: a.length,
        rustLength: b.length,
        ts: a.subarray(Math.max(0, offset - 30), offset + 100).toString(),
        rust: b.subarray(Math.max(0, offset - 30), offset + 100).toString(),
      };
}

export function outcome(method, left, right, requests) {
  const diffs = differences(left.failure ?? left.value, right.failure ?? right.value);
  const acceptsFallback = (request) =>
    request.expectedFallback &&
    (method === '$startPublic' ||
      (request.host === 'fapi.asterdex.com' && ['refreshMarkets', 'fetchMarkets', 'fetchTickers'].includes(method)));
  const transportBlocked = requests.some(
    (request) =>
      request.transportError ||
      request.blocked ||
      (request.protocol === 'websocket' && (request.failure || request.tlsError)) ||
      (request.status >= 400 && !acceptsFallback(request)),
  );
  const errorBudget = (implementation) =>
    requests.filter((request) => request.implementation === implementation && acceptsFallback(request)).length;
  const blocked =
    transportBlocked ||
    left.failure ||
    right.failure ||
    left.errors > errorBudget('ts') ||
    right.errors > errorBudget('rust') ||
    left.value?.error ||
    right.value?.error ||
    (method === '$balance' && (!Number.isFinite(left.value?.total) || !Number.isFinite(right.value?.total))) ||
    (['fetchMarkets', 'fetchTickers', '$markets', '$tickers'].includes(method) &&
      (!Array.isArray(left.value) || !left.value.length || !Array.isArray(right.value) || !right.value.length));
  // A new native public response means the inputs were not held constant.
  // Report that separately instead of mislabeling moving data as a mapper bug.
  const inputMismatch = requests.some(
    (request) => request.implementation === 'rust' && request.read && !request.sessionRead && !request.cacheHit,
  );
  return {
    status: transportBlocked
      ? 'BLOCKED'
      : inputMismatch
        ? 'INPUT_DIFF'
        : diffs.length
          ? 'DIFF'
          : blocked
            ? 'BLOCKED'
            : 'PASS',
    diffs,
  };
}
