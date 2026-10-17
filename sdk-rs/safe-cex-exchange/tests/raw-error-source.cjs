const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const root = path.resolve(__dirname, '../../..');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated TypeScript package required');
const isolated = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const ts = isolated('typescript');
function ast(relative) {
  const filename = path.join(root, relative);
  return ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
}
function compile(text) {
  return ts.transpileModule(text, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
}
const base = ast('packages/safe-cex/src/exchanges/base.ts');
const owner = base.statements.find(x => ts.isClassDeclaration(x) && x.name?.text === 'BaseExchange');
const method = owner.members.find(x => x.name?.getText(base) === 'emitAndLogError');
assert(method);
const correlation = ast('packages/safe-cex/src/utils/httpFailureContext.ts');
const read = correlation.statements.find(x => ts.isVariableStatement(x) && x.declarationList.declarations.some(d => d.name.getText(correlation) === 'readHttpFailureContext'));
assert(read);
const enhancer = ast('packages/safe-cex/src/utils/axiosErrorEnhancer.ts');
const formatter = enhancer.statements.find(x => ts.isFunctionDeclaration(x) && x.name?.text === 'formatErrorForSentry');
assert(formatter);
const source = compile(`${read.getText(correlation).replace(/^export /, '')}\n${formatter.getText(enhancer).replace(/^export /, '')}\nclass Reporter { ${method.getText(base)} }\n`);
let cases = 0;
const getterProfiles = [];
for (const worker of [false, true]) {
  const events = [], reports = [], trace = [];
  const env = {self: {}, window: worker ? undefined : {}, console: {
    error: (...args) => trace.push(['console', ...args]), trace: () => trace.push(['trace']), warn: () => trace.push(['warn']),
  }, reportException: (error, data) => reports.push({error, data})};
  const report = vm.runInNewContext(`${source}; Reporter.prototype.emitAndLogError`, env);
  const host = {isDisposed: false, name: 'source', options: {}, _lastHttpFailure: undefined,
    emitter: {emit: (...args) => events.push(args)}};
  report.call(host, 'plain', undefined, {suppressConsoleLog: true});
  const metadata = events.at(-1)[2];
  assert.deepEqual(Object.keys(metadata), worker ? ['sendToSentry', 'sentryData', 'enhancedContext', 'suppressConsoleLog'] : ['enhancedContext', 'suppressConsoleLog']);
  assert(Object.hasOwn(metadata, 'enhancedContext'));
  assert.equal(metadata.enhancedContext, undefined);
  const sentry = worker ? metadata.sentryData : reports.at(-1).data;
  assert.deepEqual(Object.keys(sentry), worker ? ['error', 'tags', 'extra'] : ['tags', 'extra']);
  const options = sentry.extra.exchangeOptions;
  assert.deepEqual(Object.keys(options), ['testnet', 'publicDataOnly']);
  assert(Object.hasOwn(options, 'testnet') && options.testnet === undefined);
  assert(Object.hasOwn(options, 'publicDataOnly') && options.publicDataOnly === undefined);
  cases++;
  for (const [input, expected] of [
    [undefined, 'Unknown Exchange Error'], [null, 'Unknown Exchange Error'],
    [false, 'Unknown Exchange Error'], [-0, 'Unknown Exchange Error'],
    [NaN, 'Unknown Exchange Error'], ['', 'Unknown Exchange Error'],
    [true, 'true'], [17, '17'], [Infinity, 'Infinity'], [-Infinity, '-Infinity'],
    ['plain', 'Exchange Error: plain'], [Symbol.iterator, 'Symbol(Symbol.iterator)'],
  ]) {
    report.call(host, input, undefined, {suppressConsoleLog: true});
    assert.equal(events.at(-1)[1], expected);
    cases++;
  }
  const message = {native: Infinity, missing: undefined};
  const raw = {message, get code() { throw new Error('reporter must not eagerly read code'); }};
  report.call(host, raw, undefined, {suppressConsoleLog: true});
  assert.equal(events.at(-1)[1], message);
  assert.equal(worker ? events.at(-1)[2].sentryData.error : reports.at(-1).error, raw);
  cases++;
  const nativeCode = {native: Infinity, missing: undefined};
  const nativeContext = {errorCode: nativeCode};
  const nativeMessage = {zero: -0};
  const enhanced = {context: nativeContext, enhancedMessage: '\ud800a', originalError: {message: nativeMessage}};
  report.call(host, enhanced, undefined, {suppressConsoleLog: true});
  const enhancedData = worker ? events.at(-1)[2].sentryData : reports.at(-1).data;
  assert.equal(events.at(-1)[1], '\ud800a');
  assert.equal(events.at(-1)[2].enhancedContext, nativeContext);
  assert.equal(enhancedData.tags.errorCode, nativeCode);
  assert.equal(enhancedData.extra.originalMessage, nativeMessage);
  cases++;
  const reads = [];
  const observed = (label, values) => new Proxy(values, {get(target, key) {
    reads.push(`${label}.${String(key)}`); return Reflect.get(target, key);
  }});
  const config = observed('config', {baseURL: '/native', timeout: 20, headers: {Authorization: 'fixture'}});
  const context = observed('context', {isNetworkError: false, errorCode: 0, statusCode: 401,
    isTimeout: false, isCORS: false, url: '/native', method: 'GET', statusText: 'Unauthorized', timestamp: 'now'});
  const original = observed('original', {message: 'original'});
  const accessorError = observed('error', {context, enhancedMessage: 'enhanced', config, originalError: original, code: 'CODE'});
  report.call(host, accessorError, undefined, {suppressConsoleLog: true});
  getterProfiles.push({worker, reads});
  cases++;
  trace.length = 0;
  const coerced = {toString() { trace.push(['coerce']); return 'source object'; }};
  report.call(host, coerced);
  assert.equal(trace[0][0], 'console');
  assert.equal(trace[0][2], coerced);
  assert.equal(trace[1][0], 'trace');
  assert.equal(trace[2][0], 'coerce');
  assert.equal(events.at(-1)[1], 'source object');
  cases++;
}
assert.equal(cases, 34);
if (process.argv.includes('--trace')) process.stdout.write(JSON.stringify(getterProfiles));
else console.log('RAW-ERROR actual source: 34 worker/main profiles preserve own undefined/order, native message/error identity, accessor read order and console-before-coercion.');
