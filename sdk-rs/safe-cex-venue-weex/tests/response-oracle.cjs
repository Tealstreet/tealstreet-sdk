const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { load, capture: captureScalar, root, ts, header } = require('./types-oracle.cjs');
function capture(value) {
  if (Array.isArray(value)) return { kind: 'array', values: value.map(capture) };
  if (value !== null && typeof value === 'object') return { kind: 'object', entries: Object.entries(value).map(([key, item]) => [key, capture(item)]) };
  return captureScalar(value);
}
assert.notDeepEqual(capture([]), capture({}), 'capture_array_object');
assert.notDeepEqual(capture([['a', 1]]), capture({ a: 1 }), 'capture_nonempty_array_object');
const cases = JSON.parse(fs.readFileSync(path.join(__dirname, 'response-cases.json'), 'utf8'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts');
const virtual = path.join(path.dirname(file), '__weex_response_witness.ts');
const names = ['WeexKlineRow', 'WeexExchangeInfoResponse', 'WeexOrderResult'];
let source = `import type { ${names.join(', ')} } from './weex.types';\n`;
source += 'interface WitnessBase { inheritedRequired: string }\ninterface WitnessChild extends WitnessBase { ownRequired: number }\n';
const spans = [];
for (let i = 0; i < cases.length; i++) {
  const start = source.length;
  const target = cases[i].type === 'WeexKlineRow' ? cases[i].type : `(${cases[i].type} & Record<string, unknown>)`;
  source += `const row_${i}: ${target} = (${cases[i].raw});\n`;
  spans.push([start, source.length]);
}
const options = { strict: true, skipLibCheck: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, noEmit: true, types: [] };
const host = ts.createCompilerHost(options);
const read = host.readFile.bind(host), exists = host.fileExists.bind(host);
host.readFile = name => name === virtual ? source : read(name);
host.fileExists = name => name === virtual || exists(name);
const program = ts.createProgram([virtual, file], options, host);
const checker = program.getTypeChecker();
const witness = program.getSourceFile(virtual);
assert.ok(witness, 'witness source loaded');
assert.equal(program.getSyntacticDiagnostics(witness).length, 0, 'witness_syntax');
// Expected assignment failures are classified by source span, not suppressed globally.
const diagnostics = program.getSemanticDiagnostics(witness);
for (const diagnostic of diagnostics) {
  assert.ok(spans.some(([start, end]) => diagnostic.start >= start && diagnostic.start < end), ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
}
const venueSource = program.getSourceFile(file);
const schema = {};
function describe(type, location) {
  if (type.flags & ts.TypeFlags.Any) return 'any';
  if (type.flags & ts.TypeFlags.StringLike) return 'string';
  if (type.flags & ts.TypeFlags.NumberLike) return 'number';
  if (type.flags & ts.TypeFlags.BooleanLike) return 'boolean';
  if (type.flags & ts.TypeFlags.Undefined) return 'undefined';
  if (type.isUnion()) {
    const parts = [...new Set(type.types.map(t => describe(t, location)))].sort();
    return parts.length === 1 ? parts[0] : { union: parts };
  }
  if (checker.isTupleType(type)) return { tuple: checker.getTypeArguments(type).map(t => describe(t, location)) };
  if (checker.isArrayType(type)) return { array: describe(checker.getTypeArguments(type)[0], location) };
  const index = checker.getIndexTypeOfType(type, ts.IndexKind.String);
  if (index) return { record: describe(index, location) };
  assert.ok(type.flags & ts.TypeFlags.Object, `unsupported census kind ${checker.typeToString(type)}`);
  return { fields: checker.getPropertiesOfType(type).map(property => ({
    name: property.name,
    optional: !!(property.flags & ts.SymbolFlags.Optional),
    type: describe(checker.getTypeOfSymbolAtLocation(property, location), location),
  })) };
}
for (const statement of venueSource.statements) {
  if (ts.isTypeAliasDeclaration(statement) && names.includes(statement.name.text)) {
    schema[statement.name.text] = describe(checker.getTypeAtLocation(statement), statement);
  }
}
assert.deepEqual(Object.keys(schema), names, 'type_inventory');
const child = witness.statements.find(s => ts.isInterfaceDeclaration(s) && s.name.text === 'WitnessChild');
const inherited = describe(checker.getTypeAtLocation(child), child).fields;
assert.deepEqual(inherited.map(f => f.name).sort(), ['inheritedRequired', 'ownRequired'], 'census_inherited_required');
assert.ok(inherited.every(f => !f.optional), 'census_inherited_optionality');
const parser = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const rows = cases.map((entry, i) => {
  const [start, end] = spans[i];
  const accepted = !diagnostics.some(d => d.start >= start && d.start < end);
  assert.equal(accepted, entry.accepted, `source_shape::${entry.name}`);
  // This is the actual shared TS parser. REST's bigint-string transform is a later slice.
  const parsed = parser.jsonParse(entry.raw);
  const projected = entry.type === 'WeexKlineRow' || !entry.accepted ? parsed : Object.fromEntries(schema[entry.type].fields.filter(f => Object.hasOwn(parsed, f.name)).map(f => [f.name, parsed[f.name]]));
  return { ...entry, capture: capture(projected) };
});
const byName = new Map(rows.map(row => [row.name, row]));
const distinctKline = parser.jsonParse(byName.get('kline_distinct').raw);
assert.equal(new Set(distinctKline.filter(v => typeof v === 'string')).size, 8, 'source_distinct::kline_strings');
assert.equal(new Set(distinctKline.filter(v => typeof v === 'number')).size, 3, 'source_distinct::kline_numbers');
const distinctInfo = parser.jsonParse(byName.get('info_distinct').raw);
assert.notDeepEqual(capture(distinctInfo.assets), capture(distinctInfo.symbols), 'source_distinct::assets_symbols');
const distinctOrder = parser.jsonParse(byName.get('order_distinct').raw);
assert.equal(new Set(Object.values(distinctOrder).filter(v => typeof v === 'string')).size, 4, 'source_distinct::order_strings');
const depthRaw = '{"assets":[{"nested":' + '['.repeat(150) + '0' + ']'.repeat(150) + '}]}';
assert.notEqual(parser.jsonParse(depthRaw), null, 'source_depth_accepted');
const orderDepthRaw = '{"orderId":"ok","extra":' + '['.repeat(150) + '0' + ']'.repeat(150) + '}';
assert.equal(parser.jsonParse(orderDepthRaw).orderId, 'ok', 'source_order_depth_accepted');
process.stdout.write(JSON.stringify({ header, schema, rows, depth: { raw: depthRaw, sourceAccepted: true }, orderDepth: { raw: orderDepthRaw, sourceAccepted: true } }));
