const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/phemex/phemex.types.ts');
const original = fs.readFileSync(filename, 'utf8');
// Type-only probes instantiate the ACTUAL generic alias. A minimal ambient
// library keeps this shape census independent of unrelated monorepo packages.
const source = `${original}\ndeclare global { interface Array<T> { [n: number]: T; length: number; } }
type Record<K extends string, T> = { [P in K]: T };
type ProbeEnvelope = PhemexApiResponse<POSITIONS_RESPONSE>;
interface InheritedRequired { inheritedRequired: number; }
interface HeritageControl extends InheritedRequired { own: string; }
`;
const options = { noLib: true, noResolve: true, target: ts.ScriptTarget.ES2022 };
const host = ts.createCompilerHost(options);
host.getSourceFile = name => name === filename ? ts.createSourceFile(name, source, options.target, true) : undefined;
const program = ts.createProgram([filename], options, host);
const checker = program.getTypeChecker();
const ast = program.getSourceFile(filename);
function named(name) {
  const declarations = ast.statements.filter(s => s.name?.text === name);
  assert.equal(declarations.length, 1, `unique type ${name}`);
  return checker.getTypeAtLocation(declarations[0]);
}
function shape(type) {
  if (type.flags & ts.TypeFlags.NumberLike) return 'number';
  if (type.flags & ts.TypeFlags.StringLike) return 'string';
  if (type.flags & ts.TypeFlags.BooleanLike) return 'boolean';
  const element = checker.getIndexTypeOfType(type, ts.IndexKind.Number);
  if (element) return [shape(element)];
  assert(!(type.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)), 'shape census refuses unresolved type');
  const properties = checker.getPropertiesOfType(type);
  assert(properties.length, 'shape census refuses unhandled empty type');
  return Object.fromEntries(properties.map(p => {
    assert(!(p.flags & ts.SymbolFlags.Optional), `shape census refuses optional ${p.name}`);
    return [p.name, shape(checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration || p.declarations[0]))];
  }));
}
assert.deepEqual(shape(named('HeritageControl')), { own: 'string', inheritedRequired: 'number' }, 'inherited required-field census');
const response = shape(named('POSITIONS_RESPONSE'));
const envelope = shape(named('ProbeEnvelope'));
assert.equal(Object.keys(response.account).length, 7, 'account field census');
assert.equal(Object.keys(response.positions[0]).length, 42, 'position field census');
assert.deepEqual(envelope, { ...response, code: 'number', msg: 'string' }, 'generic intersection census');
let sequence = 0;
function fixture(descriptor) {
  if (descriptor === 'number') return ++sequence * 101 + 0.25;
  if (descriptor === 'string') return `field-${++sequence}`;
  if (descriptor === 'boolean') return true;
  if (Array.isArray(descriptor)) return [fixture(descriptor[0])];
  return Object.fromEntries(Object.entries(descriptor).map(([k, v]) => [k, fixture(v)]));
}
function bits(n) {
  // Only NaN is canonicalized: neither zero sign nor infinities are changed.
  if (Number.isNaN(n)) return '7ff8000000000000';
  const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');
}
function capture(value) { return typeof value === 'number' ? ['number', bits(value)] : [typeof value, value]; }
// Execute a typed projection against the actual source alias. Types erase;
// the compiler census above is the type witness, and this is the JS numeric witness.
const projection = ts.transpileModule(`function observe(value: POSITIONS_RESPONSE) {
 const result: Record<string, unknown> = {};
 for (const [key, v] of Object.entries(value.account)) result['account.' + key] = capture(v);
 for (const [key, v] of Object.entries(value.positions[0])) result['positions[0].' + key] = capture(v);
 return result;
}`, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const observe = vm.runInNewContext(`${projection}\nobserve`, { capture });
const base = fixture(response);
const cases = [];
function add(label, raw) {
  const value = JSON.parse(raw);
  cases.push({ label, raw, expected: observe(value) });
}
add('distinct', JSON.stringify(base));
const numberFields = [
  ...Object.entries(response.account).filter(([, t]) => t === 'number').map(([k]) => ['account', k]),
  ...Object.entries(response.positions[0]).filter(([, t]) => t === 'number').map(([k]) => ['positions', k]),
];
for (const token of ['-0', '0', '9007199254740993', '1e400', '-1e400', '-1e-400', '5e-324']) {
  for (const [container, field] of numberFields) {
    const copy = structuredClone(base);
    (container === 'account' ? copy.account : copy.positions[0])[field] = '__NUMBER_TOKEN__';
    add(`${container}.${field}:${token}`, JSON.stringify(copy).replace('"__NUMBER_TOKEN__"', token));
  }
}
const envelopes = [...cases.map(c => ['67.5', c.raw]), ...['-0', '9007199254740993', '1e400'].map(token => [token, JSON.stringify(base)])].map(([token, payload]) => {
  const raw = `{"code":${token},"msg":"metadata-msg",${payload.slice(1)}`;
  const value = JSON.parse(raw);
  return { raw, code: capture(value.code), msg: value.msg, expected: observe(value) };
});
const surrogateRaw = JSON.stringify(base).replace('field-3', '\\ud800');
assert.equal(JSON.parse(surrogateRaw).account.currency.charCodeAt(0), 0xd800, 'source JSON retains lone surrogate');
const overlapShape = shape(named('ProbeEnvelope'));
assert.equal(overlapShape.code, 'number', 'source intersection has numeric code');
process.stdout.write(JSON.stringify({ surrogateRaw, profile: { node: process.version, major: process.versions.node.split('.')[0], arch: process.arch, platform: process.platform }, response, envelope, cases, envelopes }));
