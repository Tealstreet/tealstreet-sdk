// Actual MEXC source/worker bridge compared with the real Rust/Wasm browser post.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {isNativeError} from 'node:util/types';
import {ts, root, load} from '../../safe-cex-venue-mexc/tests/source.mjs';

assert(process.argv[2], 'provide wasm-bindgen mexc_reporting_worker_host.js');
const require = createRequire(import.meta.url);
const wasm = require(path.resolve(process.argv[2]));
const dependency = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../mexc-worker-host.cjs'));
const {TinyEmitter} = dependency('tiny-emitter');
function members(relative, className, names) {
  const filename = root + relative;
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === className);
  return names.map(name => {
    const member = klass.members.find(n => n.name?.getText(ast) === name);
    assert(member, `SOURCE actual ${className}.${name}`);
    return member.getText(ast);
  }).join('\n');
}
const base = members('packages/safe-cex/src/exchanges/base.ts', 'BaseExchange', ['emitAndLogError']);
const exchange = members('packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts', 'MexcExchange', ['fetchPositions', 'fetchBalance', 'validateAccount', 'getAccount']);
const bridge = members('packages/safe-cex/src/webworker/ExchangeWorker.ts', 'ExchangeWorker', ['_setupExchangeEventListeners']);
const enhancerFile = root + 'packages/safe-cex/src/utils/axiosErrorEnhancer.ts';
const enhancer = ts.createSourceFile(enhancerFile, fs.readFileSync(enhancerFile, 'utf8'), ts.ScriptTarget.Latest, true);
const formatter = enhancer.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'formatErrorForSentry');
assert(formatter, 'SOURCE actual formatter');
const compiled = ts.transpileModule(`class Venue {${base}\n${exchange}}\nclass Worker {${bridge}}\n${formatter.getText(enhancer)}\nglobalThis.Venue=Venue;globalThis.Worker=Worker;`,
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const http = load('packages/safe-cex/src/utils/httpFailureContext.ts');
function observation(post, expectError) {
  assert.equal(post.op, 'error');
  assert.equal(post.data.length, 2);
  assert.equal(post.data[1].sendToSentry, true);
  const error = post.data[1].sentryData.error;
  if (expectError) {
    assert(isNativeError(error), 'UI receives a real Error, never a JSON record');
    return {message: post.data[0], name: error.name, errorMessage: error.message};
  }
  assert.equal(typeof error, 'number');
  assert.equal(error, 7);
  assert.equal(JSON.stringify(error), '7');
  return {message: post.data[0], error, json: JSON.stringify(error)};
}
let cases = 0;
for (const numeric of [false, true]) for (const method of ['positions', 'validate', 'account']) {
  const before = [], received = [];
  let calls = 0;
  const context = {exports: {}, recordHttpFailure: http.recordHttpFailure, readHttpFailureContext: http.readHttpFailureContext,
    reportException() { throw Error('source worker must post'); }, self: {}, window: undefined, Date: {now: () => 100},
    console: {error() {}, trace() {}}, ENDPOINTS: {OPEN_POSITIONS: '/api/v1/private/position/open_positions', ASSETS: '/api/v1/private/account/assets'}};
  vm.runInNewContext(compiled, context);
  const venue = new context.Venue(), emitter = new TinyEmitter();
  Object.assign(venue, {name: 'mexc', options: {key: 'key', secret: 'secret', publicDataOnly: false}, isDisposed: false, isPublicDataOnly: false,
    emitter, on: emitter.on.bind(emitter), store: {positions: [], subscribe() {}},
    xhr: {async get() { calls++; throw Object.assign(new Error('transport'), {response: {data: {message: numeric ? 7 : 'detail'}}}); }}});
  const worker = new context.Worker();
  Object.assign(worker, {_exchange: venue, _ctx: {postMessage(post) {before.push(post); received.push(structuredClone(post));}}});
  worker._setupExchangeEventListeners();
  try { await venue[method === 'positions' ? 'fetchPositions' : method === 'validate' ? 'validateAccount' : 'getAccount'](); }
  catch { assert.equal(method, 'account', 'only getAccount propagates the scripted failure'); }
  assert.equal(calls, 1, 'source MEXC HTTP child floor');
  assert.equal(before.length, 1, 'source installed worker bridge posts once');
  const nativeBefore = [], nativeReceived = [];
  wasm.post_mexc_reporting_data(post => {nativeBefore.push(post); nativeReceived.push(structuredClone(post));}, numeric, method);
  assert.equal(nativeBefore.length, 1, 'native MEXC installed worker bridge posts once');
  assert.equal(nativeReceived.length, 1);
  assert.deepEqual(observation(nativeBefore[0], !numeric), observation(before[0], !numeric), `actual pre-post Error getters ${method}`);
  assert.deepEqual(observation(nativeReceived[0], !numeric), observation(received[0], !numeric), `actual UI structured clone ${method}`);
  if (!numeric) {
    assert.notEqual(nativeBefore[0].data[1].sentryData.error, nativeReceived[0].data[1].sentryData.error);
    assert.equal(nativeBefore[0].data[1].sentryData.error.name, 'ExchangeError');
    assert.equal(nativeReceived[0].data[1].sentryData.error.name, 'Error', 'native structuredClone resets custom names exactly as TS');
    assert.equal(nativeReceived[0].data[1].sentryData.error.message, 'Exchange Error: detail');
  }
  cases++;
}
assert.equal(cases, 6);
console.log('MEXC production constructor/readers -> installed worker bridge -> actual WASM browser post -> UI structuredClone: 6 source-equivalent Error/name/message/Number cases PASS');
