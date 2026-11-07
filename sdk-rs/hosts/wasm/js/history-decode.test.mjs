import assert from 'node:assert/strict';
import test from 'node:test';
import { ExchangeError } from './host.mjs';

test('row decode evidence survives the host as closed schema/reason fields only', () => {
  const historyDecode = { schema: 'krakenOrders', reason: 'filledQuantity', raw: 'must not cross' };
  const error = new ExchangeError('invalid row', { kind: 'internal', causeCode: 'HISTORY_DECODE', historyDecode });
  assert.deepEqual(error.historyDecode, { schema: 'krakenOrders', reason: 'filledQuantity' });
});

test('unknown schema/reason and malformed evidence are discarded', () => {
  for (const historyDecode of [null, 'raw', { schema: 'https://private.invalid', reason: 'quantity' }, { schema: 'krakenOrders', reason: 'secret-value' }]) {
    assert.equal(new ExchangeError('invalid row', { historyDecode }).historyDecode, undefined);
  }
});
