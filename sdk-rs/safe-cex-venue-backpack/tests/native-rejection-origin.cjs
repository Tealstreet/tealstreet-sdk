'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function unexpected(id) { return new Proxy(function () { throw Error('unexpected ' + id); }, {get(_t, key) { if (key === '__esModule') return false; throw Error('unexpected ' + id); }}); }
function load(file, require) {
  const exports = {};
  const result = ts.transpileModule(fs.readFileSync(path.join(root, file), 'utf8'), {
    reportDiagnostics: true, compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
  });
  assert.equal(result.diagnostics.length, 0);
  vm.runInNewContext(result.outputText, {exports, require});
  return exports;
}
const types = load('packages/safe-cex/src/types.ts', id => id === '@tealstreet/logging/src/LogSeverity'
  ? load('packages/logging/src/LogSeverity.ts', unexpected) : unexpected(id));
const constants = load('packages/safe-cex/src/exchanges/backpack/backpack.types.ts', unexpected);
class Base { constructor(name, options, store) { this.options = options; this.store = store; } createXhrWithSecondaryKey() { return {}; } }
class Socket {}
const Exchange = load('packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts', id => {
  if (id === '../../types') return types;
  if (id === './backpack.types') return constants;
  if (id === '../base') return {BaseExchange: Base};
  if (id === './backpack.api') return {createAPI: unexpected(id)};
  if (id.includes('backpack.ws-')) return {BackpackPublicWebSocket: Socket, BackpackPrivateWebSocket: Socket};
  if (id === 'axios-rate-limit') return x => x;
  return unexpected(id);
}).BackpackExchange;
const bytes = new ArrayBuffer(8), view = new DataView(bytes);
view.setBigUint64(0, 0x7ff8000000000042n);
const values = [{name: '\ud800', message: '\udc00', code: {nan: NaN, zero: -0}, undefined}, -0, view.getFloat64(0), '\udfff'];
(async () => {
  const result = [];
  for (const value of values) {
    let reads = 0, http = 0;
    const row = Object.defineProperty({}, 'triggerPrice', {get() { reads++; throw value; }});
    const exchange = new Exchange('backpack', {}, {});
    exchange.xhr = {get() { http++; throw Error('unexpected HTTP'); }, post() { http++; throw Error('unexpected HTTP'); }, delete() { http++; throw Error('unexpected HTTP'); }};
    let returned;
    try { await exchange.fixTriggerOrdersWithoutTriggerBy([row]); assert.fail('source must reject'); } catch (error) { returned = error; }
    assert.ok(Object.is(returned, value));
    let bits = null;
    if (typeof returned === 'number') { view.setFloat64(0, returned); bits = view.getBigUint64(0).toString(16).padStart(16, '0'); }
    result.push({same: true, reads, http, type: typeof returned, bits, units: typeof returned === 'string' ? Array.from({length: returned.length}, (_, i) => returned.charCodeAt(i)) : null, json: JSON.stringify(returned)});
  }
  process.stdout.write(JSON.stringify(result));
})().catch(error => { console.error(error); process.exitCode = 1; });
