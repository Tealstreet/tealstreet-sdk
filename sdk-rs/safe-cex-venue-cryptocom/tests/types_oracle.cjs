// Execute the complete checked-in TypeScript module; never a copied implementation.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/cryptocom/cryptocom.types.ts');
const source = fs.readFileSync(file, 'utf8');
const compiled = ts.transpileModule(source, {
  fileName: file,
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  reportDiagnostics: true,
});
assert.equal((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0,
  'typescript_transpile');
const exportsObject = {};
const context = vm.createContext({
  exports: exportsObject,
  require: name => { throw new Error(`Unexpected runtime dependency: ${name}`); },
});
vm.runInContext(compiled.outputText, context, { filename: file });
const names = [
  'BASE_URLS', 'WSS_URLS', 'CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS',
  'CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS', 'ENDPOINTS', 'PUBLIC_ENDPOINTS',
  'GRAFANA_ENDPOINTS_EVERY', 'GRAFANA_ENDPOINTS_PERIODIC',
];
assert.deepEqual(Object.keys(exportsObject).sort(), [...names, 'mapCandlestickInterval'].sort(),
  'runtime_export_census');
const constants = Object.fromEntries(names.map(name => [name, exportsObject[name]]));
const numberObservations = Object.fromEntries(names.filter(name => name.endsWith('_MS')).map(name => {
  const value = exportsObject[name];
  assert.equal(typeof value, 'number', `constant_type:${name}`);
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(Number.isNaN(value) ? NaN : value);
  // NaN alone is canonicalized; retain signed-zero bits.
  return [name, { type: typeof value, bits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex') }];
}));
const inputs = ['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '6h', '12h', '1d', '1w',
  '', '1D', '7D', '1M', ' 1m', '1m ', '🙂', 'toString', 'constructor', '__proto__',
  '__defineGetter__', '__defineSetter__', 'hasOwnProperty', '__lookupGetter__',
  '__lookupSetter__', 'isPrototypeOf', 'propertyIsEnumerable', 'valueOf', 'toLocaleString'];
const intervals = inputs.map(input => {
  let value;
  try { value = exportsObject.mapCandlestickInterval(input); }
  catch (error) { return { input, error: error.message }; }
  // Capture typeof and inherited referent identity BEFORE JSON.stringify.
  const type = typeof value;
  assert(['string', 'function', 'object'].includes(type), `interval_type:${input}`);
  const inherited = type !== 'string';
  if (inherited) {
    context.lookupKey = input;
    const same = vm.runInContext('({})[lookupKey]', context);
    assert.equal(value, same, `inherited_identity:${input}`);
  }
  return { input, type, inherited, value: String(value) };
});
// Explicitly observe the chosen Rust string boundary rather than projecting it to U+FFFD.
const surrogate = '\ud800';
let surrogateError;
try { exportsObject.mapCandlestickInterval(surrogate); } catch (error) { surrogateError = error.message; }
assert(surrogateError.endsWith(surrogate), 'lone_surrogate_diagnostic_retained');
// Interfaces have no emitted runtime body. Their actual compiler-resolved fields
// define this corpus; the whole source module above still executes unchanged.
const recordNames = ['CryptocomInstrument', 'CryptocomTicker', 'CryptocomCandlestick',
  'CryptocomPositionBalance', 'CryptocomError', 'CryptocomTrade', 'CryptocomTradeFill', 'CryptocomOrder'];
const controlSource = `
interface CensusParent { inherited: number; }
interface CensusChild extends CensusParent { own: string; }\ninterface CensusBroken extends MissingBase { own: string; }\ninterface CensusMerged { first: string; }\ninterface CensusMerged extends MissingMergedBase {}\ninterface CensusParentMerged { base: string; }\ninterface CensusParentMerged extends MissingMergedBase {}\ninterface CensusChildMerged extends CensusParentMerged { own: string; }`;
const ast = ts.createSourceFile(file, source + controlSource, ts.ScriptTarget.Latest, true);
const host = ts.createCompilerHost({ noLib: true, noResolve: true, strictNullChecks: true });
host.getSourceFile = name => name === file ? ast : undefined;
host.fileExists = name => name === file;
host.readFile = name => name === file ? source + controlSource : undefined;
const program = ts.createProgram([file], { noLib: true, noResolve: true, strictNullChecks: true }, host);
const checker = program.getTypeChecker();
function schema(name, ancestors = new Set(), allowOptional = false) {
  assert(!ancestors.has(name), `schema_reference_cycle:${name}`);
  const nextAncestors = new Set(ancestors).add(name);
  const declarations = ast.statements.filter(node => ts.isInterfaceDeclaration(node) && node.name.text === name);
  assert.equal(declarations.length, 1, `interface_declaration_count:${name}`);
  const node = declarations[0];
  function validateHeritage(declaration, seen = new Set()) {
    assert(!seen.has(declaration), `heritage_cycle:${name}`);
    seen.add(declaration);
    for (const member of declaration.members) assert(ts.isPropertySignature(member), `interface_member_shape:${name}`);
    for (const clause of declaration.heritageClauses || []) for (const base of clause.types) {
      const parents = checker.getSymbolAtLocation(base.expression)?.declarations;
      assert(parents?.length, `heritage_resolved:${name}`);
      assert.equal(parents.length, 1, `heritage_declaration_count:${name}`);
      const parent = parents[0];
      assert(ts.isInterfaceDeclaration(parent) && parent.getSourceFile() === ast, `heritage_resolved:${name}`);
      validateHeritage(parent, new Set(seen));
    }
  }
  validateHeritage(node);
  const type = checker.getTypeAtLocation(node);
  function describe(property) {
    const declaration = property.valueDeclaration;
    assert(declaration && ts.isPropertySignature(declaration), `property_shape:${name}:${property.name}`);
    assert(allowOptional || !(property.flags & ts.SymbolFlags.Optional), `required_field:${name}:${property.name}`);
    const fieldType = checker.getTypeFromTypeNode(declaration.type);
    if (declaration.type && ts.isTypeReferenceNode(declaration.type)
        && ts.isIdentifier(declaration.type.typeName) && declaration.type.typeName.text === 'Array') {
      // noLib may return a synthetic unresolved symbol; reject source declarations.
      assert(!checker.getSymbolAtLocation(declaration.type.typeName)?.declarations?.length, `array_builtin_shadow:${name}:${property.name}`);
      assert.equal(declaration.type.typeArguments?.length, 1, `array_type_argument:${name}:${property.name}`);
      const element = declaration.type.typeArguments[0];
      if (ts.isTupleTypeNode(element)) {
        const kinds = element.elements.map(member => {
          assert.equal(member.kind, ts.SyntaxKind.StringKeyword, `tuple_string_member:${name}:${property.name}`);
          return 'string';
        });
        return [property.name, 'tuple-array', kinds];
      }
      assert(ts.isTypeLiteralNode(element), `array_inline_element:${name}:${property.name}`);
      for (const member of element.members) assert(ts.isPropertySignature(member), `array_inline_property:${name}:${property.name}`);
      return [property.name, 'array', checker.getPropertiesOfType(checker.getTypeAtLocation(element)).map(child => describe(child))];
    }
    if (declaration.type && ts.isArrayTypeNode(declaration.type)) {
      const element = declaration.type.elementType;
      if (element.kind === ts.SyntaxKind.StringKeyword) return [property.name, 'string-array'];
      assert(ts.isTypeReferenceNode(element) && !element.typeArguments, `array_element_reference:${name}:${property.name}`);
      const declarations = checker.getSymbolAtLocation(element.typeName)?.declarations;
      assert(declarations?.length === 1, `array_reference_declaration_count:${name}:${property.name}`);
      const target = declarations[0];
      assert(ts.isInterfaceDeclaration(target) && target.getSourceFile() === ast, `array_reference_source:${name}:${property.name}`);
      return [property.name, 'array', schema(target.name.text, nextAncestors)];
    }
    if (declaration.type && ts.isTypeLiteralNode(declaration.type)) {
      for (const member of declaration.type.members) assert(ts.isPropertySignature(member), `inline_member_shape:${name}:${property.name}`);
      return [property.name, 'object', checker.getPropertiesOfType(fieldType).map(child => describe(child))];
    }
    const members = fieldType.isUnion() ? fieldType.types : [fieldType];
    if (members.every(member => member.isStringLiteral())) {
      return [property.name, 'string', members.map(member => member.value)];
    }
    const kind = checker.typeToString(fieldType);
    assert(['string', 'number', 'boolean'].includes(kind), `field_kind:${name}:${property.name}:${kind}`);
    return [property.name, kind];
  }
  return checker.getPropertiesOfType(type).map(property => {
    const field = describe(property);
    if (property.flags & ts.SymbolFlags.Optional) { field[3] = true; }
    return field;
  });
}
assert.deepEqual(schema('CensusChild'), [['own', 'string'], ['inherited', 'number']],
  'inherited_required_field_control');
assert.throws(() => schema('CensusBroken'), /heritage_resolved:CensusBroken/, 'unresolved_heritage_refusal_control');
assert.throws(() => schema('CensusMerged'), /interface_declaration_count:CensusMerged/, 'merged_declaration_refusal_control');
assert.throws(() => schema('CensusChildMerged'), /heritage_declaration_count:CensusChildMerged/, 'merged_parent_refusal_control');
function observe(value) {
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(value);
    return {type:'number', bits:Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex')};
  }
  return {type:typeof value, value};
}
const wire = {};
for (const name of recordNames) {
  const fields = schema(name, new Set(), ['CryptocomTradeFill', 'CryptocomOrder'].includes(name));
  const base = Object.fromEntries(fields.map(([field, kind, literals], index) => [field,
    literals ? literals[0] : kind === 'string' ? `${name}:${field}:🙂` : kind === 'number' ? index + 0.25 : kind === 'string-array' ? ['exec:first:🙂', 'exec:second:🙂'] : false]));
  const cases = [];
  function add(label, raw) {
    const native = JSON.parse(raw);
    cases.push({label, raw, observed:Object.fromEntries(fields.map(([field]) => [field, observe(native[field])]))});
  }
  add('distinct_fields', JSON.stringify(base));
  for (const [field, kind, literals, optional] of fields) {
    if (optional) {
      const absent = {...base}; delete absent[field]; add(`absent:${field}`, JSON.stringify(absent));
      if (kind === 'string' && !literals) add(`empty:${field}`, JSON.stringify({...base, [field]:''}));
    }
    if (kind === 'string-array') for (const count of [0,1,2,3]) add(`array:${field}:${count}`, JSON.stringify({...base, [field]:Array.from({length:count}, (_, i) => `exec:${i}:🙂`)}));
    if (literals) for (const literal of literals) add(`literal:${field}:${literal}`, JSON.stringify({...base, [field]:literal}));
    if (kind === 'boolean') add(`one_hot:${field}`, JSON.stringify({...base, [field]:true}));
    if (kind === 'number') for (const token of ['-0', '0', '1e400', '-1e400', '9007199254740993', '5e-324', '1.0000000000000002']) {
      add(`${field}:${token}`, JSON.stringify({...base, [field]:'NUMERIC_TOKEN'}).replace('"NUMERIC_TOKEN"', token));
    }
  }
  const stringField = fields.find(([, kind, literals]) => kind === 'string' && !literals)[0];
  const surrogateRaw = JSON.stringify({...base, [stringField]:'\ud800'});
  const retained = JSON.parse(surrogateRaw)[stringField];
  assert.equal(retained.charCodeAt(0), 0xd800, `wire_lone_surrogate:${name}`);
  const extrasRaw = JSON.stringify({...base, __extra:'retained-in-TS'});
  const extrasRetained = Object.hasOwn(JSON.parse(extrasRaw), '__extra');
  assert(extrasRetained, `ts_extra_retained:${name}`);
  wire[name] = {fields, cases, surrogateRaw, surrogateUnits:[retained.charCodeAt(0)], extrasRaw, extrasRetained};
}
// This inline object has no emitted runtime body; validate its compiler shape,
// then observe actual JavaScript parsed values before crossing JSON transport.
const accountFields = schema('CryptocomAccountsResponse');
assert.deepEqual(accountFields, [['this_account', 'object', [['uuid', 'string']]]], 'account_shape_census');
const outerKey = accountFields[0][0];
const uuidKey = accountFields[0][2][0][0];
const accounts = { fields: accountFields, cases: [] };
for (const uuid of ['account-distinct:🙂', '', 'escaped\"\\\u0000']) {
  const raw = JSON.stringify({[outerKey]: {[uuidKey]: uuid}});
  const native = JSON.parse(raw);
  accounts.cases.push({raw, uuid: observe(native[outerKey][uuidKey])});
}
accounts.extrasRaw = JSON.stringify({[outerKey]: {[uuidKey]:'nested-extra', __inner:'inner'}, __outer:'outer'});
const extrasNative = JSON.parse(accounts.extrasRaw);
accounts.extrasObserved = [extrasNative.__outer, extrasNative[outerKey].__inner];
accounts.surrogateRaw = JSON.stringify({[outerKey]: {[uuidKey]:'\ud800'}});
accounts.surrogateUnits = [JSON.parse(accounts.surrogateRaw)[outerKey][uuidKey].charCodeAt(0)];
assert.deepEqual(accounts.surrogateUnits, [0xd800], 'account_lone_surrogate');
const summaryFields = schema('CryptocomBalanceSummary');
assert.deepEqual(summaryFields.find(([key]) => key === 'position_balances')[2], schema('CryptocomPositionBalance'), 'summary_array_reference_census');
function summaryBase(fields, prefix, count) {
  return Object.fromEntries(fields.map(([key, kind, children], index) => [key,
    kind === 'array' ? Array.from({length:count}, (_, i) => summaryBase(children, `${prefix}.${key}.${i}`, 0)) :
    kind === 'string' ? `${prefix}.${key}:🙂` : kind === 'boolean' ? prefix.endsWith('.1') : index + 0.25]));
}
function summaryObserve(value, fields) {
  return Object.fromEntries(fields.map(([key, kind, children]) => [key,
    kind === 'array' ? value[key].map(item => summaryObserve(item, children)) : observe(value[key])]));
}
const summary = { fields: summaryFields, cases: [] };
for (const count of [0, 1, 2]) {
  const raw = JSON.stringify(summaryBase(summaryFields, 'summary', count));
  const native = JSON.parse(raw);
  summary.cases.push({raw, observed:summaryObserve(native, summaryFields)});
}
const summaryExtras = summaryBase(summaryFields, 'summary', 2);
summaryExtras.__outer = 'outer';
for (const [i, item] of summaryExtras.position_balances.entries()) item.__inner = `inner:${i}`;
summary.extrasRaw = JSON.stringify(summaryExtras);
const summaryExtrasNative = JSON.parse(summary.extrasRaw);
summary.extrasObserved = [summaryExtrasNative.__outer, ...summaryExtrasNative.position_balances.map(item => item.__inner)];
summary.surrogates = [];
for (const path of [['instrument_name'], ['position_balances', 1, 'instrument_name']]) {
  const value = summaryBase(summaryFields, 'summary', 2);
  let target = value;
  for (const key of path.slice(0,-1)) target = target[key];
  target[path.at(-1)] = '\ud800';
  const raw = JSON.stringify(value);
  let native = JSON.parse(raw);
  for (const key of path) native = native[key];
  summary.surrogates.push({path, raw, units:[native.charCodeAt(0)]});
}
const bookFields = schema('CryptocomBook');
assert.deepEqual(bookFields, [['instrument_name','string'], ['depth','number'], ['data','array', [
  ['bids','tuple-array',['string','string','string']], ['asks','tuple-array',['string','string','string']], ['t','number']
]]], 'book_compiler_shape_census');
function bookBase(count) {
  return {instrument_name:'BTC_USD:🙂', depth:7.25, data:Array.from({length:count}, (_, i) => ({
    bids:Array.from({length:i+1}, (_, j) => [`${i}${j}101.25`, `${i}${j}2.50`, `${i}${j}3`]),
    asks:Array.from({length:i+2}, (_, j) => [`${i}${j}104.75`, `${i}${j}5.50`, `${i}${j}6`]),
    t:1000.5+i,
  }))};
}
function observeBook(value) {
  return {instrument_name:observe(value.instrument_name), depth:observe(value.depth), data:value.data.map(row => ({
    bids:row.bids.map(tuple => tuple.map(observe)), asks:row.asks.map(tuple => tuple.map(observe)), t:observe(row.t)
  }))};
}
const book = {fields:bookFields, cases:[]};
function addBook(raw) { book.cases.push({raw, observed:observeBook(JSON.parse(raw))}); }
for (const count of [0,1,2]) addBook(JSON.stringify(bookBase(count)));
const emptySides = bookBase(1); emptySides.data[0].bids = []; emptySides.data[0].asks = []; addBook(JSON.stringify(emptySides));
for (const path of [['depth'], ['data',0,'t'], ['data',1,'t']]) {
  for (const token of ['-0','0','1e400','-1e400','9007199254740993','5e-324','1.0000000000000002']) {
    const value = bookBase(2); let target=value;
    for (const key of path.slice(0,-1)) target=target[key];
    target[path.at(-1)]='NUMERIC_TOKEN';
    addBook(JSON.stringify(value).replace('"NUMERIC_TOKEN"',token));
  }
}
const bookExtra = bookBase(2); bookExtra.__outer='outer'; bookExtra.data[1].__inner='inner';
book.extrasRaw=JSON.stringify(bookExtra);
const bookExtrasNative=JSON.parse(book.extrasRaw);
book.extrasObserved=[bookExtrasNative.__outer,bookExtrasNative.data[1].__inner];
book.surrogates=[];
for (const path of [['instrument_name'],['data',1,'asks',0,1]]) {
  const value=bookBase(2); let target=value;
  for (const key of path.slice(0,-1)) target=target[key];
  target[path.at(-1)]='\ud800';
  const raw=JSON.stringify(value); let native=JSON.parse(raw);
  for (const key of path) native=native[key];
  book.surrogates.push({path,raw,units:[native.charCodeAt(0)]});
}
// Capture input cardinality independently of projected comparison outputs.
const sourceCardinality = {
  order: wire.CryptocomOrder.cases.map(row => { const input=JSON.parse(row.raw); return input.exec_inst === undefined ? null : input.exec_inst.length; }),
  summary: summary.cases.map(row => JSON.parse(row.raw).position_balances.length),
  book: book.cases.map(row => JSON.parse(row.raw).data.map(data => [data.bids.length, data.asks.length])),
};
process.stdout.write(JSON.stringify({ runtime: { node: process.version, nodeMajor: Number(process.versions.node.split(".")[0]), arch: process.arch, platform: process.platform }, constants, numberObservations, intervals, wire, accounts, summary, book, sourceCardinality,
  surrogateErrorUnits: Array.from(surrogateError, char => char.charCodeAt(0)) }));
