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

test('precise Kraken predicates survive serialization without arbitrary row data', () => {
  for (const reason of ['quantityNegative', 'quantityZero', 'filledQuantityNegative',
    'filledQuantityExceedsQuantity', 'quantityDecimal', 'filledQuantityDecimal',
    'timestampDecimal', 'lastUpdateTimestampDecimal', 'limitPriceDecimal']) {
    const detail = { schema: 'krakenOrders', reason, uid: 'private-id', quantity: 'private-amount' };
    const error = new ExchangeError('Kraken order history rejected', { historyDecode: detail });
    assert.deepEqual(error.historyDecode, { schema: 'krakenOrders', reason });
    assert.equal(JSON.stringify(error.historyDecode).includes('private-'), false);
  }
});
