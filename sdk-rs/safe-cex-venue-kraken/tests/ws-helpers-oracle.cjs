'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/kraken/kraken.ws-public.ts');
const source = fs.readFileSync(filename, 'utf8');
const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name.text === 'KrakenPublicWebsocket');
assert.equal(classes.length, 1);
const names = ['getTickerProductIds', 'mapTimeframe'];
const selected = names.map(name => {
  const members = classes[0].members.filter(node => node.name?.getText(ast) === name);
  assert.equal(members.length, 1, `actual helper ${name}`);
  return members[0].getText(ast);
});
const compiled = ts.transpileModule('class Probe {' + selected.join('\n') + '}; module.exports = Probe;', {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true
});
assert.equal((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const sandbox = {module: {exports: {}}};
vm.runInNewContext(compiled.outputText, sandbox, {filename});
const Probe = sandbox.module.exports;
const symbols = ['B', 'A', 'B', 'Off', 'constructor', '1', '1'];
const specs = [
  {symbols: [], active: []}, {symbols, active: []}, {symbols, active: ['A']},
  {symbols, active: ['B', 'B']}, {symbols, active: ['missing']},
  {symbols, active: ['constructor', '1']}, {symbols, active: ['1', 'A', 'B']},
  {symbols, active: ['B'], replace: true}, {symbols, active: [], fail: true},
];
const products = specs.map(spec => {
  const trace = [], original = new Error('active symbols failure');
  const probe = new Probe();
  probe.parent = {store: {markets: spec.symbols.map((symbol, i) => ({symbol, active: i !== 3}))},
    getActiveTickerSymbols() {
      trace.push('active');
      if (spec.replace) this.store.markets = [{symbol: 'CHANGED'}];
      if (spec.fail) throw original;
      return spec.active;
    }};
  try {
    const output = probe.getTickerProductIds();
    return {...spec, trace, output: Array.from(output), after: probe.parent.store.markets.map(m => m.symbol)};
  } catch (error) {
    return {...spec, trace, error: {message: error.message, same: error === original}, after: probe.parent.store.markets.map(m => m.symbol)};
  }
});
const keys = ['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '6h', '12h', '1d', '1w',
  'constructor', '__defineGetter__', '__defineSetter__', 'hasOwnProperty', '__lookupGetter__', '__lookupSetter__',
  'isPrototypeOf', 'propertyIsEnumerable', 'toString', 'valueOf', '__proto__', 'toLocaleString',
  'missing', '', ' 1m', '1m ', '😀', '\u0000', '\ud800', 'a\udfffz'];
const timeframeProbe = new Probe();
const timeframes = keys.map(key => {
  const output = timeframeProbe.mapTimeframe(key);
  const input = Array.from({length: key.length}, (_, i) => key.charCodeAt(i));
  if (typeof output === 'number') {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(output);
    return {input, output: {kind: 'number', bits: bytes.toString('hex')}};
  }
  if (typeof output === 'function') return {input, output: {kind: 'function', text: String(output), length: output.length}};
  assert.equal(output, vm.runInNewContext('Object.prototype', sandbox));
  return {input, output: {kind: 'object', identity: 'Object.prototype'}};
});
process.stdout.write(JSON.stringify({header: {node: process.version, v8: process.versions.v8, arch: process.arch,
  source: crypto.createHash('sha256').update(source).digest('hex')}, products, timeframes}));
