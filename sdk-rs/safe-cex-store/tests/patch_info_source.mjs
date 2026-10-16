import assert from 'node:assert/strict';
import {sourceStore} from './store_source.mjs';

const {Store} = sourceStore();
let count = 0;
for (const operation of ['order', 'position', 'positions', 'market']) {
  const store = new Store();
  store.update({
    orders: [{id: 'o', symbol: 'BTC', price: 1, status: 'open'}],
    positions: [{id: 'p', symbol: 'BTC', entryPrice: 1}],
    markets: [{id: 'm', symbol: 'BTC'}],
  });
  const info = {'$serde_json::private::Number': '17'};
  let row;
  if (operation === 'order') {
    store.updateOrder({id: 'o'}, {info});
    row = store.orders[0];
    assert.equal(row.price, 1, 'source order sibling');
  } else if (operation === 'market') {
    store.updateMarket({id: 'm'}, {info});
    row = store.markets[0];
  } else {
    if (operation === 'position') store.updatePosition({id: 'p'}, {info});
    else store.updatePositions([[{id: 'p'}, {info}]]);
    row = store.positions[0];
    assert.equal(row.entryPrice, 1, 'source position sibling');
  }
  assert.equal(row.symbol, 'BTC', 'source symbol sibling');
  assert.equal(row.info, info, 'source retains actual object identity');
  assert.deepEqual(Object.keys(row.info), ['$serde_json::private::Number']);
  assert.equal(row.info['$serde_json::private::Number'], '17');
  count += 1;
}
assert.equal(count, 4, 'independent source mutation-path floor');
console.log('STORE-INFO source assertions: 4 paths');
