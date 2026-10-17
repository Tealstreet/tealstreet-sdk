const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const sourceFile = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts');
const source = fs.readFileSync(sourceFile, 'utf8');
require('./interface-heritage.cjs')(ts, ts.createSourceFile(sourceFile, source, ts.ScriptTarget.Latest, true));
const original = fs.readFileSync(path.join(__dirname, '../tests/fixtures/types.json'));
const originalHash = crypto.createHash('sha256').update(original).digest('hex');
// The seed repairs only the excluded WsUserEvent's fabricated user-only row.
// Pin its current bytes and prove every other byte still matches the old feeder.
assert.equal(originalHash, '9f24077387244a38985a94c331d24553baf33c24058a3e8b520d3ecec1165585', 'reviewed shared feeder must remain byte-identical');
const userEventRaw = JSON.parse(original).cases.find(row => row.name === 'WsUserEvent').raw;
const legacy = original.toString().replace(JSON.stringify(userEventRaw), JSON.stringify('{"user":"wallet"}'));
assert.equal(crypto.createHash('sha256').update(legacy).digest('hex'), '4faf350885222f47b8831759d0f5a8a348e0afe30e2731419f424d7f8a70dbdf', 'only the separately covered intersection fixture changed');
const bases = JSON.parse(original).cases;
const bindings = fs.readFileSync(path.join(__dirname, '../tests/fixtures/field-identity-bindings.rs'), 'utf8');
const supported = new Set([...bindings.matchAll(/"([^"]+)"\s*=>/g)].map(match => match[1]));
const virtualFile = path.join(root, 'sdk-rs/safe-cex-venue-hyperliquid/tests/collection-identity-probes.ts');
const options = {strict: true, noEmit: true, skipLibCheck: true, types: [], target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS};
function programFor(cases) {
  const text = 'import type * as Source from "../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types";\n' + cases.map((entry, index) => `const case${index}: Source.${entry.name} = ${entry.raw};`).join('\n');
  const host = ts.createCompilerHost(options);
  const read = host.readFile.bind(host), exists = host.fileExists.bind(host);
  host.readFile = file => file === virtualFile ? text : read(file);
  host.fileExists = file => file === virtualFile || exists(file);
  const program = ts.createProgram([virtualFile], options, host);
  const declarations = program.getSourceFile(virtualFile).statements.filter(ts.isVariableStatement);
  const errors = ts.getPreEmitDiagnostics(program);
  assert(!errors.some(error => error.file?.fileName !== virtualFile), 'source declarations and imports must type-check');
  return {program, declarations, errors};
}
const initial = programFor(bases.filter(base => base.name !== 'WsUserEvent'));
assert.equal(initial.errors.length, 0, 'source feeder inputs must satisfy their declared types');
const checker = initial.program.getTypeChecker();
const typedBases = bases.filter(base => base.name !== 'WsUserEvent');
function typeAt(steps, location) {
  let expression = location.declarationList.declarations[0].initializer;
  for (const key of steps) {
    if (ts.isArrayLiteralExpression(expression)) {
      expression = expression.elements[Number(key)];
    } else {
      assert(ts.isObjectLiteralExpression(expression), 'unsupported source initializer shape');
      const property = expression.properties.find(property => ts.isPropertyAssignment(property) && property.name.text === String(key));
      assert(property, `unsupported source collection path ${steps.join('/')}`);
      expression = property.initializer;
    }
    assert(expression, 'source collection initializer must resolve');
  }
  const type = checker.getContextualType(expression);
  assert(type, 'source collection contextual type must resolve');
  return checker.getNonNullableType(type);
}
function arrays(value, steps = [], result = []) {
  if (Array.isArray(value)) result.push(steps);
  if (value && typeof value === 'object') for (const [key, child] of Object.entries(value)) arrays(child, [...steps, key], result);
  return result;
}
function at(value, steps) { for (const key of steps) value = value[key]; return value; }
function element(value, index, counts = {number: 0, boolean: 0, string: 0}) {
  if (typeof value === 'number') return 101 + index * 1000 + counts.number++;
  if (typeof value === 'boolean') return (index + counts.boolean++) % 2 === 0;
  if (typeof value === 'string' && value.startsWith('wire-')) return `collection:${index}:${counts.string++}:雪`;
  if (Array.isArray(value)) return value.map(child => element(child, index, counts));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, element(child, index, counts)]));
  return value;
}
const catalog = [], candidates = [];
for (let baseIndex = 0; baseIndex < bases.length; baseIndex++) {
  const base = bases[baseIndex], value = JSON.parse(base.raw);
  const location = initial.declarations[typedBases.indexOf(base)];
  for (const steps of arrays(value)) {
    const entry = {name: base.name, path: '/' + steps.join('/'), template: baseIndex, initialLength: at(value, steps).length};
    if (base.name === 'WsUserEvent') {
      catalog.push({...entry, kind: 'excluded', reason: 'existing Value-intersection finding'});
      continue;
    }
    assert(supported.has(base.name), `unresolved native collection root ${base.name}`);
    const type = typeAt(steps, location);
    if (checker.isTupleType(type)) {
      const arity = checker.getTypeArguments(type).length;
      assert.equal(entry.initialLength, arity, 'tuple sample must retain every declared slot');
      catalog.push({...entry, kind: 'tuple', arity, reason: 'fixed arity; variable-length expansion excluded'});
      continue;
    }
    assert(checker.isArrayType(type), `unsupported collection type ${checker.typeToString(type)} at ${base.name}${entry.path}`);
    assert(entry.initialLength > 0, 'collection supplement needs an actual element template');
    catalog.push({...entry, kind: 'array'});
    for (const length of [2, 3]) {
      const changed = JSON.parse(base.raw);
      const values = Array.from({length}, (_, index) => element(at(value, steps)[0], index));
      const distinct = new Set(values.map(item => JSON.stringify(item))).size === length;
      if (steps.length) at(changed, steps.slice(0, -1))[steps.at(-1)] = values;
      else assert(Array.isArray(changed), 'root collection must be an array');
      const input = steps.length ? changed : values;
      candidates.push({...entry, length, distinct, identityScope: distinct ? 'element-order-and-cardinality' : 'cardinality-only-identical-elements', raw: JSON.stringify(input)});
    }
  }
}
const checked = programFor(candidates);
const cases = [], rejected = [];
for (let index = 0; index < candidates.length; index++) {
  const declaration = checked.declarations[index];
  const errors = checked.errors.filter(error => error.start >= declaration.pos && error.start < declaration.end);
  if (errors.length) rejected.push({...candidates[index], errors: errors.map(error => error.code)});
  else cases.push(candidates[index]);
}
assert(cases.length > 0, 'collection supplement must exercise native outputs');
const catalogHash = crypto.createHash('sha256').update(JSON.stringify(catalog)).digest('hex');
process.stdout.write(JSON.stringify({scope: 'self-authored collection field-identity supplement; no retroactive independence claim', captureArchitecture: process.arch, nodeVersion: process.version, nodeMajor: Number(process.versions.node.split('.')[0]), originalFeederSha256: originalHash, catalogHash, excludedRoots: [{name: 'WsUserEvent', reason: 'existing Value-intersection finding', discoveredPaths: arrays(JSON.parse(bases.find(base => base.name === 'WsUserEvent').raw)).length}], catalog, rejected, cases}));
