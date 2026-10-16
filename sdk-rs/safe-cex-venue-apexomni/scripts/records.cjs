const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const directory = path.join(root, 'packages/safe-cex/src/exchanges/apexomni/interface');
const files = ['apexomni.index.interface.ts', 'apexomni.public.interface.ts', 'apexomni.private.interface.ts'].map(name => path.join(directory, name));
const options = { strict: true, exactOptionalPropertyTypes: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
const host = ts.createCompilerHost(options);
const read = host.readFile.bind(host);
host.readFile = file => {
  let source = read(file);
  if (process.env.APEXOMNI_MUTATE_TS === 'record' && file === files[1]) {
    assert(source.includes('fundingRate: string;'), 'record mutation must apply');
    source = source.replace('fundingRate: string;', 'fundingRate: number;');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'record-boolean' && file === files[1]) {
    assert(source.includes('enableTrade: boolean;'), 'boolean mutation must apply');
    source = source.replace('enableTrade: boolean;', 'enableTrade: string;');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'record-optional' && file === files[0]) {
    assert(source.includes('publicKeyYCoordinate?: string;'), 'optional mutation must apply');
    source = source.replace('publicKeyYCoordinate?: string;', 'publicKeyYCoordinate: string;');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'record-nested' && file === files[1]) {
    assert(source.includes('usdc: CurrencyObject[];'), 'nested mutation must apply');
    source = source.replace('usdc: CurrencyObject[];', 'usdc: string[];');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'record-inherited' && file === files[1]) {
    assert(source.includes('export interface TickerObject {'), 'heritage mutation must apply');
    source = source.replace('export interface TickerObject {', 'interface InheritedControl { inheritedRequired: string; }\nexport interface TickerObject extends InheritedControl {');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'config-inherited' && file === files[1]) {
    assert(source.includes('export interface PerpetualContractObject {'));
    source = source.replace('export interface PerpetualContractObject {', 'export interface PerpetualContractObject { inheritedRequired: string;');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'config-any' && file === files[2]) {
    assert(source.includes('userData?: any;'));
    source = source.replace('userData?: any;', 'userData?: string;');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'config-chains' && file === files[1]) {
    assert(source.includes('chains: any[][];'));
    source = source.replace('chains: any[][];', 'chains: any[];');
  }
  const mutation = process.env.APEXOMNI_MUTATE_TS;
  if (mutation === 'order-iso' && file === files[0]) {
    assert(source.includes('export type ISO8601 = string | number;'));
    source = source.replace('export type ISO8601 = string | number;', 'export type ISO8601 = string | boolean;');
  }
  if (mutation === 'order-price' && file === files[0]) {
    assert(source.includes('humanQuoteAmount?: undefined;'));
    source = source.replace('humanQuoteAmount?: undefined;', 'humanQuoteAmount?: string;');
  }
  if (mutation === 'order-client' && file === files[0]) {
    assert(source.includes('OrderParamsBase & WithPrice & WithClientId'));
    source = source.replace('OrderParamsBase & WithPrice & WithClientId', 'OrderParamsBase & WithPrice');
  }
  if (mutation === 'order-create' && file === files[2]) {
    assert(source.includes("type: 'LIMIT' | 'MARKET' | 'STOP_LIMIT'"));
    source = source.replace("type: 'LIMIT' | 'MARKET' | 'STOP_LIMIT'", "type: 'NEW_CONTROL' | 'LIMIT' | 'MARKET' | 'STOP_LIMIT'");
  }
  if (mutation === 'order-response' && file === files[2]) {
    assert(source.includes('triggerPrice?: string | null;'));
    source = source.replace('triggerPrice?: string | null;', 'triggerPrice?: string;');
  }
  return source;
};
const program = ts.createProgram(files, options, host);
const errors = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error && d.file && files.includes(d.file.fileName));
assert.equal(errors.length, 0, errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
const checker = program.getTypeChecker();
function schema(type, stack = []) {
  if (type.flags & ts.TypeFlags.Any) return { kind: 'any' };
  if (type.flags & ts.TypeFlags.Undefined) return { kind: 'undefined' };
  if (type.flags & ts.TypeFlags.Null) return { kind: 'null' };
  if (type.flags & ts.TypeFlags.StringLiteral) return { kind: 'literal', value: type.value };
  if (type.flags & ts.TypeFlags.NumberLiteral) return { kind: 'literal', value: type.value };
  if (type.flags & ts.TypeFlags.BooleanLiteral) return { kind: 'literal', value: type.intrinsicName === 'true' };
  if (type.flags & ts.TypeFlags.String) return { kind: 'string' };
  if (type.flags & ts.TypeFlags.Number) return { kind: 'number' };
  if (type.flags & ts.TypeFlags.Boolean) return { kind: 'boolean' };
  if (type.isUnion()) return { kind: 'union', choices: type.types.map(t => schema(t, stack)) };
  if (checker.isTupleType(type)) return { kind: 'tuple', items: checker.getTypeArguments(type).map(t => schema(t, stack)) };
  if (checker.isArrayType(type)) return { kind: 'array', item: schema(checker.getElementTypeOfArrayType(type), stack) };
  assert(!stack.includes(type), 'recursive record needs explicit oracle support');
  const fields = checker.getPropertiesOfType(type).map(symbol => {
    const declaration = symbol.valueDeclaration || symbol.declarations[0];
    return { name: symbol.name, optional: !!(symbol.flags & ts.SymbolFlags.Optional), type: schema(checker.getTypeOfSymbolAtLocation(symbol, declaration), [...stack, type]) };
  });
  assert(fields.length, `unclassified type ${checker.typeToString(type)}`);
  return { kind: 'record', fields };
}
const records = {};
for (const file of files) {
  const source = program.getSourceFile(file);
  for (const item of source.statements) {
    if (!item.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)) continue;
    if (ts.isInterfaceDeclaration(item) || (ts.isTypeAliasDeclaration(item) && ['OrderWithClientId', 'ISO8601'].includes(item.name.text))) records[item.name.text] = schema(checker.getTypeAtLocation(item));
  }
}
// Counter-control: simulate a census silently dropping only the new inherited
// field. The Rust control must reject unrelated nested-record failures as proof.
if (process.env.APEXOMNI_BLIND_SYMBOL_INFO_CONTROL === '1') {
  assert.equal(process.env.APEXOMNI_MUTATE_TS, 'config-inherited');
  assert(records.SymbolInfoObject.fields.some(field => field.name === 'inheritedRequired'));
  records.SymbolInfoObject.fields = records.SymbolInfoObject.fields.filter(field => field.name !== 'inheritedRequired');
}
const lone = JSON.parse('"\\ud800"');
records.$stringBoundary = { units: Array.from({ length: lone.length }, (_, i) => lone.charCodeAt(i)), serialized: JSON.stringify(lone) };
records.$numbers = ['-0', '-0.0', '0', '1', '1.0', '1e400', '-1e400', '1e-400', '-1e-400', '1e-7', '1e-6', '1e20', '1e21', '9007199254740993'].map(raw => {
  const record = JSON.parse(`{"a":[],"b":[],"s":"BTC-USDT","u":${raw}}`);
  assert.equal(typeof record.u, 'number', 'numeric witness typeof before serialization');
  const bits = Buffer.alloc(8); bits.writeDoubleBE(record.u);
  return { raw, bits: bits.toString('hex'), tag: Object.is(record.u, -0) ? '-0' : String(record.u), serialized: JSON.stringify(record), numberJson: JSON.stringify(record.u) };
});
records.$nativeNumbers = [
  ['NaN', NaN], ['negative NaN', -NaN], ['Infinity', Infinity],
  ['-Infinity', -Infinity], ['-0', -0], ['0', 0], ['fraction', 1.25],
].map(([tag, value]) => {
  assert.equal(typeof value, 'number', 'native number typeof before JSON');
  const bits = Buffer.alloc(8); bits.writeDoubleBE(value);
  return { tag, type: typeof value, rawBits: bits.toString('hex'), bits: Number.isNaN(value) ? '7ff8000000000000' : bits.toString('hex'), numberJson: JSON.stringify(value) };
});
records.$undefined = [{}, { humanQuoteAmount: undefined, nonce: undefined }].map(value => ({
  ownPrice: Object.hasOwn(value, 'humanQuoteAmount'), ownNonce: Object.hasOwn(value, 'nonce'),
  priceType: typeof value.humanQuoteAmount, nonceType: typeof value.nonce, json: JSON.stringify(value),
}));
records.$runtime = { node: process.version, nodeMajor: Number(process.versions.node.split('.')[0]), arch: process.arch, rustArch: process.env.SAFE_CEX_RUST_ARCH };
process.stdout.write(JSON.stringify(records));
