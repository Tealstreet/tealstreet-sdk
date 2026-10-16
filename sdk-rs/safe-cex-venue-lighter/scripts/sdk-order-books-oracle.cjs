const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/client/order-api.ts');
const mutation = process.env.LIGHTER_ORDER_BOOKS_MUTATION;
let source = fs.readFileSync(file, 'utf8');
const names = ['OrderBooksResponse', 'OrderBookOrdersResponse', 'OrderBookDetailsResponse'];
if (mutation === 'missing-import') source = source.replace("import { Trade, OrderBookLevel } from '../types/common';", '');
for (const name of names) if (mutation === `heritage-${name}`) source = source.replace(`interface ${name} {`, `interface ${name} extends Extra {`) + '\ninterface Extra { hidden: number; }';
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0, 'BOOKS source syntax');
const imports = tree.statements.filter(n => ts.isImportDeclaration(n) && n.moduleSpecifier.text === '../types/common');
assert.equal(imports.length, 1, 'BOOKS actual common import inventory');
assert.equal(imports[0].importClause.getText(tree), '{ Trade, OrderBookLevel }', 'BOOKS actual common import binding');
const declarations = names.map(name => {
  const found = tree.statements.filter(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.equal(found.length, 1, `${name} source inventory`);
  assert.ok(!found[0].heritageClauses?.length, `${name} unsupported heritage`);
  return found[0];
});
function shape(type) {
  if (ts.isArrayTypeNode(type)) return { array: shape(type.elementType) };
  if (ts.isTypeReferenceNode(type) && type.typeName.getText(tree) === 'Array') {
    assert.equal(type.typeArguments.length, 1, 'BOOKS Array arity');
    return { array: shape(type.typeArguments[0]) };
  }
  if (ts.isTypeLiteralNode(type)) return members(type.members);
  return type.getText(tree);
}
function members(fields) {
  return fields.map(field => {
    assert.ok(ts.isPropertySignature(field) && field.type && field.name, 'BOOKS supported property');
    assert.ok(!field.questionToken, 'BOOKS required property');
    return [field.name.getText(tree), shape(field.type)];
  });
}
const schemas = Object.fromEntries(declarations.map(n => [n.name.text, members(n.members)]));
const level = [['price', 'string'], ['quantity', 'string'], ['orderCount', 'number']];
const expected = {
  OrderBooksResponse: [['orderBooks', { array: [['marketId', 'number'], ['symbol', 'string'], ['bestBid', 'string'], ['bestAsk', 'string'], ['bids', { array: 'OrderBookLevel' }], ['asks', { array: 'OrderBookLevel' }], ['timestamp', 'number']] }]],
  OrderBookOrdersResponse: [['marketId', 'number'], ['bids', { array: level }], ['asks', { array: level }], ['timestamp', 'number']],
  OrderBookDetailsResponse: [['details', { array: [['marketId', 'number'], ['symbol', 'string'], ['totalBidVolume', 'string'], ['totalAskVolume', 'string'], ['bidOrderCount', 'number'], ['askOrderCount', 'number'], ['midPrice', 'string'], ['spread', 'string'], ['spreadPercentage', 'string'], ['lastUpdate', 'number']] }]],
};
assert.deepEqual(schemas, expected, 'BOOKS source schema');
const selected = imports[0].getFullText(tree) + declarations.map(n => n.getFullText(tree)).join('\n');
const virtual = path.join(path.dirname(file), '__books_probe__.ts');
const options = { target: ts.ScriptTarget.ES2022, strictNullChecks: true, skipLibCheck: true };
const host = ts.createCompilerHost(options), read = host.readFile.bind(host), exists = host.fileExists.bind(host);
host.readFile = f => {
  if (f === virtual) return selected;
  let text = read(f);
  if (f.endsWith('/types/common.ts')) {
    if (mutation === 'missing-level') text = text.replace('export interface OrderBookLevel', 'export interface RemovedLevel');
    if (mutation === 'level-type') text = text.replace(/(export interface OrderBookLevel[\s\S]*?price): string/, '$1: number');
    if (mutation === 'level-method') text = text.replace(/(export interface OrderBookLevel[\s\S]*?price): string/, '$1(): string');
    if (mutation === 'level-optional') text = text.replace(/(export interface OrderBookLevel[\s\S]*?quantity): string/, '$1?: string');
  }
  return text;
};
host.fileExists = f => f === virtual || exists(f);
const program = ts.createProgram([virtual], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, `BOOKS selected program diagnostics: ${diagnostics.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('; ')}`);
// Pin the actual imported declaration, not only its local reference spelling.
const commonTree = program.getSourceFile(path.resolve(path.dirname(file), '../types/common.ts'));
const commonLevels = commonTree.statements.filter(n => ts.isInterfaceDeclaration(n) && n.name.text === 'OrderBookLevel');
assert.equal(commonLevels.length, 1, 'BOOKS imported level inventory');
assert.ok(!commonLevels[0].heritageClauses?.length, 'BOOKS imported level unsupported heritage');
assert.deepEqual(commonLevels[0].members.map(f => {
  assert.ok(ts.isPropertySignature(f), 'BOOKS imported level supported property');
  return [f.name.getText(commonTree), f.type.getText(commonTree), !!f.questionToken];
}),
  [['price', 'string', false], ['quantity', 'string', false]], 'BOOKS imported level schema');
const makeLevel = (i, side) => ({ price: `${side}-price-${i}`, quantity: `${side}-quantity-${i}` });
const makeBook = i => ({ marketId: 11.25 + i, symbol: `symbol-${i}`, bestBid: `best-bid-${i}`, bestAsk: `best-ask-${i}`, bids: [makeLevel(1, 'bid'), makeLevel(3, 'bid'), makeLevel(1, 'bid'), makeLevel(5, 'bid')], asks: [makeLevel(2, 'ask'), makeLevel(4, 'ask')], timestamp: 81.75 + i });
const makeDetails = i => ({ marketId: 21.25 + i, symbol: `details-${i}`, totalBidVolume: `bid-volume-${i}`, totalAskVolume: `ask-volume-${i}`, bidOrderCount: 31.5 + i, askOrderCount: 42.75 + i, midPrice: `mid-${i}`, spread: `spread-${i}`, spreadPercentage: `percent-${i}`, lastUpdate: 93.125 + i });
const bases = {
  OrderBooksResponse: { orderBooks: [makeBook(2), makeBook(0), makeBook(2), makeBook(1)] },
  OrderBookOrdersResponse: { marketId: 73.25, bids: [{ ...makeLevel(1, 'bid'), orderCount: 14.5 }, { ...makeLevel(2, 'bid'), orderCount: 25.75 }], asks: [{ ...makeLevel(3, 'ask'), orderCount: 36.125 }], timestamp: 84.5 },
  OrderBookDetailsResponse: { details: [makeDetails(3), makeDetails(1), makeDetails(3), makeDetails(0)] },
};
const get = (value, keys) => keys.reduce((value, key) => value[key], value);
const set = (value, keys, next) => { get(value, keys.slice(0, -1))[keys.at(-1)] = next; };
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
const numeric = [], strings = [], collections = [], extras = [], depths = [];
const lexemes = ['-0', '-0.0', '-0e0', '0', '1e3', '1e400', '-1e400', '16777217.25', '-1e-400'];
const texts = ['', ' x ', 'é', '👩', '\ufeff', '\u0085', '\ud800', '\udfff', 'x\ud800y'];
function leaves(value, keys = []) {
  if (Array.isArray(value)) return value.length ? leaves(value[0], [...keys, 0]) : [];
  if (value && typeof value === 'object') return Object.entries(value).flatMap(([key, child]) => leaves(child, [...keys, key]));
  return [{ keys, type: typeof value }];
}
for (const record of names) for (const { keys, type } of leaves(bases[record])) {
  if (type === 'number') for (const raw of lexemes) {
    const input = structuredClone(bases[record]); set(input, keys, '__NUMBER__');
    const encoded = JSON.stringify(input).replace('"__NUMBER__"', raw);
    numeric.push({ record, keys, raw, encoded, bits: bits(get(JSON.parse(encoded), keys)) });
  }
  if (type === 'string') for (const text of texts) {
    const input = structuredClone(bases[record]); set(input, keys, text);
    const encoded = JSON.stringify(input), actual = get(JSON.parse(encoded), keys);
    strings.push({ record, keys, encoded, type: typeof actual, length: actual.length, units: units(actual), representable: actual.isWellFormed() });
  }
}
const axes = [ ['OrderBooksResponse', ['orderBooks']], ['OrderBooksResponse', ['orderBooks', 0, 'bids']], ['OrderBooksResponse', ['orderBooks', 0, 'asks']], ['OrderBookOrdersResponse', ['bids']], ['OrderBookOrdersResponse', ['asks']], ['OrderBookDetailsResponse', ['details']] ];
for (const [record, keys] of axes) for (const mode of ['empty', 'single', 'duplicate']) {
  const input = structuredClone(bases[record]), first = get(input, keys)[0];
  const items = mode === 'empty' ? [] : mode === 'single' ? [first] : [first, first, first, first];
  set(input, keys, items); const encoded = JSON.stringify(input);
  collections.push({ record, keys, mode, encoded, count: get(JSON.parse(encoded), keys).length });
}
const locations = [ ['OrderBooksResponse', []], ['OrderBooksResponse', ['orderBooks', 0]], ['OrderBooksResponse', ['orderBooks', 0, 'bids', 0]], ['OrderBooksResponse', ['orderBooks', 0, 'asks', 0]], ['OrderBookOrdersResponse', []], ['OrderBookOrdersResponse', ['bids', 0]], ['OrderBookOrdersResponse', ['asks', 0]], ['OrderBookDetailsResponse', []], ['OrderBookDetailsResponse', ['details', 0]] ];
for (const [record, keys] of locations) {
  for (const text of texts) {
    const input = structuredClone(bases[record]); get(input, keys).extra = text;
    const encoded = JSON.stringify(input), actual = get(JSON.parse(encoded), keys).extra;
    extras.push({ record, keys, encoded, type: typeof actual, length: actual.length, units: units(actual), representable: actual.isWellFormed() });
  }
  for (const count of [122, 123, 124, 125, 126, 127, 140]) {
    let extra = 17; for (let i = 0; i < count; i++) extra = [extra];
    const input = structuredClone(bases[record]); get(input, keys).extra = extra;
    const encoded = JSON.stringify(input); let actual = get(JSON.parse(encoded), keys).extra, observed = 0;
    while (Array.isArray(actual)) { assert.equal(actual.length, 1); observed++; actual = actual[0]; }
    assert.equal(observed, count); assert.equal(actual, 17);
    depths.push({ record, keys, count, encoded });
  }
}
if (mutation === 'content') bases.OrderBooksResponse.orderBooks[0].bestBid = 'bad-bid-2!';
if (mutation === 'reverse') bases.OrderBooksResponse.orderBooks.reverse();
// Preserve the encoded size and metadata while changing the lone UTF16 unit.
if (mutation === 'surrogate-content') {
  const original = strings[6].encoded;
  strings[6].encoded = original.replace('\\ud800', '\\ud801');
  assert.notEqual(strings[6].encoded, original, 'BOOKS surrogate control changes bytes');
  assert.equal(strings[6].encoded.length, original.length, 'BOOKS surrogate control preserves length');
}
const body = { schemas, bases, numeric, strings, collections, extras, depths };
const contentHash = require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex');
process.stdout.write(JSON.stringify({ header: { nodeVersion: process.version, captureArchitecture: process.arch, capturePlatform: process.platform, scope: 'Selected byte-exact interfaces and actual common import; no runtime OrderApi adoption' }, contentHash, ...body }));
