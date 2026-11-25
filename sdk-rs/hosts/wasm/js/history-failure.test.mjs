import assert from 'node:assert/strict';
import test from 'node:test';
import { historyFailureOf } from './host.mjs';

test('only closed operation, kind and native-code classifications survive', () => {
  const operations = ['bybitClosedPnl', 'bybitOrders', 'bybitFills', 'bybitIncome', 'bybitTrades'];
  const kinds = ['rejected', 'invalidInput', 'insufficientBalance', 'authInvalid', 'authExpired',
    'authPermission', 'rateLimited', 'network', 'timeout', 'venueUnavailable', 'unsupported', 'internal'];
  const codes = [undefined, '10000', '10001', '10002', '10003', '10004', '10005', '10006', '10010',
    '10016', '10018', '10024', '10027', '10028', '10029', '100028', '33004', '429', 'other'];
  for (const operation of operations) for (const kind of kinds) for (const venueCode of codes) {
    const input = { operation, kind, ...(venueCode === undefined ? {} : { venueCode }) };
    const result = historyFailureOf({ ...input, message: 'PRIVATE_CANARY', row: { id: 'PRIVATE_CANARY' } });
    assert.deepEqual(result, input);
    assert.equal(JSON.stringify(result).includes('PRIVATE_CANARY'), false);
  }
});

test('malformed or unavailable evidence never invents a failure classification', () => {
  for (const value of [undefined, null, [], 'network', {}, { operation: 'bybitOrders' },
    { operation: 'bybitOrders', kind: 'PRIVATE_CANARY' },
    { operation: 'bybitOrders', kind: 'network', venueCode: 10006 },
    { operation: 'bybitOrders', kind: 'network', venueCode: '987654' },
    { operation: 'PRIVATE_CANARY', kind: 'network' },
    { get operation() { throw Error('PRIVATE_CANARY'); } },
  ]) assert.equal(historyFailureOf(value), undefined);
});
