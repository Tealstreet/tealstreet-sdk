'use strict';
// Execute the maintained class arrows; replace only terminal HTTP/Store/reporters.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const receipts = [];
const modules = new Map();
function load(relative) {
  const file = path.join(root, relative);
  if (modules.has(file)) return modules.get(file);
  const exports = {};
  modules.set(file, exports);
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  vm.runInNewContext(code, {exports, require(name) {
    if (name === '@tealstreet/logging/src/LogSeverity') return load('packages/logging/src/LogSeverity.ts');
    assert.ok(name.startsWith('.'), `unexpected source dependency ${name}`);
    return load(path.relative(root, path.resolve(path.dirname(file), name) + '.ts'));
  }}, {filename: file});
  return exports;
}
const futuresTypes = load('packages/safe-cex/src/exchanges/kraken/kraken.types.ts');
const spotTypes = load('packages/safe-cex/src/exchanges/kraken/krakenspot.types.ts');
const {LogSeverity} = load('packages/logging/src/LogSeverity.ts');
function method(filename, className, member, receiver, bindings) {
  const file = path.join(root, 'packages/safe-cex/src/exchanges/kraken', filename);
  const text = fs.readFileSync(file, 'utf8');
  const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name.text === className);
  assert.equal(classes.length, 1);
  const arrows = classes[0].members.filter(node => node.name?.getText(ast) === member);
  assert.equal(arrows.length, 1);
  assert.ok(ts.isPropertyDeclaration(arrows[0]) && ts.isArrowFunction(arrows[0].initializer));
  const code = ts.transpileModule(`function make(){return (${arrows[0].initializer.getText(ast)});}`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}, reportDiagnostics: true,
  });
  assert.equal((code.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  receipts.push({filename, member, sha256: crypto.createHash('sha256').update(text).digest('hex')});
  return vm.runInNewContext(code.outputText + ';make', bindings, {filename: file}).call(receiver);
}
async function main() {
  for (const [filename, className, member, bindings, opts, body] of [
    ['kraken.exchange.ts', 'KrakenExchange', 'fetchTickers', {ENDPOINTS: futuresTypes.ENDPOINTS}, [], {tickers: []}],
    ['krakenspot.exchange.ts', 'KrakenSpotExchange', 'fetchOHLCV', {ENDPOINTS: spotTypes.ENDPOINTS, INTERVAL: spotTypes.INTERVAL}, [{symbol: 'BTC/USD', interval: '1m'}], {result: {}}],
  ]) {
    const calls = [];
    let release;
    const pending = new Promise(resolve => { release = resolve; });
    const receiver = {
      xhr: {get(...args) { calls.push(args); return pending; }},
      store: {marketsBySymbol: {}}, emitAndLogError() { assert.fail('unexpected source error'); },
    };
    const fn = method(filename, className, member, receiver, bindings);
    const waiter = fn(...opts);
    assert.equal(calls.length, 1, `${member} calls HTTP before returning its Promise`);
    release({data: body});
    assert.equal((await waiter).length, 0);
    assert.equal(calls.length, 1);
  }
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const calls = [], logs = [], errors = [];
  const receiver = {
    store: {marketsBySymbol: {PF_XBTUSD: {limits: {leverage: {min: 1, max: 50}}}}},
    leverageSettings: new Map(),
    xhr: {put(...args) { calls.push(args); return pending; }},
    log(...args) { logs.push(args); }, emitAndLogError(...args) { errors.push(args); },
  };
  const fn = method('kraken.exchange.ts', 'KrakenExchange', 'setLeverage', receiver,
    {ENDPOINTS: futuresTypes.ENDPOINTS, LogSeverity});
  // Deliberately discard the returned Promise: its body still owns the continuation.
  fn('PF_XBTUSD', 500);
  assert.equal(calls.length, 1);
  assert.equal(calls[0][1].maxLeverage, 50);
  assert.equal(logs.length, 0);
  release({data: {result: 'success'}});
  await new Promise(setImmediate);
  assert.equal(receiver.leverageSettings.get('PF_XBTUSD'), 50);
  assert.equal(logs.length, 1);
  assert.equal(logs[0][0], 'Leverage set to 50x for PF_XBTUSD');
  const rejected = fn('UNKNOWN', 2);
  assert.equal(errors.length, 1, 'rejection reporting precedes returning the Promise');
  await assert.rejects(rejected, /Market UNKNOWN not found/);
  assert.equal(calls.length, 1);
  console.log(JSON.stringify({receipts, synchronousHttpPrefixes: 3, detachedContinuation: true, rejectionPrefix: true}));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
