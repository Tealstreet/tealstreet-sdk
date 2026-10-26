const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated TypeScript install required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const lodash = require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../lodash/lodash.js'));
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/get-key.ts');
let source = fs.readFileSync(file, 'utf8').replace(/^import .*;\r?\n/gm, '');
if (process.argv.includes('--control-null')) {
  const wrong = source.replace('val !== undefined', 'val != null');
  assert.notEqual(wrong, source, 'source null mutation applies');
  source = wrong;
}
const context = { exports: {}, camelCase: lodash.camelCase, snakeCase: lodash.snakeCase, memoize: lodash.memoize };
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, context);
const rows = [];
const names = Object.getOwnPropertyNames(Object.prototype);
const encode = value => {
  if (value === undefined) return { kind: 'undefined' };
  for (const name of names) if (value === (name === '__proto__' ? Object.prototype : Object.prototype[name])) return { kind: 'inherited', text: String(value) };
  return { kind: 'json', value };
};
function emit(entries, keys) {
  const obj = {};
  for (const [key, value, undefinedValue] of entries) Object.defineProperty(obj, key, { value: undefinedValue ? undefined : value, configurable: true, enumerable: true });
  rows.push({ entries, keys, v: encode(context.exports.v(obj, keys[0])), vn: encode(context.exports.vn(obj, ...keys)) });
}
const keys = ['tradeId', 'trade_id', 'XMLHttpRequest', 'déjàVu', 'foo42Bar', '😀Value', '\ufefffoo', ...names];
const values = [null, false, 0, '', [], {}, 7];
for (const key of keys) {
  emit([], [key, 'fallback']);
  for (const value of values) for (const spelling of [key, lodash.snakeCase(key), lodash.camelCase(key)]) {
    emit([[spelling, value, false], ['fallback', 9, false]], [key, 'fallback']);
    emit([[spelling, null, true], ['fallback', 9, false]], [key, 'fallback']);
  }
  emit([[key, null, true], [lodash.snakeCase(key), 4, false], ['fallback', 9, false]], [key, 'fallback']);
}
assert.equal(context.exports.vn({ first: null, second: 9 }, 'first', 'second'), null, 'present null terminates aliases');
assert.equal(context.exports.v({ tradeId: undefined, trade_id: 9 }, 'tradeId'), undefined, 'own undefined terminates spelling search');
for (const escaped of ['"\\ud800"', '"a\\udfff"']) {
  const key = JSON.parse(escaped);
  const obj = {}; Object.defineProperty(obj, key, { value: 12 });
  assert.equal(context.exports.v(obj, key), 12, 'valid JSON lone-surrogate key accepted by TypeScript');
}
process.stdout.write(JSON.stringify(rows));
