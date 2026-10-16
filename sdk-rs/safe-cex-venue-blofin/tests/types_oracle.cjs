// Execute the maintained module; never substitute enum values or response fixtures.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const filename = path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.types.ts');
const source = fs.readFileSync(filename, 'utf8');
const cache = new Map();
function execute(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {};
  cache.set(file, exports);
  const js = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(js, { exports, require(spec) {
    if (spec.startsWith('@tealstreet/')) return execute(path.join(root, 'packages', spec.slice('@tealstreet/'.length) + '.ts'));
    assert(spec.startsWith('.'), `unexpected dependency ${spec}`);
    return execute(path.resolve(path.dirname(file), `${spec}.ts`));
  } }, { filename: file });
  return exports;
}
const constants = execute(filename);
// Fail closed on heritage and all shapes this slice does not model.
function schema(node) {
  if (ts.isInterfaceDeclaration(node)) {
    assert(!node.heritageClauses?.length, 'inherited_fields_refused');
    return members(node.members);
  }
  if (ts.isTypeLiteralNode(node)) return members(node.members);
  if (ts.isArrayTypeNode(node)) return { array: schema(node.elementType) };
  if (node.kind === ts.SyntaxKind.StringKeyword) return { string: true };
  if (ts.isUnionTypeNode(node)) {
    assert(node.types.every(n => ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal)), 'unsupported_union');
    return { literals: node.types.map(n => n.literal.text) };
  }
  throw new Error(`unsupported_type: ${ts.SyntaxKind[node.kind]}`);
}
function members(nodes) {
  return { fields: nodes.map(n => {
    assert(ts.isPropertySignature(n) && ts.isIdentifier(n.name) && n.type, 'unsupported_member');
    return { name: n.name.text, optional: !!n.questionToken, shape: schema(n.type) };
  }) };
}
const control = ts.createSourceFile('control.ts', 'interface Base { inherited: string }; interface Child extends Base { own: string }', ts.ScriptTarget.Latest, true);
assert.throws(() => schema(control.statements.find(n => n.name?.text === 'Child')), /inherited_fields_refused/, 'heritage_control');
const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
const schemas = {};
for (const n of ast.statements) {
  if (ts.isTypeAliasDeclaration(n)) schemas[n.name.text] = schema(n.type);
  else if (ts.isInterfaceDeclaration(n)) schemas[n.name.text] = schema(n);
}
let discriminator = 0;
function sample(shape, label, minimal, variant) {
  if (shape.string) return `${label}:field-${++discriminator}:雪😀 e\u0301 \t`;
  if (shape.literals) return shape.literals[variant % shape.literals.length];
  if (shape.array) return [sample(shape.array, `${label}[0]`, minimal, variant)];
  return Object.fromEntries(shape.fields.filter(f => !minimal || !f.optional)
    .map(f => [f.name, sample(f.shape, `${label}.${f.name}`, minimal, variant)]));
}
const cases = [];
for (const [name, shape] of Object.entries(schemas)) {
  for (const minimal of [false, true]) for (let variant = 0; variant < 3; variant++) {
    const input = sample(shape, name, minimal, variant);
    // Type aliases erase at runtime: execute a TS typed identity over actual JSON.parse.
    const probe = ts.transpileModule(`export const observe = (text: string): ${name} => JSON.parse(text);`, {
      compilerOptions: { module: ts.ModuleKind.CommonJS },
    }).outputText;
    const exports = {};
    vm.runInNewContext(probe, { exports });
    const value = exports.observe(JSON.stringify(input));
    function inspect(value) {
      if (Array.isArray(value)) return value.map(inspect);
      if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k, inspect(v)]));
      assert.equal(typeof value, 'string', 'native_string_before_serialization');
      return value;
    }
    cases.push({ name, minimal, variant, value: inspect(value) });
  }
}
// Source runtime accepts values its declarations exclude; capture before serialization.
const boundaries = ['{"code":"\\ud800"}', '["0","message",[]]', '{"code":-0}', '{"code":1e400}', '{"code":null}'].map(raw => {
  const parsed = JSON.parse(raw);
  const code = parsed.code;
  const bits = typeof code === 'number' ? (() => { const b = Buffer.alloc(8); b.writeDoubleBE(code); return b.toString('hex'); })() : null;
  return { raw, array: Array.isArray(parsed), type: typeof code, bits, codeUnit: typeof code === 'string' ? code.charCodeAt(0) : null };
});

const probes = [];
for (const [name, shape] of Object.entries(schemas)) {
  const original = cases.find(c => c.name === name && !c.minimal).value;
  function walk(sh, keys) {
    if (sh.array) return walk(sh.array, [...keys, 0]);
    if (sh.fields) {
      for (const field of sh.fields) {
        const p = [...keys, field.name];
        mutate(p, 'missing', undefined, !field.optional);
        mutate(p, 'null', null, true);
        if (field.shape.fields) mutate(p, 'array_record', Object.values(original[field.name]), true);
        if (field.shape.array) mutate([...p, 0], 'array_record', Object.values(original[field.name][0]), true);
        walk(field.shape, p);
      }
    } else {
      if (sh.string) { mutate(keys, 'empty_string', '', false); mutate(keys, 'unicode', '😀 e\u0301 \t', false); }
      mutate(keys, 'number', 71, true);
      mutate(keys, 'surrogate', '\ud800', true);
      if (sh.literals) mutate(keys, 'unknown_literal', 'future-side', true);
    }
  }
  function mutate(keys, reason, value, reject) {
    const copy = JSON.parse(JSON.stringify(original));
    let parent = copy;
    for (const key of keys.slice(0, -1)) parent = parent[key];
    const key = keys.at(-1);
    if (reason === 'missing') delete parent[key]; else parent[key] = value;
    const raw = JSON.stringify(copy);
    const parsed = JSON.parse(raw);
    let native = parsed;
    for (const key of keys) native = native?.[key];
    probes.push({ name, path: keys.join('.'), reason, reject, raw,
      sourceType: typeof native,
      sourceCodeUnit: typeof native === 'string' ? native.charCodeAt(0) : null });
  }
  walk(shape, []);
  const duplicateRaw = '{"code":"first",' + JSON.stringify({...original, code: 'last'}).slice(1);
  assert.equal(JSON.parse(duplicateRaw).code, 'last', 'source_duplicate_last_wins');
  probes.push({ name, path: 'code', reason: 'duplicate_key', reject: false, raw: duplicateRaw, sourceType: 'string' });
  let deep = 'leaf'; for (let i = 0; i < 140; i++) deep = [deep];
  const deepRaw = JSON.stringify({...original, extra: deep});
  assert(Array.isArray(JSON.parse(deepRaw).extra), 'source_deep_extra_accepted');
  probes.push({ name, path: 'extra', reason: 'depth_budget', reject: true, raw: deepRaw, sourceType: 'object' });
  probes.push({ name, path: 'extra', reason: 'extra_field', reject: false, raw: JSON.stringify({...original, extra: 'retained-in-source'}), sourceType: 'string' });
  for (const rawToken of ['-0', '-0.0', '1e400', '-1e400', '-1e-4000', '1.25', '1e3']) {
    const copy = {...original, code: '__numeric_probe__'};
    const raw = JSON.stringify(copy).replace('"__numeric_probe__"', rawToken);
    const native = JSON.parse(raw).code;
    assert.equal(typeof native, 'number', 'numeric_source_type');
    const bits = Buffer.alloc(8); bits.writeDoubleBE(native);
    probes.push({ name, path: 'code', reason: rawToken, reject: true, raw, sourceType: typeof native, bits: bits.toString('hex') });
  }
  probes.push({ name, path: '', reason: 'array_root', reject: true, raw: JSON.stringify(Object.values(original)), sourceType: 'object' });
}
const originalCases = Buffer.from(JSON.stringify(cases));
assert.equal(originalCases.length, 26602, 'BLOFIN-CARDINALITY original corpus bytes');
assert.equal(require('node:crypto').createHash('sha256').update(originalCases).digest('hex'), '4db1755c7ceec59ca0e72f1207c5ac7fe766a586b71013e09ca8c558a5523f8b', 'BLOFIN-CARDINALITY original corpus identity');
// Cardinality supplement: append distinct rows; the original 36 cases stay unchanged.
for (const name of ['FETCH_POSITION_RESPONSE','FETCH_BATCH_LEV_RESPONSE','ORDERS_RESPONSE','POSITIONS_RESPONSE']) {
  const value = sample(schemas[name], name + ':cardinality', false, 0);
  const second = sample(schemas[name], name + ':cardinality:second', false, 1);
  value.data.push(second.data[0]);
  const code = ts.transpileModule(`export const observe = (text: string): ${name} => JSON.parse(text);`, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
  const exports = {}; vm.runInNewContext(code, {exports});
  cases.push({name, minimal:false, variant:0, cardinalitySupplement:true, value:exports.observe(JSON.stringify(value))});
}
process.stdout.write(JSON.stringify({ header: {architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]), nodeVersion: process.version},  constants, schemas, cases, boundaries, probes }));
