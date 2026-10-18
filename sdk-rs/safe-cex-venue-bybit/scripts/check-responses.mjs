import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import vm from 'node:vm';
import ts from 'typescript';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const exports = {};
vm.runInNewContext(ts.transpileModule(readFileSync(root + 'packages/safe-cex/src/exchanges/bybit/bybit.response.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports });
const values = [null, false, true, [], {}, '', ' ', 'OK', ' OK ', '\u0085OK\u0085', '\ufeffOK\ufeff', 0, -1, 10003, '0', '-0', '1e2', '.5', '+.5', '1.', '1e309', 'NaN', 'Infinity', 'inf', '0x0', '0xff', '0b10', '0o10', '-0x1', '0x', '0x1p2', '١', '\ufeff0\ufeff'];
// Long radix strings expose intermediate-rounding errors in naive digit accumulation.
let seed = 913;
for (let length = 1; length <= 300; length++) {
  let digits = '';
  for (let i = 0; i < length; i++) { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; digits += (seed >>> 28).toString(16); }
  values.push('0x' + digits);
}
const cases = [null, false, '', [], {}];
for (const key of ['retCode', 'ret_code', 'code', 'retMsg', 'ret_msg', 'message', 'msg']) {
  for (const value of values) {
    const envelope = { [key]: value };
    cases.push(envelope, { raw_response: envelope }, { rawResponse: JSON.stringify(envelope) });
  }
}
for (const first of values.slice(0, 34)) for (const second of values.slice(0, 34)) cases.push({ retCode: first, ret_code: second, code: 10003, retMsg: first, message: second, rawResponse: { code: 0, msg: 'OK' } });
for (const raw of values) cases.push({ raw_response: raw, rawResponse: { code: 0 } });
const normalCaseCount = cases.length;
// JS accepts lone UTF-16 surrogate escapes in JSON; Rust String cannot hold them.
cases.push({raw_response: '{"retMsg":"\\ud800"}'});
assert.equal(cases.length,8488,'RESPONSES-CARDINALITY source total');
const run = spawnSync(process.env.CARGO ?? 'cargo', ['run', '--jobs', '1', '--quiet', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-bybit', '--example', 'responses'], { cwd: root, encoding: 'utf8', input: JSON.stringify(cases), maxBuffer: 16 * 1024 * 1024 });
assert.equal(run.status, 0, run.error?.message ?? run.stderr);
const actual = JSON.parse(run.stdout);
assert.equal(actual.length,cases.length,'RESPONSES-CARDINALITY expected/native outputs');
for (let i = 0; i < normalCaseCount; i++) {
  const expected = JSON.parse(JSON.stringify(exports.getBybitResponseMeta(cases[i])));
  assert.equal(Object.is(actual[i].retCode, -0), Object.is(exports.getBybitResponseMeta(cases[i]).retCode, -0), `signed zero ${i}`);
  assert.deepEqual(JSON.parse(JSON.stringify(actual[i])), expected, `response ${i}: ${JSON.stringify(cases[i])}`);
}
const divergent = exports.getBybitResponseMeta(cases[normalCaseCount]);
assert.equal(divergent.retMsg.charCodeAt(0), 0xd800, 'source still accepts lone surrogate');
assert.deepEqual(actual[normalCaseCount], {refusal:{name:'TypeError',message:'Bybit ret_msg cannot be represented as a Rust string'}}, 'visible typed refusal for unrepresentable message');
assert.notDeepEqual(actual[normalCaseCount], JSON.parse(JSON.stringify(divergent)), 'declared raw JSON surrogate divergence');
console.log(`Bybit response equivalence: ${cases.length} live TypeScript cases.`);
