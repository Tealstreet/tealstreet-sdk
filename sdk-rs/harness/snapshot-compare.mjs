import { stable } from './tape.mjs';

export function duplicateSymbols(rows, label) {
  const seen = new Map();
  const errors = [];
  if (!Array.isArray(rows)) return [`${label}: rows missing`];
  rows.forEach((row, index) => {
    if (seen.has(row.symbol)) errors.push(`${label}: duplicate symbol ${row.symbol} at ${seen.get(row.symbol)} and ${index}`);
    else seen.set(row.symbol, index);
  });
  return errors;
}

export function valueDifferences(left, right, path) {
  if (JSON.stringify(stable(left)) === JSON.stringify(stable(right))) return [];
  if (left !== null && right !== null && typeof left === 'object' && typeof right === 'object' && Array.isArray(left) === Array.isArray(right)) {
    const errors = [];
    if (Array.isArray(left) && left.length !== right.length) errors.push(`${path}.length: ts=${left.length} rust=${right.length}`);
    for (const key of new Set([...Object.keys(left), ...Object.keys(right)]))
      errors.push(...valueDifferences(left[key], right[key], Array.isArray(left) ? `${path}[${key}]` : `${path}.${key}`));
    return errors;
  }
  return [`${path}: ts=${JSON.stringify(left)} rust=${JSON.stringify(right)}`];
}

export function compareSnapshots(left, right, label = 'snapshot') {
  const errors = [];
  for (const [impl, snapshot] of [['ts', left], ['rust', right]]) {
    if (!snapshot || typeof snapshot !== 'object') {
      errors.push(`${label}: ${impl} snapshot missing`);
      continue;
    }
    for (const field of ['markets', 'tickers']) errors.push(...duplicateSymbols(snapshot[field], `${label} ${impl} ${field}`));
    errors.push(...(snapshot.duplicateSymbols ?? []).map((error) => `${label} ${impl}: ${error}`));
  }
  if (!left || !right) return errors;
  for (const field of ['markets', 'tickers']) {
    const sorted = (rows) => [...(rows ?? [])].sort((a, b) => a.symbol.localeCompare(b.symbol));
    errors.push(...valueDifferences(sorted(left[field]), sorted(right[field]), `${label} ${field}`));
  }
  for (const field of ['books', 'account']) {
    errors.push(...valueDifferences(left[field], right[field], `${label} ${field}`));
  }
  return errors;
}

export function compareCheckpoints(left, right) {
  if (!Array.isArray(left) || !Array.isArray(right) || left.length === 0 || right.length === 0)
    return ['checkpoint snapshots missing'];
  const errors = [];
  if (left.length !== right.length) errors.push(`checkpoint count: ts=${left.length} rust=${right.length}`);
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const a = left[index];
    const b = right[index];
    for (const [impl, point] of [['ts', a], ['rust', b]]) {
      if (!Number.isInteger(point?.ordinal) || point.ordinal < 0 || !Number.isInteger(point?.recordIndex) || point.recordIndex < -1 || !Number.isFinite(point?.t))
        errors.push(`checkpoint ${index} ${impl}: invalid tape boundary`);
    }
    for (const key of ['ordinal', 'recordIndex', 't']) {
      if (a?.[key] !== b?.[key]) errors.push(`checkpoint ${index} ${key}: ts=${a?.[key]} rust=${b?.[key]}`);
    }
    errors.push(...compareSnapshots(a, b, `checkpoint ${index}`));
  }
  return errors;
}
