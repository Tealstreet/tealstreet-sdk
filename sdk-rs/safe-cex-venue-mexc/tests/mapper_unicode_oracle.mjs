import assert from 'node:assert/strict';
import {load} from './source.mjs';

const api = load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
if (process.env.MEXC_UNICODE_SOURCE_EXPORT_CONTROL === 'drop-lone') {
  const original = api.fromMexcSymbol;
  api.fromMexcSymbol = (value) => original(value).replace(/\ud800/g, '');
}

const names = [
  'normalizeMarket',
  'normalizeTicker',
  'normalizePosition',
  'normalizeOrder',
  'normalizePlanOrder',
  'normalizeStopOrder',
  'normalizeExecution',
];
const symbols = [
  ['ascii', 'BTC_USDT', [66, 84, 67, 85, 83, 68, 84]],
  ['bmp', '\u03a9_USDT', [937, 85, 83, 68, 84]],
  ['astral', '\ud83d\ude80_USDT', [55357, 56960, 85, 83, 68, 84]],
  ['internal-space', 'A\ufeff\u3000_USDT', [65, 65279, 12288, 85, 83, 68, 84]],
  ['lone', '\ud800_USDT', [55296, 85, 83, 68, 84]],
];
const units = (value) => Array.from({length: value.length}, (_, i) => value.charCodeAt(i));
const records = [];
let subjectCalls = 0;
let marketChildren = 0;

for (const name of names) {
  for (const [label, symbol, expectedUnits] of symbols) {
    const raw = {
      symbol,
      id: '42',
      orderId: '43',
      positionType: 1,
      side: 1,
      state: 1,
      vol: 5,
      holdVol: 7,
      price: 123,
      holdAvgPrice: 11,
      createTime: 1700000000000,
      contractSize: 1,
      leverage: 3,
    };
    let result;
    if (name === 'normalizeTicker') {
      const market = api.normalizeMarket(raw);
      marketChildren++;
      result = api.normalizeTicker(raw, market);
    } else {
      result = api[name](raw);
    }
    subjectCalls++;
    assert.ok(result, 'SOURCE materialized mapper non-null ' + name + '/' + label);
    assert.equal(typeof result.symbol, 'string', 'SOURCE materialized mapper symbol type ' + name + '/' + label);
    assert.deepEqual(units(result.symbol), expectedUnits, 'SOURCE materialized mapper symbol units ' + name + '/' + label);
    records.push({name, label, symbolUnits: units(result.symbol)});
  }
}

assert.equal(subjectCalls, 35, 'SOURCE independently fixed Unicode subject call floor');
assert.equal(marketChildren, 5, 'SOURCE independently fixed Unicode ticker market child call floor');
assert.equal(records.length, 35, 'SOURCE independently fixed Unicode record floor');
console.log(JSON.stringify({subjectCalls, marketChildren, records}));
