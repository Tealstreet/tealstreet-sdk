const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { root, ts, dependencies } = require('../../safe-cex-types/scripts/accounts-source.cjs');

function evaluate(name, bindings = {}, expose = '') {
  const file = path.join(root, 'packages/safe-cex/src/utils', name);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const source = ast.statements.filter(node => !ts.isImportDeclaration(node))
    .map(node => node.getText(ast)).join('\n');
  const code = ts.transpileModule(source + expose, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const exports = {};
  new Function('exports', ...Object.keys(bindings), code)(exports, ...Object.values(bindings));
  return exports;
}
const rest = evaluate('venueRestDiagnostics.ts', {},
  '\nexports.private = {stampSigned,T0,SIGN_MS,SIGNED_AT,SIG_AGE_MS,PROXY_HEADER};');
const { stampSigned, T0, SIGN_MS, SIGNED_AT, SIG_AGE_MS: AGE, PROXY_HEADER } = rest.private;
let cases = 0;
for (const value of [1, 0, -0, NaN]) {
  const config = Object.seal({ [T0]: value });
  assert.equal(rest.markRestRequestStart(config, 2), config);
  assert.equal(config[T0], 2, 'SOURCE SEALED existing request stamp');
  cases++;
}
const absent = Object.seal({});
assert.equal(rest.markRestRequestStart(absent, 2), absent);
assert.equal(Object.hasOwn(absent, T0), false);
cases++;
for (const value of ['empty', '']) {
  const config = Object.seal({ [PROXY_HEADER]: value });
  assert.equal(rest.markRestProxyHeader(config, 'present'), config);
  assert.equal(config[PROXY_HEADER], 'present', 'SOURCE SEALED existing proxy stamp');
  cases++;
}
assert.equal(rest.markRestProxyHeader(absent, 'present'), absent);
assert.equal(Object.hasOwn(absent, PROXY_HEADER), false);
cases++;
function configWith(initial, timestamp = '900', failure) {
  let calls = 0;
  const headers = { get(name) {
    assert.equal(this, headers, 'SOURCE SEALED callback receiver');
    assert.equal(name, 'x-bapi-timestamp');
    calls++;
    if (failure) throw failure;
    return timestamp;
  } };
  return { config: Object.seal({ ...initial, headers }), calls: () => calls };
}
for (const initial of [
  { [SIGN_MS]: 1, [SIGNED_AT]: 2, [AGE]: 3 },
  { [SIGN_MS]: 0, [SIGNED_AT]: -0, [AGE]: NaN },
]) {
  const { config, calls } = configWith(initial);
  assert.equal(stampSigned(config, 9, 1100), config);
  assert.equal(config[SIGN_MS], 9);
  assert.equal(config[SIGNED_AT], 1100);
  assert.equal(config[AGE], 200);
  assert.equal(calls(), 1, 'SOURCE SEALED bound callback invocation');
  cases++;
}
for (const [initial, expected, reads] of [
  [{}, {}, 0],
  [{ [SIGN_MS]: 1 }, { [SIGN_MS]: 9 }, 0],
  [{ [SIGN_MS]: 1, [SIGNED_AT]: 2 }, { [SIGN_MS]: 9, [SIGNED_AT]: 1100 }, 1],
]) {
  const { config, calls } = configWith(initial);
  assert.throws(() => stampSigned(config, 9, 1100), TypeError);
  const actual = Object.fromEntries(Object.entries(config).filter(([key]) => key !== 'headers'));
  assert.deepEqual(actual, expected, 'SOURCE SEALED rejected assignment retains ordered prefix');
  assert.equal(calls(), reads);
  cases++;
}
const failure = new Error('header failed');
const failed = configWith({ [SIGN_MS]: 1, [SIGNED_AT]: 2, [AGE]: 17 }, '900', failure);
assert.throws(() => stampSigned(failed.config, 9, 1100), error => error === failure);
assert.equal(failed.config[SIGN_MS], 9);
assert.equal(failed.config[SIGNED_AT], 1100);
assert.equal(failed.config[AGE], 17);
assert.equal(failed.calls(), 1);
cases++;
for (const initial of [
  { [SIGN_MS]: 1, [SIGNED_AT]: 2 },
  { [SIGN_MS]: 1, [SIGNED_AT]: 2, [AGE]: 17 },
]) {
  const { config, calls } = configWith(initial, 'invalid');
  assert.equal(stampSigned(config, 9, 1100), config);
  assert.equal(config[AGE], initial[AGE]);
  assert.equal(config[SIGN_MS], 9);
  assert.equal(config[SIGNED_AT], 1100);
  assert.equal(calls(), 1);
  cases++;
}
const partial = configWith({ [SIGN_MS]: 1, [SIGNED_AT]: 2 });
assert.equal(rest.markRestSigned(partial.config, 9, 1100), partial.config);
assert.equal(partial.config[SIGN_MS], 9);
assert.equal(partial.config[SIGNED_AT], 1100);
assert.equal(Object.hasOwn(partial.config, AGE), false);
assert.equal(partial.calls(), 1);
cases++;
const merge = evaluate('mergeUpdatesUsingLastUpdate.ts', {
  uniqBy: dependencies('lodash/uniqBy'), virtualClock: { getCurrentTimeMs: () => 2000 },
}).mergeUpdatesUsingLastUpdate;
for (const second of [0, undefined]) {
  let reads = 0;
  const update = { id: 'X' };
  const existing = { id: 'X', get lastUpdate() { return ++reads === 1 ? 1500 : second; } };
  assert.equal(merge([update], [existing])[0], second === 0 ? update : existing,
    'SOURCE MERGE second read determines selected identity');
  assert.equal(reads, 2);
  cases++;
}
const trace = [];
let aReads = 0;
let bLast = 1500;
function row(id, label) {
  return {
    get id() {
      trace.push('id:' + label);
      if (label === 'E_A' && ++aReads === 3) bLast = 0;
      return id;
    },
    get lastUpdate() { trace.push('last:' + label); return label === 'E_B' ? bLast : 1500; },
  };
}
const ua = row('A', 'U_A'), ub = row('B', 'U_B'), ea = row('A', 'E_A'), eb = row('B', 'E_B');
const merged = merge([ua, ub], [ea, eb]);
assert.equal(merged[1], eb, 'SOURCE MERGE maps before dedupe mutation');
assert.equal(merged[0], ea);
assert.equal(aReads, 4);
assert.deepEqual(trace, [
  'id:E_A', 'id:U_A', 'last:E_A', 'last:E_A', 'id:E_A', 'id:U_B', 'id:E_B', 'id:U_B',
  'last:E_B', 'last:E_B', 'id:E_A', 'id:E_B', 'id:E_A', 'id:E_B',
]);
cases++;
assert.equal(cases, 20, 'SOURCE finite independent twenty-case floor');
console.log('DIAGNOSTICS CHOICES: 20 maintained-source cases verified');
