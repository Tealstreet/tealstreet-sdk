const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/orderly/orderly.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = tree.statements.filter(s => ts.isClassDeclaration(s) && s.name?.text === 'OrderlyExchange');
assert.equal(classes.length, 1, 'OrderlyExchange class census');
const pieces = [];
for (const name of ['mapMarginType', 'toOrderlyMarginMode', 'createOrderlyPositionId']) {
  const fields = classes[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(tree) === name);
  assert.equal(fields.length, 1, `margin helper census/${name}`);
  assert.ok(fields[0].initializer && ts.isArrowFunction(fields[0].initializer), `margin helper arrow/${name}`);
  // Execute the complete actual initializer, without editing its function body.
  pieces.push(`export const ${name} = ${fields[0].initializer.getText(tree)};`);
}
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
for (const name of ['MarginType', 'PositionSide', 'PositionMode']) {
  const enums = types.statements.filter(s => ts.isEnumDeclaration(s) && s.name.text === name);
  assert.equal(enums.length, 1, `shared enum declaration census/${name}`);
  pieces.unshift(enums[0].getText(types));
}
const sharedFile = path.join(root, 'packages/safe-cex/src/utils/createPositionId.ts');
const sharedTree = ts.createSourceFile(sharedFile, fs.readFileSync(sharedFile, 'utf8'), ts.ScriptTarget.Latest, true);
const shared = sharedTree.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => d.name.getText(sharedTree) === 'createPositionId'));
assert.equal(shared.length, 1, 'actual createPositionId declaration census');
pieces.push(shared[0].getText(sharedTree));
const code = ts.transpileModule(pieces.join('\n'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const mod = { exports: {} };
new vm.Script(`(function(module,exports){${code}\n})`, { filename: file }).runInThisContext()(mod, mod.exports);
const actual = mod.exports;
const inputs = ['', 'ISOLATED', 'isolated', 'cross', 'CROSS', ' isolated', 'isolated ', 'ısolated', 'iſolated', 'İSOLATED', '\0isolated', 'isolated\0'];
for (let mask = 0; mask < 256; mask++) inputs.push(Array.from('isolated', (c, i) => mask & (1 << i) ? c.toUpperCase() : c).join(''));
for (let i = 0; i < 8; i++) for (const ch of [' ', '\n', 'ſ', 'ı', 'İ', 'ß', 'ﬆ', '😀', '_']) inputs.push('isolated'.slice(0, i) + ch + 'isolated'.slice(i + 1));
const mapped = inputs.map(input => ({ input, result: actual.mapMarginType(input) }));
for (const input of [undefined, null, false, true, 0, 1, [], {}, new String('ISOLATED')]) mapped.push({ input: null, result: actual.mapMarginType(input) });
const modes = Object.values(actual.MarginType).map(input => ({ input, result: actual.toOrderlyMarginMode(input) }));
const surrogate = '\ud800ISOLATED';
const boundary = { firstUnit: surrogate.charCodeAt(0), result: actual.mapMarginType(surrogate) };
const positions = [];
for (const symbol of ['', 'BTCUSDC', 'ETHUSDC', 'BTCUSDC:long', 'BTCUSDC:isolated', 'constructor', '__proto__', '日本語😀', ' BTC ']) {
  for (const side of Object.values(actual.PositionSide)) for (const mode of Object.values(actual.PositionMode)) for (const margin of Object.values(actual.MarginType)) {
    positions.push({ symbol, side, mode, margin, result: actual.createOrderlyPositionId(symbol, side, mode, margin) });
  }
}
const surrogateId = actual.createOrderlyPositionId('\ud800', actual.PositionSide.Short, actual.PositionMode.Hedged, actual.MarginType.Isolated);
const surrogateIdUnits = Array.from({ length: surrogateId.length }, (_, i) => surrogateId.charCodeAt(i));
console.log(JSON.stringify({ positions, surrogateIdUnits, runtime: { node: process.version, arch: process.arch }, mapped, modes, boundary }));
