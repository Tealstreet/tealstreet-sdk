// Live, source-first corpus; stdout only. No historical artifact is rewritten.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const bits = require('./number-bits.cjs');
const root = path.resolve(__dirname, '../../..');
function load(name, imports = {}, extra = '') {
  const exports = {};
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/woo', name), 'utf8');
  vm.runInNewContext(ts.transpileModule(source + extra, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText, { exports, require: name => { assert.ok(Object.hasOwn(imports, name), name); return imports[name]; } });
  return exports;
}
const contracts = load('woo.v3.contracts.ts');
const api = load('woo.v3.cancel-all-after.ts', { './woo.v3.contracts': contracts },
  '\nexport { requireRecord, requireExactKeys, requireNonnegativeSafeInteger };');
const cases = [];
function capture(operation, raw) {
  const input = raw === null ? undefined : JSON.parse(raw);
  let observation;
  try {
    const value = api[operation](input, 'probe');
    if (operation === 'buildWooV3CancelAllAfterRequest') {
      assert.equal(Object.isFrozen(value), true, 'source request remains frozen');
      assert.equal(Object.isFrozen(value.body), true, 'source request body remains frozen');
      observation = { ok: { method: value.method, path: value.path, triggerAfter: bits(value.body.triggerAfter) } };
    } else if (operation === 'decodeWooV3CancelAllAfterResponse') {
      assert.equal(Object.isFrozen(value), true, 'source response remains frozen');
      observation = { ok: { accepted: value.accepted, timestamp: bits(value.timestamp), expectedTriggerTime: bits(value.expectedTriggerTime) } };
    } else if (operation === 'requireNonnegativeSafeInteger') observation = { ok: bits(value) };
    else observation = { ok: value };
  } catch (error) { observation = { error: { name: error.name, message: error.message } }; }
  cases.push({ operation, raw, ...observation });
}
const inputs = [null, 'null', 'false', 'true', '"0"', '"x"', '[]', '{}', '0', '-0', '-0.0', '-1e-400',
  '1e-400', '-1', '0.5', '1', '899999', '900000', '900001', '900000.1', '9007199254740991',
  '9007199254740992', '1e400', '-1e400'];
for (const raw of inputs) {
  capture('buildWooV3CancelAllAfterRequest', raw);
  capture('requireRecord', raw);
  capture('requireNonnegativeSafeInteger', raw);
  capture('decodeWooV3CancelAllAfterResponse', raw);
  for (const key of ['success', 'timestamp', 'data', 'expectedTriggerTime']) {
    const success = key === 'success' ? raw : 'true';
    const timestamp = key === 'timestamp' ? raw : '101';
    const expected = key === 'expectedTriggerTime' ? raw : '202';
    const data = key === 'data' ? raw : `{${expected === null ? '' : '"expectedTriggerTime":' + expected}}`;
    const fields = [success === null ? '' : '"success":' + success,
      timestamp === null ? '' : '"timestamp":' + timestamp, data === null ? '' : '"data":' + data].filter(Boolean);
    capture('decodeWooV3CancelAllAfterResponse', '{' + fields.join(',') + '}');
  }
}
for (const raw of [
  '{"success":false,"timestamp":null,"data":null,"extra":1}',
  '{"success":false,"timestamp":null,"data":null}',
  '{"success":true,"timestamp":null,"data":null}',
  '{"success":true,"timestamp":null,"data":{"expectedTriggerTime":-1,"extra":1}}',
  '{"success":true,"timestamp":null,"data":{"expectedTriggerTime":-1}}',
  '{"success":true,"timestamp":101,"data":{"expectedTriggerTime":202,"__proto__":{}}}',
  '{"success":true,"timestamp":-0,"data":{"expectedTriggerTime":-1e-400}}',
]) capture('decodeWooV3CancelAllAfterResponse', raw);
const exactKeys = [];
for (const input of [{ a: 1, b: 2 }, { a: 1 }, { a: 1, b: 2, c: 3 }, { b: 1, a: 2 }]) {
  let error = null;
  try { api.requireExactKeys(input, ['b', 'a'], 'probe'); } catch (e) { error = e.message; }
  exactKeys.push({ input, expected: ['b', 'a'], error });
}
const unicodeKeys = ['\ue000', '\u{10000}', '\ufeff', 'a'];
let unicodeError;
try { api.requireExactKeys({}, unicodeKeys, 'probe'); } catch (error) { unicodeError = error.message; }
exactKeys.push({ input: {}, expected: unicodeKeys, error: unicodeError, unicode: true });
const surrogatePath = JSON.parse('"\\ud800"');
const firstCodeUnits = [];
for (const operation of ['requireRecord', 'requireExactKeys', 'requireNonnegativeSafeInteger']) {
  let message;
  try {
    if (operation === 'requireExactKeys') api[operation]({ extra: 1 }, [], surrogatePath);
    else api[operation](null, surrogatePath);
  } catch (error) { message = error.message; }
  assert.equal(message.charCodeAt(0), 0xd800, 'source diagnostic retains lone surrogate');
  firstCodeUnits.push(message.charCodeAt(0));
}
const representation = { input: JSON.stringify(surrogatePath), firstCodeUnits };
const paths = [];
for (const diagnosticPath of ['ordinary', 'astral 😀', '\ufeffspace']) {
  let message;
  try { api.requireRecord(null, diagnosticPath); } catch (error) { message = error.message; }
  paths.push({ path: diagnosticPath, message });
}
process.stdout.write(JSON.stringify({ paths, representation, capture: { platform: process.platform, architecture: process.arch, node: process.version },
  limits: api.WOO_V3_CANCEL_ALL_AFTER_LIMITS, parity: api.WOO_V3_CANCEL_ALL_AFTER_PARITY, cases, exactKeys }));
