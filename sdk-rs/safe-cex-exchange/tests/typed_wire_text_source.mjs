import assert from 'node:assert/strict';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const {Store} = sourceStore();
const units = value => Array.from({length: value.length}, (_, index) => value.charCodeAt(index));
const market = id => ({id, symbol: id, base: 'BTC', quote: 'USDT', active: true,
  precision: {amount: 0.001, price: 0.1},
  limits: {amount: {min: 0.001, max: 100}, leverage: {min: 1, max: 100}}});
const cases = [];
for (const text of ['\ud800', '\udc00', '\ud800A\udc00', '\ud800\ud800', '😀', 'ordinary', '\ufffd']) {
  const rows = [market('before'), {...market('middle'), base: text,
    ['\ud800']: 'unknown sibling', info: {text, ['\udc00']: text}}, market('after')];
  const wire = JSON.stringify(rows);
  const parsed = JSON.parse(wire);
  const store = new Store();
  store.update({markets: parsed});
  assert.equal(store.markets.length, 3);
  assert.equal(store.markets[1].base, text);
  assert.equal(store.markets[1].info.text, text);
  assert.equal(store.markets[1].info['\udc00'], text);
  cases.push({wire, ids: store.markets.map(row => row.id), text: units(text),
    typedText: units(text.toWellFormed()), infoKey: [0xdc00]});
}
console.log(JSON.stringify({cases}));
