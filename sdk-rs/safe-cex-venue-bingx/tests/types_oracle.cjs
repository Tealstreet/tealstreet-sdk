// Source-derived declaration witnesses. Type aliases have no runtime body: use
// the real TS checker for their shape and execute the actual types module too.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const typeFile = path.join(root, 'packages/safe-cex/src/exchanges/bingx/bingx.types.ts');
const apiFile = path.join(root, 'packages/safe-cex/src/exchanges/bingx/bingx.api.ts');
const heritageFile = path.join(root, 'sdk-rs/target/bingx-heritage-control.ts');
const options = { strict: true, noResolve: true, target: ts.ScriptTarget.ES2022 };
const host = ts.createCompilerHost(options);
const originalGet = host.getSourceFile.bind(host);
host.getSourceFile = (file, language, ...args) => file === heritageFile
  ? ts.createSourceFile(file, "interface Parent { inherited: string } interface Child extends Parent { own: number } import type { Missing } from './unresolved'; interface Unresolved extends Missing { own: number } interface Mid extends Missing { mid: string } interface Transitive extends Mid { own: number }", language, true)
  : originalGet(file, language, ...args);
const program = ts.createProgram([typeFile, apiFile, heritageFile], options, host);
assert.equal(program.getSyntacticDiagnostics().length, 0, 'source syntax');
const checker = program.getTypeChecker();
function schema(type, seen = new Set()) {
  if (type.flags & ts.TypeFlags.String) return { kind: 'string' };
  if (type.flags & ts.TypeFlags.Number) return { kind: 'number' };
  if (type.flags & ts.TypeFlags.Boolean) return { kind: 'boolean' };
  if (type.flags & ts.TypeFlags.StringLiteral) return { kind: 'union', values: [type.value] };
  if (type.isUnion()) {
    const members = type.types.filter(t => !(t.flags & ts.TypeFlags.Undefined));
    if (members.length === 1) return schema(members[0], seen);
    if (members.every(t => t.flags & ts.TypeFlags.BooleanLiteral)) return { kind: 'boolean' };
    assert(members.every(t => t.flags & ts.TypeFlags.StringLiteral), `unsupported union ${checker.typeToString(type)}`);
    return { kind: 'union', values: members.map(t => t.value) };
  }
  if (checker.isArrayType(type)) return { kind: 'array', item: schema(checker.getTypeArguments(type)[0], seen) };
  assert(type.flags & ts.TypeFlags.Object, `unsupported type ${checker.typeToString(type)}`);
  assert(!seen.has(type), 'recursive type must be handled explicitly');
  seen = new Set([...seen, type]);
  // noResolve bounds this corpus to the declared local shapes; do not let an
  // unresolved imported base silently turn into an own-members-only census.
  for (const declaration of type.getSymbol()?.declarations ?? []) {
    for (const clause of declaration.heritageClauses ?? []) {
      for (const base of clause.types) {
        const baseType = checker.getTypeAtLocation(base);
        assert(!(baseType.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)), 'unresolved heritage');
        schema(baseType, seen);
      }
    }
  }
  assert.equal(checker.getIndexInfosOfType(type).length, 0, 'index signatures need explicit handling');
  const fields = checker.getPropertiesOfType(type).map(symbol => {
    const declaration = symbol.valueDeclaration ?? symbol.declarations?.[0];
    assert(declaration && ts.isPropertySignature(declaration), `unsupported member ${symbol.name}`);
    return { name: symbol.name, optional: !!(symbol.flags & ts.SymbolFlags.Optional),
      type: schema(checker.getTypeOfSymbolAtLocation(symbol, declaration), seen) };
  });
  assert(fields.length, 'empty/unresolved record must not silently pass');
  return { kind: 'record', fields };
}
const inheritedDeclaration = program.getSourceFile(heritageFile).statements.find(s => s.name?.text === 'Child');
assert.deepEqual(schema(checker.getTypeAtLocation(inheritedDeclaration)).fields.map(f => f.name).sort(),
  ['inherited', 'own'], 'inherited_field_census');
const unresolvedDeclaration = program.getSourceFile(heritageFile).statements.find(s => s.name?.text === 'Unresolved');
assert.throws(() => schema(checker.getTypeAtLocation(unresolvedDeclaration)), /unresolved heritage/, 'unresolved_heritage_refusal');
const transitiveDeclaration = program.getSourceFile(heritageFile).statements.find(s => s.name?.text === 'Transitive');
assert.throws(() => schema(checker.getTypeAtLocation(transitiveDeclaration)), /unresolved heritage/, 'transitive_heritage_refusal');
const declarations = {};
for (const file of [typeFile, apiFile]) {
  for (const declaration of program.getSourceFile(file).statements) {
    if (ts.isTypeAliasDeclaration(declaration) || ts.isInterfaceDeclaration(declaration)) {
      declarations[declaration.name.text] = schema(checker.getTypeAtLocation(declaration));
    }
  }
}
// Execute maintained module and real shared enums with fail-closed imports.
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const module = { exports: {} }; cache.set(file, module.exports);
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    fileName: file, compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const requireSource = name => {
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert(name.startsWith('.'), `unhandled source dependency ${name}`);
    return load(path.resolve(path.dirname(file), `${name}.ts`));
  };
  vm.runInThisContext(`(function(require,module,exports){${code}\n})`, { filename: file })(requireSource, module, module.exports);
  return module.exports;
}
const runtime = load(typeFile);
assert.deepEqual(Object.keys(runtime.ORDER_STATUS).sort(), [...declarations.BINGX_ORDER_STATUS.values].sort(), 'runtime_status_union');
function observe(value) {
  if (typeof value === 'number') {
    const bits = Buffer.alloc(8); bits.writeDoubleBE(Number.isNaN(value) ? NaN : value);
    return { number: Number.isNaN(value) ? '7ff8000000000000' : bits.toString('hex') };
  }
  if (Array.isArray(value)) return value.map(observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,observe(v)]));
  return value;
}
let sequence = 0;
function sample(s, at = '') {
  if (s.kind === 'string') return `string:${at}:\u{1f980}`;
  if (s.kind === 'number') return ++sequence + 0.125;
  if (s.kind === 'boolean') return false;
  if (s.kind === 'union') return s.values[0];
  if (s.kind === 'array') return [sample(s.item, `${at}/0`), sample(s.item, `${at}/1`)];
  return Object.fromEntries(s.fields.map(f => [f.name, sample(f.type, `${at}/${f.name}`)]));
}
function sites(s, at = []) {
  const result = [{ at, schema: s }];
  if (s.kind === 'record') for (const f of s.fields) result.push(...sites(f.type, [...at, f.name]).map(x => ({...x, optional: x.at.length === at.length + 1 ? f.optional : x.optional})));
  if (s.kind === 'array') for (const index of [0, 1]) result.push(...sites(s.item, [...at, index]));
  return result;
}
function change(value, at, replacement, remove = false) {
  if (!at.length) return replacement;
  value = structuredClone(value);
  let parent = value;
  for (const key of at.slice(0, -1)) parent = parent[key];
  if (remove) delete parent[at.at(-1)]; else parent[at.at(-1)] = replacement;
  return value;
}
function rawReplace(value, at, token) {
  if (!at.length) return token;
  if (Array.isArray(value)) return '[' + value.map((v,i) => i === at[0] ? rawReplace(v, at.slice(1), token) : JSON.stringify(v)).join(',') + ']';
  return '{' + Object.entries(value).map(([k,v]) => JSON.stringify(k) + ':' + (k === at[0] ? rawReplace(v, at.slice(1), token) : JSON.stringify(v))).join(',') + '}';
}
const cases = [];
for (const [name, s] of Object.entries(declarations)) {
  const base = sample(s);
  function add(label, raw, valid, boundary = null, at = []) {
    const parsed = JSON.parse(raw);
    // Capture native JS types/bits before JSON serialization can erase overflow/-0.
    let selected = parsed; for (const key of at) selected = selected[key];
    cases.push({ name, label, raw, valid, boundary, wire: JSON.stringify(parsed),
      units: boundary ? Array.from({length: selected.length}, (_,i) => selected.charCodeAt(i)) : null,
      native: valid && !boundary ? observe(parsed) : null });
  }
  add('distinct_fields', JSON.stringify(base), true);
  for (const { at, schema: leaf, optional } of sites(s)) {
    const label = at.join('/');
    add(`null:${label}`, JSON.stringify(change(base, at, null)), false);
    if (at.length && typeof at.at(-1) === 'string') add(`absent:${label}`, JSON.stringify(change(base, at, undefined, true)), !!optional);
    if (leaf.kind === 'number') {
      for (const token of ['-0', '9007199254740993', '1e400', '-1e400', '-1e-999', '5e-324', '1.0000000000000002']) add(`number:${label}:${token}`, rawReplace(base, at, token), true);
      add(`wrong_type:${label}`, JSON.stringify(change(base, at, '5')), false);
    } else if (leaf.kind === 'union') {
      for (const value of leaf.values) add(`literal:${label}:${value}`, JSON.stringify(change(base, at, value)), true);
      for (const value of ['__unknown', { [leaf.values[0]]: null }, [leaf.values[0]]]) add(`wrong_union:${label}:${JSON.stringify(value)}`, JSON.stringify(change(base, at, value)), false);
    } else if (leaf.kind === 'boolean') {
      add(`boolean:${label}`, JSON.stringify(change(base, at, true)), true);
      add(`wrong_type:${label}`, JSON.stringify(change(base, at, 'true')), false);
    } else if (leaf.kind === 'record') {
      let item = base; for (const key of at) item = item[key];
      add(`positional_array:${label}`, JSON.stringify(change(base, at, Object.values(item))), false);
    } else if (leaf.kind === 'array') {
      add(`wrong_type:${label}`, JSON.stringify(change(base, at, {})), false);
    } else {
      add(`wrong_type:${label}`, JSON.stringify(change(base, at, 5)), false);
      add(`unicode:${label}`, JSON.stringify(change(base, at, 'e\u0301 é \ufeff \u0085 🦀')), true);
      for (const token of ['"\\ud800"', '"\\udfff"', '"a\\ud800b"']) {
        add(`utf8_boundary:${label}:${token}`, rawReplace(base, at, token), true, 'utf8', at);
      }
    }
  }
}
process.stdout.write(JSON.stringify({ runtime: { node: process.version, arch: process.arch, platform: process.platform }, names: Object.keys(declarations), cases, heritage: true }));
