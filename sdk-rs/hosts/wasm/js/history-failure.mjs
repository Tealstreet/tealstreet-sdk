const OPERATIONS = new Set(['bybitClosedPnl', 'bybitOrders', 'bybitFills', 'bybitIncome', 'bybitTrades']);
const KINDS = new Set(['rejected', 'invalidInput', 'insufficientBalance', 'authInvalid', 'authExpired',
  'authPermission', 'rateLimited', 'network', 'timeout', 'venueUnavailable', 'unsupported', 'internal']);
const CODES = new Set(['10000', '10001', '10002', '10003', '10004', '10005', '10006', '10010',
  '10016', '10018', '10024', '10027', '10028', '10029', '100028', '33004', '429', 'other']);

/** Copy closed read-failure classifications only; never native messages or values. */
export function historyFailureOf(value) {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const { operation, kind, venueCode } = value;
    if (!OPERATIONS.has(operation) || !KINDS.has(kind) || (venueCode !== undefined && !CODES.has(venueCode))) return undefined;
    return { operation, kind, ...(venueCode === undefined ? {} : { venueCode }) };
  } catch {
    return undefined;
  }
}
