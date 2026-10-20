import { stable } from './tape.mjs';
import { comparesField } from './comparison-policy.mjs';

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

// Check the public/private snapshot contract even when both producers erase
// the same section. Schema failures share the non-exempt coverage prefix.
export function snapshotCoverageErrors(snapshot, label = 'snapshot', { isPrivate, venue } = {}) {
  const prefix = `checkpoint coverage ${label}:`;
  const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
  if (!object(snapshot)) return [`${prefix} snapshot object missing`];
  const errors = [];
  for (const field of ['markets', 'tickers']) if (!Array.isArray(snapshot[field]))
    errors.push(`${prefix} ${field} rows missing`);
  if (!object(snapshot.books)) errors.push(`${prefix} books object missing`);
  if (isPrivate === true && !object(snapshot.account)) errors.push(`${prefix} private account object missing`);
  if (isPrivate === false && snapshot.account !== undefined) errors.push(`${prefix} public snapshot contains account`);
  if (object(snapshot.account)) {
    const account = snapshot.account;
    const requireFields = (row, fields, path) => {
      if (!object(row)) { errors.push(`${prefix} ${path} row object missing`); return; }
      for (const field of fields) if (!Object.hasOwn(row, field) || row[field] === undefined) errors.push(`${prefix} ${path}.${field} missing`);
    };
    if (!Object.hasOwn(account, 'hedged') || typeof account.hedged !== 'boolean')
      errors.push(`${prefix} account.hedged boolean missing`);
    if (!Object.hasOwn(account, 'balance')) errors.push(`${prefix} account.balance missing`);
    else if (account.balance !== null) {
      if (!object(account.balance)) errors.push(`${prefix} account.balance object or null required`);
      else for (const field of ['used', 'free', 'total', 'upnl']) {
        if (!Object.hasOwn(account.balance, field)) errors.push(`${prefix} account.balance.${field} missing`);
        else if (account.balance[field] !== null && !Number.isFinite(account.balance[field]))
          errors.push(`${prefix} account.balance.${field} finite number or null required`);
      }
    }
    const rowFields = {
      orders: ['id', 'symbol', 'status', 'type', 'side', 'price', 'amount', 'filled', 'remaining', 'reduceOnly', 'close', 'trigger', 'rejectReason',
        'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId', 'takeProfitTrigger', 'stopLossTrigger',
        ...(comparesField(venue, 'clientOrderId') ? ['clientOrderId'] : [])],
      positions: ['symbol', 'side', 'entryPrice', 'contracts', 'notional', 'leverage', 'unrealizedPnl', 'liquidationPrice',
        'marginType', 'positionMode', 'marginSize', 'realizedPnl'],
      settings: ['symbol', 'marginType'],
      fills: ['id', 'symbol', 'side', 'amount', 'executionId', 'fee', 'timestamp', ...(comparesField(venue, 'fillPrice') ? ['price'] : [])],
      flatPositionPnl: ['symbol', 'side', 'positionMode', 'realizedPnl'],
    };
    for (const [section, fields] of Object.entries(rowFields)) {
      if (!Object.hasOwn(account, section) || !Array.isArray(account[section])) {
        errors.push(`${prefix} account.${section} rows missing`);
        continue;
      }
      for (const [index, row] of account[section].entries()) {
        const path = `account.${section}[${index}]`;
        requireFields(row, fields, path);
        if (!object(row)) continue;
        if (section === 'orders' && row.type === 'limit' && comparesField(venue, 'clientOrderId'))
          requireFields(row, ['timeInForce'], path);
        if (section === 'settings') {
          const own = (field) => Object.hasOwn(row, field) && row[field] !== undefined;
          const single = own('leverage') && !Object.hasOwn(row, 'buyLeverage') && !Object.hasOwn(row, 'sellLeverage');
          const independent = !Object.hasOwn(row, 'leverage') && own('buyLeverage') && own('sellLeverage');
          if (!single && !independent) errors.push(`${prefix} ${path} single leverage or buyLeverage/sellLeverage pair missing`);
        }
      }
    }
  }
  return errors;
}

// Equality between runners cannot establish coverage: both could omit the
// same quarter or command snapshot. Validate each stream against the original
// tape-derived plan before inspecting financial values or applying fixers.
export function checkpointCoverageErrors(actual, expected, label = 'replay', scope = {}) {
  const prefix = `checkpoint coverage ${label}:`;
  if (!Array.isArray(expected) || expected.length === 0) return [`${prefix} expected tape plan missing`];
  if (!Array.isArray(actual)) return [`${prefix} snapshots missing`];
  const errors = [];
  const seen = new Map();
  if (actual.length !== expected.length) errors.push(`${prefix} count ${actual.length}; expected ${expected.length}`);
  for (let index = 0; index < Math.max(actual.length, expected.length); index++) {
    const point = actual[index];
    const planned = expected[index];
    if (!point) {
      errors.push(`${prefix} missing point ${index} (${JSON.stringify(planned)})`);
      continue;
    }
    errors.push(...snapshotCoverageErrors(point, `${label} point ${index}`, scope));
    const valid = Number.isInteger(point.ordinal) && point.ordinal >= 0 && Number.isInteger(point.recordIndex) && point.recordIndex >= -1 && Number.isFinite(point.t);
    if (!valid) errors.push(`${prefix} invalid tape boundary at point ${index}`);
    else {
      const key = JSON.stringify([point.ordinal, point.recordIndex, point.t]);
      if (seen.has(key)) errors.push(`${prefix} duplicate points ${seen.get(key)} and ${index} (${key})`);
      else seen.set(key, index);
    }
    if (!planned) {
      errors.push(`${prefix} unexpected point ${index}`);
      continue;
    }
    if (!Number.isInteger(planned.ordinal) || !Number.isInteger(planned.recordIndex) || !Number.isFinite(planned.t))
      errors.push(`${prefix} invalid expected tape boundary at point ${index}`);
    for (const field of ['ordinal', 'recordIndex', 't']) if (point[field] !== planned[field])
      errors.push(`${prefix} point ${index} ${field}=${JSON.stringify(point[field])}; expected ${JSON.stringify(planned[field])}`);
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
