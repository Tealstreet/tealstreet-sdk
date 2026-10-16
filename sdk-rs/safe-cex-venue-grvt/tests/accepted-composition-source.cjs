'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(path.isAbsolute(process.env.SAFE_CEX_TYPESCRIPT_PATH || ''), 'absolute isolated TypeScript compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const mode = process.argv[2];
assert([undefined, 'id-boundary', 'cache-limit', 'duplicate-validator'].includes(mode), 'unknown source control');
const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const sourceText = fs.readFileSync(sourcePath, 'utf8');
let tree = ts.createSourceFile(sourcePath, sourceText, ts.ScriptTarget.Latest, true);
function owningClass(tree) {
  assert.equal(tree.parseDiagnostics.length, 0, 'maintained source parses');
  const owners = tree.statements.filter(node => ts.isClassDeclaration(node) && node.name?.text === 'GrvtExchange');
  assert.equal(owners.length, 1, 'unique actual GRVT source class');
  return owners[0];
}
let owner = owningClass(tree);
function declaredName(name) {
  assert(ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isPrivateIdentifier(name), 'static maintained member name');
  return ts.isPrivateIdentifier(name) ? name.getText(tree) : name.text;
}
function member(name) {
  const selected = owner.members.filter(node => node.name && declaredName(node.name) === name);
  assert.equal(selected.length, 1, `unique maintained member ${name}`);
  return selected[0];
}
if (mode === 'duplicate-validator') {
  const original = member('clientOrderIdRule').initializer;
  assert(ts.isObjectLiteralExpression(original), 'control rule object');
  const at = original.end - 1;
  assert.equal(sourceText[at], '}', 'control object closing brace');
  // A quoted late override also defeats matching raw identifier text.
  const changed = sourceText.slice(0, at) + "'isValid': () => true," + sourceText.slice(at);
  tree = ts.createSourceFile(sourcePath, changed, ts.ScriptTarget.Latest, true);
  owner = owningClass(tree);
}
const rule = member('clientOrderIdRule').initializer;
assert(ts.isObjectLiteralExpression(rule), 'actual client rule object');
assert(rule.properties.every(node => ts.isPropertyAssignment(node) && !ts.isComputedPropertyName(node.name)), 'plain rule properties without spreads or overrides');
const propertyNames = rule.properties.map(node => declaredName(node.name));
assert.equal(new Set(propertyNames).size, propertyNames.length, 'unique rule properties');
const validators = rule.properties.filter(node => declaredName(node.name) === 'isValid');
assert.equal(validators.length, 1, 'unique actual validator');
const validator = validators[0].initializer;
assert(ts.isArrowFunction(validator), 'actual validation arrow');
let validatorText = validator.getText(tree);
let limitText = member('MAX_NONCE_MAPPINGS').initializer.getText(tree);
if (mode === 'id-boundary') {
  assert.equal(validatorText.split('2n ** 63n').length, 2, 'boundary control source anchor');
  validatorText = validatorText.replace('2n ** 63n', '2n ** 64n');
}
if (mode === 'cache-limit') limitText = `(${limitText}) + 1`;
const source = `const isValid=${validatorText};
class GrvtExchange { static MAX_NONCE_MAPPINGS=${limitText}; ${member('recordNonce').getText(tree)} }
exports.api={isValid,GrvtExchange};`;
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  reportDiagnostics: true,
});
assert.equal(compiled.diagnostics.length, 0, 'actual source transpiles');
const context = { exports: {} };
vm.runInNewContext(compiled.outputText, context, { filename: sourcePath });
const { isValid, GrvtExchange } = context.exports.api;
const contracts = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../safe-cex-parity/accepted-set/contracts/safe-cex-venue-grvt.json')));
const prefix = 'safe_cex_venue_grvt::composition::';
const idCases = contracts[prefix + 'ClientOrderIdRule'].cases;
assert.equal(idCases.length, 29, 'independent validation floor');
for (const row of idCases) assert.deepEqual({ valid: isValid(row.input) }, row.output, `CLIENT-ID actual source ${JSON.stringify(row.input)}`);
function bits(value) {
  if (value === undefined) return null;
  const data = new DataView(new ArrayBuffer(8));
  data.setFloat64(0, value, false);
  return data.getBigUint64(0, false).toString();
}
const entries = contracts[prefix + 'GrvtExchange'].entrypoints;
assert.equal(entries.length, 2, 'complete composition API floor');
const nonceCases = entries.find(entry => entry.api.name === 'record_nonce').cases;
assert.equal(nonceCases.length, 4, 'independent nonce floor');
for (const [index, row] of nonceCases.entries()) {
  const exchange = new GrvtExchange();
  exchange._nonceCreationTimes = new Map();
  exchange._nonceToOrderId = new Map();
  for (const operation of row.input.operations) {
    const time = operation.time === undefined ? undefined : operation.time === 'inf' ? Infinity : Number(operation.time);
    exchange.recordNonce(operation.nonce, operation.real === null ? undefined : operation.real, time);
  }
  for (let i = 0; i < (row.input.fill || 0); ++i) exchange.recordNonce(`n${i}`, `id${i}`, 1000 + i);
  const times = Array.from(exchange._nonceCreationTimes, ([key, value]) => ({ key, bits: bits(value) }));
  const ids = Array.from(exchange._nonceToOrderId);
  const actual = {
    times: times.slice(0, 3), ids: ids.slice(0, 3), time_count: times.length, id_count: ids.length,
    last_time: times.at(-1) ?? null, last_id: ids.at(-1) ?? null,
    retained_first_two: [exchange._nonceCreationTimes.has('n0'), exchange._nonceCreationTimes.has('n1'), exchange._nonceToOrderId.has('n0'), exchange._nonceToOrderId.has('n1')],
  };
  for (const key of Object.keys(actual)) assert.deepEqual(actual[key], row.output[key], `NONCE actual source case${index} ${key}`);
}
console.log('Actual maintained GRVT source: 29 client-ID + 4 nonce expectations match; native registry/alias checks remain separate.');
