// Adapt sparse historical reference-fill oracles to the full comparison shape.
// Only the actual TS snapshot and the input-derived oracle enter this module.
import { isDeepStrictEqual } from 'node:util';

export const LEGACY_FILL_METADATA = Object.freeze(['executionId', 'fee', 'timestamp']);
const hasEvidence = (row) => LEGACY_FILL_METADATA.some((field) => row[field] !== null && row[field] !== undefined);
const hasPrice = (row) => Number.isFinite(row.price);
const sameIdentity = (left, right) => left.id != null && right.id != null
  && String(left.id) === String(right.id) && left.symbol === right.symbol
  && left.side === right.side && Object.is(left.amount, right.amount)
  && (!hasPrice(left) || !hasPrice(right) || Object.is(left.price, right.price));

/** Match whole executions, never cumulative amounts or ambiguous equal lots. */
export function matchLegacyFillRows(previous, next) {
  const candidates = next.map((row) => previous.flatMap((source, index) => sameIdentity(source, row) ? [index] : []));
  const uses = previous.map(() => 0);
  for (const choices of candidates) if (choices.length === 1) uses[choices[0]]++;
  const sourceIndices = candidates.map((choices) => choices.length === 1 && uses[choices[0]] === 1 ? choices[0] : undefined);
  const targetIndices = previous.map(() => undefined);
  for (const [target, source] of sourceIndices.entries()) if (source !== undefined) targetIndices[source] = target;
  return {
    sourceIndices, targetIndices,
    ambiguous: candidates.some((choices) => choices.length > 1) || uses.some((count) => count > 1),
  };
}

const complete = (row, source) => ({
  ...source, ...row,
  price: Object.hasOwn(row, 'price') ? row.price ?? null : source?.price ?? null,
  ...Object.fromEntries(LEGACY_FILL_METADATA.map((field) => [field, source?.[field] ?? null])),
});

// Audited reference objects are Proxies, which structuredClone cannot read.
// Traverse their data properties through the observer instead of unwrapping it
// or serializing away undefined, NaN or negative zero. Every cloned object is
// detached; accessors and non-data containers have no replay JSON meaning.
export function cloneOracleData(value, active = new Set(), depth = 0) {
  if (value === null || typeof value !== 'object') {
    if (typeof value === 'function' || typeof value === 'symbol')
      throw new Error('Legacy fill oracle requires plain replay data');
    return value;
  }
  if (depth > 128 || active.has(value)) throw new Error('Legacy fill oracle requires bounded acyclic replay data');
  const prototype = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null)
    throw new Error('Legacy fill oracle requires plain replay data');
  const copy = Array.isArray(value) ? new Array(value.length) : Object.create(prototype);
  active.add(value);
  try {
    for (const key of Reflect.ownKeys(value)) {
      if (Array.isArray(value) && key === 'length') continue;
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (typeof key !== 'string' || !descriptor || !Object.hasOwn(descriptor, 'value'))
        throw new Error('Legacy fill oracle requires data properties');
      Object.defineProperty(copy, key, {
        value: cloneOracleData(descriptor.value, active, depth + 1),
        enumerable: descriptor.enumerable, configurable: true, writable: true,
      });
    }
  } finally {
    active.delete(value);
  }
  return copy;
}

/**
 * Run a legacy oracle on a detached shadow, then publish its net TS changes.
 * Ambiguous execution evidence keeps the actual TS fills rather than guessing
 * metadata allocation. Exceptions publish nothing, including non-fill changes.
 */
export function applyLegacyFillOracle(reference, oracle) {
  const shadow = cloneOracleData(reference);
  const original = cloneOracleData(shadow.account?.fills);
  if (!Array.isArray(original)) throw new Error('Legacy fill oracle requires actual TS fill rows');
  shadow.account.fills = original.map((row) => Object.fromEntries(
    Object.entries(row).filter(([field]) => !LEGACY_FILL_METADATA.includes(field)),
  ));
  oracle(shadow);
  if (!Array.isArray(shadow.account?.fills)) throw new Error('Legacy fill oracle removed the TS fill section');
  for (const row of shadow.account.fills)
    if (LEGACY_FILL_METADATA.some((field) => Object.hasOwn(row, field)))
      throw new Error('Legacy fill oracle attempted to write protected execution metadata');
  const matches = matchLegacyFillRows(original, shadow.account.fills);
  const preserveRaw = matches.ambiguous || original.some((row, index) => hasEvidence(row) && matches.targetIndices[index] === undefined);
  shadow.account.fills = preserveRaw
    ? original.map((row) => complete(row, row))
    : shadow.account.fills.map((row, index) => complete(row, original[matches.sourceIndices[index]]));
  // Catch metadata-loss bugs in the adapter itself independently of the field
  // permission auditor. A non-null observed execution must survive exactly once.
  if (!preserveRaw)
    for (const [index, row] of original.entries()) {
      if (!hasEvidence(row)) continue;
      const target = shadow.account.fills[matches.targetIndices[index]];
      for (const field of LEGACY_FILL_METADATA)
        if (!isDeepStrictEqual(row[field] ?? null, target[field]))
          throw new Error('Legacy fill oracle changed observed TS execution metadata');
    }
  for (const key of Object.keys(reference)) if (!Object.hasOwn(shadow, key)) delete reference[key];
  Object.assign(reference, shadow);
  return { preservedRawFills: preserveRaw };
}
