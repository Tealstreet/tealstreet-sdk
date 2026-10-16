'use strict';
const { load, root, ts, fs, path, vm, assert } = require('./oracle-loader.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts');
load(file); // Execute the maintained module and its actual enum dependencies.
const controlFile = path.join(__dirname, 'heritage-control.ts');
const host = ts.createCompilerHost({ strictNullChecks: true });
const read = host.getSourceFile.bind(host);
host.getSourceFile = (name, version, ...rest) => name === controlFile
  ? ts.createSourceFile(name, 'interface Parent { inherited: string }; interface Child extends Parent { own: number }', version, true)
  : read(name, version, ...rest);
const program = ts.createProgram([file, controlFile], { strictNullChecks: true }, host);
const checker = program.getTypeChecker();
function named(name, source = program.getSourceFile(file)) {
  const node = source.statements.find(node => node.name?.text === name);
  assert(node, `missing declaration ${name}`);
  return checker.getTypeAtLocation(node);
}
function properties(type) { return checker.getPropertiesOfType(type); }
assert.deepEqual(properties(named('Child', program.getSourceFile(controlFile))).map(p => p.name).sort(),
  ['inherited', 'own'], 'inherited required-field census');
assert.equal(properties(named('Child', program.getSourceFile(controlFile))).find(p => p.name === 'inherited').flags & ts.SymbolFlags.Optional, 0, 'inherited field remains required');
const response = named('GET_PENDING_POSITIONS_RESPONSE');
assert(checker.isArrayType(response), 'pending response must remain an array');
const row = checker.getTypeArguments(response)[0];
const fields = properties(row).map(symbol => {
  const type = checker.getTypeOfSymbolAtLocation(symbol, symbol.valueDeclaration);
  const arms = type.isUnion() ? type.types.filter(t => !(t.flags & ts.TypeFlags.Undefined)) : [type];
  let values;
  if (arms.every(t => t.flags & ts.TypeFlags.StringLiteral)) values = arms.map(t => t.value);
  else if (arms.length === 1 && arms[0].flags === ts.TypeFlags.String) values = 'string';
  else if (arms.length === 1 && arms[0].flags === ts.TypeFlags.Number) values = 'number';
  else throw new Error(`unsupported type ${symbol.name}: ${checker.typeToString(type)}`);
  return { name: symbol.name, optional: !!(symbol.flags & ts.SymbolFlags.Optional), values };
});
const base = Object.fromEntries(fields.map((f, i) => [f.name, Array.isArray(f.values) ? f.values[0] : f.values === 'string' ? `${f.name}:${i}:distinct` : i + 0.375]));
const cases = [];
function add(label, value, accepted = true) { cases.push({ label, raw: JSON.stringify(value), accepted }); }
add('all fields', [base]);
add('unicode scalar astral whitespace', [{ ...base, symbol: 'µ漢字😀\uFEFF\u0000 \t\n' }]);
for (const unit of [0xd800, 0xdfff]) {
  add(`chosen lone surrogate ${unit}`, [{ ...base, symbol: String.fromCharCode(unit) }], false);
  cases.at(-1).sourceAccepted = true;
  cases.at(-1).surrogate = unit;
}
add('optional absent', [Object.fromEntries(fields.filter(f => !f.optional).map(f => [f.name, base[f.name]]))]);
for (const f of fields) {
  const missing = { ...base }; delete missing[f.name];
  add(`missing ${f.name}`, [missing], f.optional);
  add(`null ${f.name}`, [{ ...base, [f.name]: null }], false);
  add(`wrong type ${f.name}`, [{ ...base, [f.name]: f.values === 'number' ? '123' : 123 }], false);
  if (Array.isArray(f.values)) {
    for (const value of f.values) add(`${f.name} ${value}`, [{ ...base, [f.name]: value }]);
    add(`object enum ${f.name}`, [{ ...base, [f.name]: { [f.values[0]]: null } }], false);
    add(`invalid ${f.name}`, [{ ...base, [f.name]: '__invalid' }], false);
  }
}
add('array row refused', [Object.values(base)], false);
add('object root refused', base, false);
const numeric = ['-0', '-0.0', '1e400', '-1e400', '9007199254740993', '5e-324', '1.0000000000000002'];
for (const f of fields.filter(f => f.values === 'number')) for (const token of numeric) {
  const raw = JSON.stringify([{ ...base, [f.name]: '__NUMERIC__' }]).replace('"__NUMERIC__"', token);
  cases.push({ label: `bits ${f.name} ${token}`, raw, accepted: true });
}
// Run typed capture code compiled in the actual venue module. Types erase at runtime;
// acceptance above is the resolved declaration contract, not a TS runtime validator.
const captureSource = fs.readFileSync(file, 'utf8') + `\nexport function capture(value: GET_PENDING_POSITIONS_RESPONSE) { return value; }`;
const moduleRecord = { exports: {} };
const compiled = ts.transpileModule(captureSource, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
vm.runInThisContext(`(function(require,module,exports){${compiled}\n})`)(id => load(path.resolve(path.dirname(file), id + '.ts')), moduleRecord, moduleRecord.exports);
function observe(value) {
  if (typeof value === 'number') {
    const b = Buffer.alloc(8); b.writeDoubleBE(value);
    return { type: 'number', bits: Number.isNaN(value) ? '7ff8000000000000' : b.toString('hex') };
  }
  assert.equal(typeof value, 'string', 'wire primitive capture');
  return { type: 'string', utf16: Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)) };
}
for (const c of cases) if (c.accepted || c.sourceAccepted) c.native = moduleRecord.exports.capture(JSON.parse(c.raw)).map(row =>
  Object.fromEntries(Object.entries(row).map(([key, value]) => [key, observe(value)])));
const interval = named('KlineInterval');
assert(interval.isUnion() && interval.types.every(t => t.flags & ts.TypeFlags.StringLiteral), 'interval literal union');
process.stdout.write(JSON.stringify({ fields, cases, intervals: interval.types.map(t => t.value) }));
