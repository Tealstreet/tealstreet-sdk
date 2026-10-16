// One compiler/runtime process for the complete type and numeric corpus.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/orderly/orderly.types.ts');
const inheritedPath = path.join(path.dirname(sourcePath), '__in_memory_heritage_control.ts');
const host = ts.createCompilerHost({ strict: true });
const originalGetSourceFile = host.getSourceFile.bind(host);
host.getSourceFile = (name, ...args) => name === inheritedPath
  ? ts.createSourceFile(name, 'interface Parent { requiredBase: number } interface Child extends Parent { optionalChild?: string }', ts.ScriptTarget.Latest, true)
  : originalGetSourceFile(name, ...args);
const program = ts.createProgram([sourcePath, inheritedPath], { strict: true, noResolve: true }, host);
const checker = program.getTypeChecker();
function properties(type) { return checker.getPropertiesOfType(type); }
const child = program.getSourceFile(inheritedPath).statements[1];
assert.deepEqual(properties(checker.getTypeAtLocation(child)).map(p => [p.name, !!(p.flags & ts.SymbolFlags.Optional)]).sort(),
  [['optionalChild', true], ['requiredBase', false]], 'inherited required-field census');
const declarations = program.getSourceFile(sourcePath).statements.filter(ts.isTypeAliasDeclaration);
const names = ['ACCOUNT_RESPONSE', 'BALANCE_RESPONSE', 'PLACE_ORDER_RESPONSE', 'EDIT_ORDER_REQUEST', 'EDIT_ORDER_RESPONSE'];
assert.deepEqual(declarations.map(d => d.name.text), names, 'wire declaration census');
function shape(type) {
  if (type.isUnion()) {
    const members = type.types.filter(t => !(t.flags & ts.TypeFlags.Undefined));
    if (members.length === 1) return shape(members[0]);
    if (members.every(t => t.flags & ts.TypeFlags.StringLiteral)) return { kind: 'literal', values: members.map(t => t.value) };
    // Boolean is represented as the true | false union by the compiler.
    if (members.every(t => t.flags & ts.TypeFlags.BooleanLiteral)) return { kind: 'boolean' };
    throw new Error('unsupported source union: ' + checker.typeToString(type));
  }
  if (type.flags & ts.TypeFlags.StringLiteral) return { kind: 'literal', values: [type.value] };
  if (type.flags & ts.TypeFlags.String) return { kind: 'string' };
  if (type.flags & ts.TypeFlags.Number) return { kind: 'number' };
  if (type.flags & (ts.TypeFlags.Boolean | ts.TypeFlags.BooleanLiteral)) return { kind: 'boolean' };
  if (checker.isArrayType(type)) return { kind: 'array', item: shape(checker.getTypeArguments(type)[0]) };
  assert.ok(type.flags & ts.TypeFlags.Object, 'unsupported source type: ' + checker.typeToString(type));
  const fields = properties(type).map(p => ({ name: p.name, optional: !!(p.flags & ts.SymbolFlags.Optional),
    shape: shape(checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration || p.declarations[0])) }));
  assert.ok(fields.length, 'unresolved or empty source object: ' + checker.typeToString(type));
  return { kind: 'object', fields };
}
function observe(value, prefix = '', result = {}) {
  if (typeof value === 'number') {
    const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value);
    result[prefix] = ['number', Number.isNaN(value) ? '7ff8000000000000' : buffer.toString('hex')];
  } else if (Array.isArray(value)) {
    result[prefix] = ['array', value.length];
    value.forEach((v, i) => observe(v, `${prefix}/${i}`, result));
  } else if (value && typeof value === 'object') {
    result[prefix] = ['object'];
    Object.entries(value).forEach(([k, v]) => observe(v, `${prefix}/${k}`, result));
  } else result[prefix] = [typeof value, value];
  return result;
}
// Execute a TypeScript-compiled typed identity against each real alias. Declarations
// are erased at runtime; the compiler-derived shape above is the structural witness.
const tsText = fs.readFileSync(sourcePath, 'utf8');
const witnessText = tsText + '\n' + names.map(name => `export function witness_${name}(raw: string): ${name} { return JSON.parse(raw) as ${name}; }`).join('\n');
const output = ts.transpileModule(witnessText, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const sharedText = fs.readFileSync(path.resolve(path.dirname(sourcePath), '../../types.ts'), 'utf8');
// Evaluate the actual shared enum declarations used by this source, not stand-ins.
const sharedAst = ts.createSourceFile('types.ts', sharedText, ts.ScriptTarget.Latest, true);
const enums = sharedAst.statements.filter(s => ts.isEnumDeclaration(s) && ['OrderSide', 'OrderType'].includes(s.name.text)).map(s => s.getText(sharedAst)).join('\n');
const shared = { exports: {} };
new Function('exports', ts.transpileModule(enums, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText)(shared.exports);
const runtime = { exports: {} };
new Function('require', 'exports', output)(name => { assert.equal(name, '../../types'); return shared.exports; }, runtime.exports);
const corpus = [];
const refusals = [];
const projections = [];
const literals = [];
for (const declaration of declarations) {
  const name = declaration.name.text;
  const schema = shape(checker.getTypeAtLocation(declaration));
  let count = 0;
  const numericPaths = [];
  const literalFields = new Map();
  function make(node, prefix, optional, specialPath, token) {
    count++;
    switch (node.kind) {
      case 'number': numericPaths.push(prefix); return prefix === specialPath ? token : String(count + 100.25);
      case 'string': return JSON.stringify(prefix + ':distinct');
      case 'literal': literalFields.set(prefix, node.values); return prefix === specialPath ? token : JSON.stringify(node.values[(optional === 'alternate' ? 1 : 0) % node.values.length]);
      case 'boolean': return optional === 'alternate' ? 'false' : 'true';
      case 'array': return '[' + [0, 1].map(i => make(node.item, `${prefix}/${i}`, optional, specialPath, token)).join(',') + ']';
      case 'object': return '{' + node.fields.filter(f => optional !== 'omit' || !f.optional).map(f => JSON.stringify(f.name) + ':' + make(f.shape, `${prefix}/${f.name}`, optional, specialPath, token)).join(',') + '}';
      default: throw Error('unknown shape');
    }
  }
  function add(mode, specialPath, token) {
    count = 0;
    const raw = make(schema, '', mode, specialPath, token);
    const value = runtime.exports[`witness_${name}`](raw);
    corpus.push({ name, mode, raw, observed: observe(value), wire: JSON.stringify(value) });
  }
  add('full');
  const base = JSON.parse(corpus[corpus.length - 1].raw);
  function refuse(mode, value) {
    const raw = JSON.stringify(value);
    const result = runtime.exports[`witness_${name}`](raw);
    assert.deepEqual(result, value, 'actual TypeScript identity accepts boundary/' + mode);
    refusals.push({ name, mode, raw });
  }
  refuse('root-array', []);
  // Recursively exercise every nested required/optional field and string boundary.
  function boundaries(node, value, route = []) {
    if (node.kind === 'array') {
      value.forEach((item, index) => {
        if (node.item.kind === 'object') {
          const altered = structuredClone(base);
          const array = route.reduce((v, key) => v[key], altered);
          array[index] = Object.values(item);
          refuse('array-item-positional/' + [...route, index].join('/'), altered);
        }
        boundaries(node.item, item, [...route, index]);
      });
    } else if (node.kind === 'object') {
      const extended = structuredClone(base);
      const target = route.reduce((v, key) => v[key], extended);
      target.__orderly_extra__ = 'source retains undeclared property';
      const raw = JSON.stringify(extended);
      const sourceValue = runtime.exports[`witness_${name}`](raw);
      const extraPath = '/' + [...route, '__orderly_extra__'].join('/');
      assert.equal(route.reduce((v, key) => v[key], sourceValue).__orderly_extra__, target.__orderly_extra__, 'source retains unknown property/' + name + extraPath);
      projections.push({ name, mode: 'omit-unknown' + extraPath, raw, extraPath, retained: observe(sourceValue), observed: observe(base), wire: JSON.stringify(base) });
      for (const field of node.fields) {
        const at = [...route, field.name];
        function replace(mode, replacement, remove = false, accepted = false) {
          const altered = structuredClone(base);
          const parent = route.reduce((v, key) => v[key], altered);
          if (remove) delete parent[field.name]; else parent[field.name] = replacement;
          const label = mode + '/' + at.join('/');
          if (accepted) {
            const raw = JSON.stringify(altered);
            const value = runtime.exports[`witness_${name}`](raw);
            corpus.push({ name, mode: label, raw, observed: observe(value), wire: JSON.stringify(value) });
          } else refuse(label, altered);
        }
        if (!field.optional) replace('missing', undefined, true);
        else replace('omit-field', undefined, true, true);
        replace('null', null);
        if (field.shape.kind === 'string') replace('surrogate', '\ud800');
        if (field.shape.kind === 'literal') {
          replace('unknown-literal', 'UNKNOWN');
          replace('tagged-literal', { [field.shape.values[0]]: null });
          replace('array-literal', [field.shape.values[0]]);
        }
        if (field.shape.kind === 'object') replace('nested-positional-array', Object.values(value[field.name]));
        boundaries(field.shape, value[field.name], at);
      }
    }
  }
  boundaries(schema, base);
  refuse('root-positional-array', Object.values(base));
  const paths = [...new Set(numericPaths)];
  add('omit'); add('alternate');
  for (const [field, values] of literalFields) {
    literals.push({ name, field, values });
    for (const value of values) add(field + ':literal:' + value, field, JSON.stringify(value));
  }
  for (const field of paths) for (const token of ['-0', '1e400', '-1e400', '9007199254740993', '5e-324', '1.234567890123456789']) add(field + ':' + token, field, token);
}
console.log(JSON.stringify({ runtime: { node: process.version, arch: process.arch }, cases: corpus, refusals, literals, projections }));
