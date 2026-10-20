import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runAuditedFixer } from './oracle-audit.mjs';

test('undeclared transient mutations cannot be hidden by restoring the output', () => {
  const reference = { account: { balance: { total: 1 } } };
  const audit = runAuditedFixer('binance', reference, (output) => {
    output.account.balance.total = 2;
    output.account.balance.total = 1;
  });
  assert.equal(reference.account.balance.total, 1);
  assert.equal(audit.errors.length, 2);
  assert.equal(audit.total, 2);
  assert.equal(audit.counts['account.balance.total'], 2);
});

test('authorized nested writes and arrays preserve real reference semantics', () => {
  const reference = { requests: ['a'], account: { positions: [{ marginSize: 0 }], orders: [] } };
  const audit = runAuditedFixer('bybit', reference, (output) => {
    output.account.positions[0].marginSize = 3;
    output.requests.push('b');
    output.requests = output.requests.filter((request) => request !== 'a');
  });
  assert.deepEqual(audit.errors, []);
  assert.equal(reference.account.positions[0].marginSize, 3);
  assert.deepEqual(reference.requests, ['b']);
  assert.doesNotThrow(() => structuredClone(reference));
  assert.ok(audit.total >= 3);
});

test('new arrays containing observed rows are unwrapped before storing', () => {
  const reference = { account: { orders: [{ id: 'a', type: 'limit' }, { id: 'b', type: 'limit' }] } };
  const audit = runAuditedFixer('orderly', reference, (output) => {
    output.account.orders = output.account.orders.filter((order) => order.id === 'b');
    output.account.orders[0].type = 'conditional';
  });
  assert.deepEqual(audit.errors, []);
  assert.deepEqual(reference.account.orders, [{ id: 'b', type: 'conditional' }]);
  assert.doesNotThrow(() => structuredClone(reference));
});

test('descriptor values remain observed, including transient restoration', () => {
  const reference = { account: { balance: { total: 1 } } };
  const audit = runAuditedFixer('binance', reference, (output) => {
    const balance = Object.getOwnPropertyDescriptor(output.account, 'balance').value;
    balance.total = 2;
    balance.total = 1;
  });
  assert.equal(audit.errors.length, 2);
  assert.equal(audit.counts['account.balance.total'], 2);
});

test('newly assigned aliases cannot mutate the reference outside observation', () => {
  const reference = { account: { balance: { total: 1 } } };
  const audit = runAuditedFixer('binance', reference, (output) => {
    const copy = { total: 1 };
    output.account.balance = copy;
    copy.total = 2;
    assert.equal(output.account.balance.total, 1);
    copy.total = 1;
  });
  assert.deepEqual(audit.errors, []);
  assert.equal(reference.account.balance.total, 1);
});

test('descriptor, accessor, prototype and extensibility changes fail closed', () => {
  for (const mutate of [
    (output) => Object.defineProperty(output.account, 'balance', { configurable: false }),
    (output) => Object.defineProperty(output.account, 'balance', { get: () => ({ total: 2 }) }),
    (output) => Object.setPrototypeOf(output.account, {}),
    (output) => Object.preventExtensions(output.account),
  ]) assert.throws(() => runAuditedFixer('binance', { account: { balance: { total: 1 } } }, mutate), /undeclared/);
});
