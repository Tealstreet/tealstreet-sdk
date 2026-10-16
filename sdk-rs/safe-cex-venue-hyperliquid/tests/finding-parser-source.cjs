'use strict';
// Source-only review observation: 13 cases, ending at literal ASCII returned action type.
// Run from the repository root, or set SAFE_CEX_ROOT. This is not a native parity gate.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = process.env.SAFE_CEX_ROOT || path.resolve(__dirname, "../../..");
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', { paths: [root] }));
const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
const parse = relative => ts.createSourceFile(relative, read(relative), ts.ScriptTarget.Latest, true);
const api = parse('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.api.ts');
const rate = parse('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.rateLimiter.ts');
const order = parse('packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-response.ts');
function initializer(sf, name) {
  const declaration = sf.statements.filter(ts.isVariableStatement)
    .flatMap(statement => [...statement.declarationList.declarations])
    .find(candidate => candidate.name.getText(sf) === name);
  assert(declaration, `actual source initializer ${name}`);
  return declaration.initializer.getText(sf);
}
function klass(sf, name) {
  const declaration = sf.statements.find(candidate => ts.isClassDeclaration(candidate) && candidate.name.text === name);
  assert(declaration, `actual source class ${name}`);
  return declaration.getText(sf);
}
const context = { exports: {} };
const source = `
exports.action = (${initializer(api, 'getRequestActionType')});
exports.parse = (${initializer(rate, 'parseRequestData')});
const REST_WEIGHT_LIMIT_PER_MINUTE = ${initializer(rate, 'REST_WEIGHT_LIMIT_PER_MINUTE')};
const REST_WEIGHT_WINDOW_MS = ${initializer(rate, 'REST_WEIGHT_WINDOW_MS')};
${klass(rate, 'RateLimiter')}; exports.Rate = RateLimiter;
${klass(order, 'HyperliquidUnreadableOrderResponseError')};
exports.Unreadable = HyperliquidUnreadableOrderResponseError;
`;
vm.runInNewContext(ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText, context);
const actual = context.exports;
let completed = 0;
for (const [suffix, firstUnit] of [['"\\ud800"', 0xd800], ['"\\ud83d\\ude00"', 0xd83d]]) {
  assert.equal(actual.action('{"action":{"type":"cancel"},"unused":' + suffix + '}'), 'cancel');
  completed++;
  const returned = actual.parse('{"type":"userRole","unused":' + suffix + '}');
  assert.equal(returned.type, 'userRole');
  assert.equal(returned.unused.charCodeAt(0), firstUnit);
  completed++;
}
const deep = '['.repeat(140) + '0' + ']'.repeat(140);
assert.equal(actual.action('{"action":{"type":"cancel"},"unused":' + deep + '}'), 'cancel');
completed++;
let returned = actual.parse('{"type":"userRole","unused":' + deep + '}').unused;
for (let index = 0; index < 140; index++) returned = returned[0];
assert.equal(returned, 0);
completed++;
for (const args of [[], [undefined, undefined], [undefined, 0], [0, undefined]]) {
  const limiter = new actual.Rate(...args);
  assert.equal(limiter.nextAvailableAt, 0);
  const expected = args.length === 0 || args.every(value => value === undefined) ? 50 : args[0] === 0 ? Infinity : 0;
  assert.equal(limiter.msPerToken, expected);
  completed++;
}
const error = new actual.Unreadable('independent arbitrary text');
assert.equal(error.message, 'independent arbitrary text');
assert.equal(error.name, 'HyperliquidUnreadableOrderResponseError');
completed++;
const materialized = actual.action('{"action":{"type":"\\ud800"}}');
assert.equal(typeof materialized,'string','ACTION returned source string');assert.equal(materialized.length,1,'ACTION exact returned source unit count');assert.equal(materialized.charCodeAt(0),0xd800,'ACTION materialized returned D800');completed++;
assert.equal(actual.action('{"action":{"type":"\\\\ud800"}}'),'\\ud800','ACTION literal ASCII returned backslash sequence');completed++;
assert.equal(completed, 13, 'independent completed source floor');
process.stdout.write(JSON.stringify({
  source_cases: completed, stop: 'literal ASCII returned action type',
  action_ignored_lone: 'cancel', action_returned_lone_unit: 'D800', rate_returned_lone_unit: 'D800', depth: 140, default_msPerToken: 50,
}) + '\n');
