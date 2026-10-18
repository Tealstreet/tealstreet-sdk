import assert from 'node:assert/strict';
import {sourceStore} from './store_source.mjs';

const {Store} = sourceStore();
const fields = [
  ['parentId', 'old-parent', 'new-parent'],
  ['status', 'open', 'canceled'],
  ['type', 'limit', 'market'],
  ['side', 'buy', 'sell'],
  ['price', 11, 23],
  ['close', true, false],
  ['info', {source: 'old'}, null],
  ['clientOrderId', 'old-client', 'new-client'],
  ['timeInForce', 'GoodTillCancel', 'PostOnly'],
  ['triggerType', 'mark_price', 'last_price'],
  ['tpTriggerType', 'index_price', 'mark_price'],
  ['slTriggerType', 'last_price', 'index_price'],
  ['takeProfit', 31, 47],
  ['stopLoss', 19, 13],
  ['extra', {chaserId: 'old-chaser'}, {chaserId: 'new-chaser'}],
];
const base = amount => ({id: 'o', symbol: 'BTC', amount, filled: 0,
  remaining: amount, reduceOnly: false, lastUpdate: 1});
const cases = [];
for (const method of ['addOrUpdateOrder', 'addOrUpdateOrders']) {
  for (const [field, oldValue, newValue] of fields) {
    for (let oldState = 0; oldState < 3; oldState++) {
      for (let newState = 0; newState < 3; newState++) {
        const old = base(2);
        const incoming = base(3);
        if (oldState) old[field] = oldState === 1 ? undefined : oldValue;
        if (newState) incoming[field] = newState === 1 ? undefined : newValue;
        const store = new Store();
        store.addOrUpdateOrder(old);
        store[method](method === 'addOrUpdateOrders' ? [incoming] : incoming);
        const actual = store.orders[0];
        const state = newState || oldState;
        assert.equal(Object.hasOwn(actual, field), state !== 0, `${method} ${field} presence`);
        assert.deepEqual(actual[field], state === 2 ? (newState ? newValue : oldValue) : undefined);
        assert.equal(actual.amount, 3, 'carried required fields still overwrite');
        cases.push({method, field, oldState, newState, oldValue, newValue,
          state: !Object.hasOwn(actual, field) ? 0 : actual[field] === undefined ? 1 : 2,
          value: actual[field] === undefined ? null : actual[field]});
      }
    }
  }
}
assert.equal(fields.length, 15);
assert.equal(cases.length, 270);
process.stdout.write(JSON.stringify(cases));
