// Executes the actual txflow TypeScript with stubbed axios factory and BaseExchange.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('isolated TypeScript compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dir = path.join(__dirname, '../../../packages/safe-cex/src/exchanges/txflow');
function load(name, imports) {
  const exports = {};
  const source = fs.readFileSync(path.join(dir, name), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(js, { exports, Error, require: (id) => {
    assert.ok(Object.hasOwn(imports, id), `unexpected import ${id} in ${name}`);
    return imports[id];
  } }, { filename: name });
  return exports;
}
const types = load('txflow.types.ts', {});
const factoryCalls = [];
const safeCexAxiosCreate = (config, options, exchange) => {
  factoryCalls.push({ config, options, exchange });
  return { client: factoryCalls.length };
};
const api = load('txflow.api.ts', { '../../safeCexAxiosCreate': { safeCexAxiosCreate }, './txflow.types': types });
const factory = [];
for (const testnet of [true, false, undefined]) {
  const options = testnet === undefined ? { key: 'k' } : { key: 'k', testnet };
  const exchange = { marker: 'exchange' };
  factoryCalls.length = 0;
  api.createAPI(options, exchange);
  assert.equal(factoryCalls.length, 1);
  assert.equal(factoryCalls[0].options, options, 'options passed by identity');
  assert.equal(factoryCalls[0].exchange, exchange, 'exchange passed by identity');
  factory.push({ testnet: testnet ?? null, config: factoryCalls[0].config });
}
const events = [];
class BaseExchange {
  constructor(name, opts, store) { events.push(['super', name]); this.opts = opts; this.store = store; }
  rejectPreflight(message) { events.push(['rejectPreflight', message]); throw new Error(message); }
}
const apiCalls = [];
const exchangeModule = load('txflow.exchange.ts', {
  '../base': { BaseExchange },
  './txflow.api': { createAPI: (opts, exchange) => { apiCalls.push(exchange instanceof BaseExchange); return { xhr: true }; } },
  './txflow.types': types,
});
const instance = new exchangeModule.TxFlowExchange('txflow', { key: 'k', testnet: true }, {});
assert.deepEqual(apiCalls, [true], 'createAPI receives the exchange instance');
assert.deepEqual(instance.xhr, { xhr: true });
let synchronous = false, message;
try { instance.start(); } catch (error) { synchronous = true; message = error.message; }
console.log(JSON.stringify({
  constants: {
    mainnet: types.TXFLOW_MAINNET_REST_URL,
    testnet: types.TXFLOW_TESTNET_REST_URL,
    unimplemented: types.TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE,
  },
  factory,
  start: { synchronous, message, events },
}));
