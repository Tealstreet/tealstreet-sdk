const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const directory = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/zklink/signer');
const declarations = new Map();
for (const name of ['types.ts', ...['contract', 'order', 'transfer', 'withdraw', 'changePubKey'].map(n => `transactions/${n}.ts`)]) {
  const file = path.join(directory, name);
  const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  for (const node of tree.statements) {
    if (ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node)) declarations.set(node.name.text, node.getText(tree));
  }
}
// Compile the actual declaration closure without executing wallet/WASM imports.
const filename = path.join(directory, 'transaction-record-oracle.ts');
const code = [...declarations.values()].join('\n');
const options = { strict: true, exactOptionalPropertyTypes: true, target: ts.ScriptTarget.ES2022 };
const host = ts.createCompilerHost(options);
const read = host.readFile.bind(host);
host.readFile = file => file === filename ? code : read(file);
const program = ts.createProgram([filename], options, host);
const errors = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error);
assert.equal(errors.length, 0, errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
const checker = program.getTypeChecker();
const source = program.getSourceFile(filename);
const schemas = {};
function shape(type) {
  if (type.flags & ts.TypeFlags.String) return {kind:'string'};
  if (type.flags & ts.TypeFlags.Number) return {kind:'number'};
  if (type.flags & ts.TypeFlags.Boolean || type.isUnion() && type.types.every(t => t.flags & ts.TypeFlags.BooleanLiteral)) return {kind:'boolean'};
  if (checker.isTupleType(type)) return {kind:'tuple', items:checker.getTypeArguments(type).map(shape)};
  if (type.flags & ts.TypeFlags.StringLiteral) return {kind:'literal', values:[type.value]};
  if (type.isUnion()) return {kind:'union', items:type.types.map(shape)};
  if (type.flags & ts.TypeFlags.NumberLiteral) return {kind:'literal', values:[type.value]};
  assert(type.symbol, `unclassified actual type ${checker.typeToString(type)}`);
  return {kind:'record', name:type.symbol.name};
}
for (const node of source.statements) {
  if (!ts.isInterfaceDeclaration(node)) continue;
  schemas[node.name.text] = checker.getPropertiesOfType(checker.getTypeAtLocation(node)).map(symbol => {
    const optional = !!(symbol.flags & ts.SymbolFlags.Optional);
    const type = checker.getTypeOfSymbolAtLocation(symbol, symbol.valueDeclaration);
    const alternatives = type.isUnion() ? type.types.filter(t => !(t.flags & ts.TypeFlags.Undefined)) : [type];
    return {name:symbol.name, optional, shape:alternatives.length === 1 ? shape(alternatives[0]) : shape(type)};
  });
}
const alias = source.statements.find(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'ChangePubkeyTypes');
const kinds = shape(checker.getTypeAtLocation(alias));
function sampleShape(desc, field, index, variant) {
  if (desc.kind === 'string') return `${field}-é-😀-${index}`;
  if (desc.kind === 'number') return index + 0.25;
  if (desc.kind === 'boolean') return field === 'hasSubsidy' ? variant === 1 : variant !== 1;
  if (desc.kind === 'literal') return desc.values[0];
  if (desc.kind === 'tuple') return desc.items.map((d, i) => sampleShape(d, `${field}.${i}`, index + i + 30, variant));
  if (desc.kind === 'union') return sampleShape(desc.items[(variant + (field === 'hasSubsidy' ? 1 : 0)) % desc.items.length], field, index, variant);
  return sample(desc.name, variant);
}
function sample(name, variant) {
  return Object.fromEntries(schemas[name].filter(f => !f.optional || variant !== 0).map((f, i) => [f.name, sampleShape(f.shape, f.name, i + 1, variant)]));
}
function bits(value) {
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  return Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex');
}
function observe(value) {
  if (typeof value === 'number') return {type:'number', bits:bits(value)};
  if (Array.isArray(value)) return value.map(observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, v]) => [key, observe(v)]));
  return value;
}
const names = Object.keys(schemas).filter(n => n !== 'TxEthSignature');
assert.equal(names.length, 19, 'transaction interface census');
function stringPath(name) {
  for (const field of schemas[name]) {
    if (field.shape.kind === 'string') return [field.name];
    if (field.shape.kind === 'record') {
      const nested = stringPath(field.shape.name);
      if (nested) return [field.name, ...nested];
    }
  }
  return null;
}
const rows = names.map(name => ({name, fields:schemas[name], samples:[0,1,2,3].map(variant => {
  const value = sample(name, variant);
  return {raw:JSON.stringify(value), native:observe(value)};
}).concat(['-0', '1e400', '5e-324'].map(token => {
  const value = sample(name, 1);
  const raw = '{' + schemas[name].map(field => {
    const flag = field.shape.kind === 'union' && field.shape.items.every(d => d.kind === 'literal' && typeof d.values[0] === 'number');
    const encoded = field.shape.kind === 'number' || flag && token === '-0' ? token : field.shape.kind === 'tuple' ? `[-0,${token}]` : JSON.stringify(value[field.name]);
    return `${JSON.stringify(field.name)}:${encoded}`;
  }).join(',') + '}';
  return {raw, native:observe(JSON.parse(raw))};
}))}));
const surrogate = JSON.parse('"\\ud800"');
assert.equal(surrogate.charCodeAt(0), 0xd800);
for (const row of rows) {
  const keys = stringPath(row.name);
  row.boundary = null;
  if (!keys) continue;
  const value = sample(row.name, 1);
  let parent = value;
  for (const key of keys.slice(0, -1)) parent = parent[key];
  parent[keys.at(-1)] = surrogate;
  row.boundary = {path:keys, units:[surrogate.charCodeAt(0)], raw:JSON.stringify(value)};
}
assert.equal(rows.filter(row => row.boundary !== null).length, 18, 'string boundary record floor');
console.log(JSON.stringify({header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH},kinds,rows,boundary:{units:[surrogate.charCodeAt(0)],raw:JSON.stringify({pubKey:surrogate,signature:'unchanged'})}}));
