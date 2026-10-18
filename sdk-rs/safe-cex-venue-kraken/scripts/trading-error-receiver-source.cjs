'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const src = path.join(root, 'packages/safe-cex/src');
const sourceFile = path.join(src, 'exchanges/kraken/kraken.exchange.ts');
const source = fs.readFileSync(sourceFile, 'utf8');
const ast = ts.createSourceFile(sourceFile, source, ts.ScriptTarget.Latest, true);
const classes = ast.statements.filter(s => ts.isClassDeclaration(s) && s.name.text === 'KrakenExchange');
assert.equal(classes.length, 1);
function transpile(text) {
  const result = ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return result.outputText;
}
const cache = new Map();
function load(file) {
  assert.ok(file.startsWith(src + path.sep) || file === path.join(root, 'packages/logging/src/LogSeverity.ts'));
  if (cache.has(file)) return cache.get(file);
  const exports = {};
  cache.set(file, exports);
  vm.runInNewContext(transpile(fs.readFileSync(file, 'utf8')), { exports, require(name) {
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert.ok(name.startsWith('.'), `unexpected runtime dependency ${name}`);
    return load(path.resolve(path.dirname(file), name) + '.ts');
  } }, { filename: file });
  return exports;
}
const types = load(path.join(src, 'types.ts'));
const { ENDPOINTS } = load(path.join(src, 'exchanges/kraken/kraken.types.ts'));
const { LogSeverity } = load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
function method(name, receiver) {
  const members = classes[0].members.filter(m => m.name?.getText(ast) === name);
  assert.equal(members.length, 1);
  assert.ok(ts.isPropertyDeclaration(members[0]) && ts.isArrowFunction(members[0].initializer));
  return vm.runInNewContext(transpile(`function capture(){return (${members[0].initializer.getText(ast)});}`) + ';capture', { ...types, ENDPOINTS, LogSeverity }, { filename: sourceFile }).call(receiver);
}
(async () => {
  let lowerGets = 0, lowerThis, emitted;
  const message = {};
  Object.defineProperty(message, 'toLowerCase', { get() {
    lowerGets += 1;
    return function () { assert.equal(arguments.length, 0); lowerThis = this; return 'ordinary'; };
  } });
  const original = { message };
  const place = method('placeOrder', {
    resolveCallerClientOrderId() {},
    store: { get marketsBySymbol() { throw original; } },
    emitAndLogError(...args) { emitted = args; },
  });
  await assert.rejects(place({ type: types.OrderType.Market, symbol: 'X' }), error => error.message === '[object Object]');
  assert.equal(lowerGets, 1);
  assert.equal(lowerThis, message);
  assert.equal(emitted[0], message);
  assert.equal(emitted[1], 'PlaceOrder Error');

  let upperGets = 0, upperThis, sent;
  const upperResult = { native: Infinity };
  const symbol = { toString() { return 'X'; } };
  Object.defineProperty(symbol, 'toUpperCase', { get() {
    upperGets += 1;
    return function () { assert.equal(arguments.length, 0); upperThis = this; return upperResult; };
  } });
  const leverage = new Map();
  const set = method('setLeverage', {
    store: { marketsBySymbol: { X: { limits: { leverage: { min: 1, max: 100 } } } } },
    xhr: { async put(endpoint, body) { assert.equal(endpoint, ENDPOINTS.LEVERAGE_PREFERENCES); sent = body; return { data: { result: 'success' } }; } },
    leverageSettings: leverage,
    log() {}, emitAndLogError() { assert.fail('unexpected source error'); },
  });
  await set(symbol, 5);
  assert.equal(upperGets, 1);
  assert.equal(upperThis, symbol);
  assert.equal(sent.symbol, upperResult);
  assert.equal(leverage.get(symbol), 5);
  console.log(JSON.stringify({ sourceSha256: crypto.createHash('sha256').update(source).digest('hex'), lowerGets, lowerReceiver: true, lowerResultForwarded: true, upperGets, upperReceiver: true, upperNativeResult: true }));
})().catch(error => { console.error(error); process.exitCode = 1; });
