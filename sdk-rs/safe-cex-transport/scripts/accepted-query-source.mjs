// Reference behavior for the independently authored query/diagnostic contracts.
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
const require = createRequire(import.meta.url);
const { load, root } = require('../../safe-cex-types/scripts/accounts-source.cjs');
const contracts = loadContracts();
const signature = load(path.join(root, 'packages/safe-cex/src/utils/signatureRejection.ts'));
const bits = value => {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  return view.getBigUint64(0).toString();
};
const number = text => {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt(text));
  return view.getFloat64(0);
};
function observe(value) {
  if (value === null) return {kind:'null'};
  if (typeof value === 'number') return {kind:'number', bits:bits(value)};
  if (typeof value === 'boolean') return {kind:'bool', value};
  if (typeof value === 'string') return {kind:'string', value};
  if (Array.isArray(value)) return {kind:'array', items:value.map(observe)};
  return {kind:'object', entries:Object.entries(value).map(([key, item]) => [key, observe(item)])};
}
const binance = contracts['safe_cex_transport::signature_rejection::is_binance_style_signature_rejection'].cases;
assert.equal(binance.length, 5);
for (const row of binance) {
  let actual;
  try { actual = {ok:true, rejected:signature.isBinanceStyleSignatureRejection(row.input)}; }
  catch (error) { actual = {ok:false, name:error.name, message:error.message}; }
  assert.deepEqual(actual, row.output, 'actual maintained signature predicate');
}
const raw = contracts['safe_cex_transport::query::deserialize_query_wire'].cases.filter(row => Object.hasOwn(row.input, 'raw'));
assert.equal(raw.length, 7);
assert.equal(new Set(raw.map(row => row.input.raw)).size, 7);
for (const row of raw) {
  let actual;
  try { actual = {ok:true, present:true, native:observe(JSON.parse(row.input.raw))}; }
  catch { actual = {ok:false}; }
  assert.deepEqual(actual, row.output, 'actual engine JSON values before projection');
}
const projections = contracts['safe_cex_transport::query::query_json_projection'].cases;
assert.equal(projections.length, 6);
for (const row of projections) {
  let value;
  switch (row.input.mode) {
    case 'undefined': value = undefined; break;
    case 'null': value = null; break;
    case 'number': value = number(row.input.bits); break;
    case 'array': value = [undefined, null, NaN, -0, '-0']; break;
    case 'object': value = {drop:undefined, zero:-0, huge:Infinity, text:'literal'}; break;
    default: throw Error('Unreviewed projection fixture');
  }
  const text = JSON.stringify(value);
  assert.deepEqual({present:text !== undefined, value:text === undefined ? null : JSON.parse(text)}, row.output, 'actual engine JSON projection');
}
console.log('18 actual-source/engine query and predicate cases passed; native F64 events remain native-only witnesses.');
