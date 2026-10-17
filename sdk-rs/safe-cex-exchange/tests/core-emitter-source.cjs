const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated TypeScript package required');
const isolated = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const ts = isolated('typescript');
const {TinyEmitter} = isolated('tiny-emitter');
const root = path.resolve(__dirname, '../../..');
const filename = path.join(root, 'packages/safe-cex/src/exchanges/base.ts');
const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BaseExchange');
assert(owner, 'maintained BaseExchange');
const initializer = owner.members.find(node => node.name?.getText(ast) === 'emitter' && node.initializer)?.initializer;
assert(initializer, 'maintained emitter initializer');
const emitter = vm.runInNewContext(initializer.getText(ast), {TinyEmitter});
const shared = {present: undefined, null: null, value: 17};
const values = [undefined, null, NaN, Infinity, -Infinity, -0, shared, '\ud800a'];
const events = [];
emitter.on('retained', function (...args) { events.push(args); });
emitter.emit('retained', ...values);
emitter.emit('retained');
assert.equal(events.length, 2);
assert.equal(events[0].length, 8);
assert.equal(events[1].length, 0);
values.forEach((value, i) => assert(Object.is(events[0][i], value)));
assert(Object.hasOwn(events[0][6], 'present'));
assert(!Object.hasOwn(events[0][6], 'absent'));
shared.value = 23;
assert.equal(events[0][6].value, 23);
const trace = [];
const nested = {value: 0};
emitter.once('once', value => { trace.push(value); emitter.emit('once', value); });
emitter.emit('once', nested);
emitter.emit('once', nested);
assert.equal(trace.length, 1);
assert.equal(trace[0], nested);
const marker = new Error('listener sentinel');
emitter.once('throw', () => { throw marker; });
emitter.on('throw', () => trace.push('tail'));
assert.throws(() => emitter.emit('throw', nested), error => error === marker);
assert.equal(trace.length, 1);
emitter.emit('throw', nested);
assert.deepEqual(trace.slice(1), ['tail']);
let codes = 0;
for (const code of [undefined, null, NaN, Infinity, -Infinity, -0, '\ud800', shared]) {
  const error = new Error('retained code');
  error.code = code;
  try { throw error; } catch (received) {
    assert(Object.hasOwn(received, 'code'));
    assert(Object.is(received.code, code));
    codes++;
  }
}
assert.equal(codes, 8);
console.log('CORE-EMITTER source: 8 retained arguments, missing arity, identity/UTF16, once/reentry/throw order, 8 native error codes');

const formatFile = path.join(root, 'packages/safe-cex/src/utils/axiosErrorEnhancer.ts');
const formatAst = ts.createSourceFile(formatFile, fs.readFileSync(formatFile, 'utf8'), ts.ScriptTarget.Latest, true);
const formatNode = formatAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'formatErrorForSentry');
assert(formatNode, 'maintained formatter');
const formatSource = ts.transpileModule(formatNode.getText(formatAst).replace(/^export /, ''), {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const format = vm.runInNewContext(`${formatSource}; formatErrorForSentry`);
for (const code of values) {
  const input = {code, context: {errorCode: 0}, config: {headers: {authorization: 'hidden'}}};
  const formatted = format(input);
  assert.deepEqual(Array.from(Object.keys(formatted.tags)), ['errorType','errorCode','statusCode','isTimeout','isCORS']);
  assert.deepEqual(Array.from(Object.keys(formatted.extra)), ['url','method','statusText','enhancedMessage','originalMessage','timestamp','config']);
  assert(Object.is(formatted.tags.errorCode, code));
  assert(Object.hasOwn(formatted.tags, 'statusCode'));
  assert.equal(formatted.tags.statusCode, undefined);
  assert(Object.hasOwn(formatted.extra.config, 'baseURL'));
  assert.equal(formatted.extra.config.baseURL, undefined);
  assert.deepEqual(Array.from(formatted.extra.config.headers), ['authorization']);
}
console.log('CORE-EMITTER formatter: 8 native codes, own undefined fields, source property order');
