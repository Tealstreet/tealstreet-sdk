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


test('BingX field/type diagnostics cross the host without row values or mutable aliases', () => {
  const shape = { field: 'row.income', expected: 'numberOrNumericString', actual: 'null', envelopeCode: 'present', raw: 'private-amount-canary' };
  const error = new ExchangeError('BingX history response could not be decoded', { historyDecode: { schema: 'bingxIncome', reason: 'fieldType', shape, row: { id: 'private-id-canary' } } });
  assert.deepEqual(error.historyDecode, { schema: 'bingxIncome', reason: 'fieldType', shape: { field: 'row.income', expected: 'numberOrNumericString', actual: 'null', envelopeCode: 'present' } });
  shape.actual = 'object';
  assert.equal(error.historyDecode.shape.actual, 'null');
  assert.equal(JSON.stringify(error.historyDecode).includes('private-'), false);
});

test('BingX envelope diagnostic distinguishes an absent code from invalid JSON', () => {
  for (const [field, actual, envelopeCode] of [['code', 'missing', 'absent'], ['root', 'invalidJson', 'unavailable']]) {
    const shape = { field, expected: field === 'code' ? 'integer' : 'object', actual, envelopeCode };
    const detail = { schema: 'bingxIncome', reason: field === 'code' ? 'envelopeCode' : 'envelopeShape', shape };
    assert.deepEqual(new ExchangeError('decode failed', { historyDecode: detail }).historyDecode, detail);
  }
});

test('BingX unknown field paths, types, envelope states and hostile getters refuse', () => {
  const shape = { field: 'row.income', expected: 'numberOrNumericString', actual: 'null', envelopeCode: 'present' };
  for (const key of ['field', 'expected', 'actual', 'envelopeCode']) {
    const bad = { ...shape, [key]: 'private-value-canary' };
    assert.equal(new ExchangeError('decode failed', { historyDecode: { schema: 'bingxIncome', reason: 'fieldType', shape: bad } }).historyDecode, undefined);
  }
  const hostile = { get field() { throw new Error('private-getter-canary'); } };
  assert.equal(new ExchangeError('decode failed', { historyDecode: { schema: 'bingxIncome', reason: 'fieldType', shape: hostile } }).historyDecode, undefined);
});
