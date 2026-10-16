const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts'), 'utf8');
const tree = ts.createSourceFile('polymarket.exchange.ts', source, ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'PolymarketExchange');
const names = ['name', 'getPositionMode', 'fetchSymbolMaxLeverage', 'setLeverage', 'changePositionMode', 'changeMarginType'];
const bodies = names.map(name => {
 const nodes = cls.members.filter(node => node.name?.getText(tree) === name);
 assert.equal(nodes.length, 1, `source_member:${name}`);
 return source.slice(nodes[0].getFullStart(), nodes[0].end);
});
// Execute the actual enum modules rather than hand-spelling their values.
const modules = new Map();
for (const filename of ['packages/logging/src/LogSeverity.ts', 'packages/safe-cex/src/types.ts']) {
 const text = fs.readFileSync(path.join(root, filename), 'utf8');
 const exports = {};
 const requireSource = id => {
  assert.equal(id, '@tealstreet/logging/src/LogSeverity', 'source_enum_import');
  assert(modules.has('packages/logging/src/LogSeverity.ts'), 'source_logging_loaded');
  return modules.get('packages/logging/src/LogSeverity.ts');
 };
 vm.runInNewContext('(function(require,exports){' + ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText + '\n})', {})(requireSource, exports);
 modules.set(filename, exports);
}
const context = { exports: {}, PositionMode: modules.get('packages/safe-cex/src/types.ts').PositionMode, LogSeverity: modules.get('packages/logging/src/LogSeverity.ts').LogSeverity };
vm.runInNewContext(ts.transpileModule('class PolymarketExchange {\n' + bodies.join('\n') + '\n}\nexports.Exchange=PolymarketExchange;', { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, context);
const inputs = [
 { symbol: '', leverage: 0, flag: false },
 { symbol: 'token-1', leverage: 12.5, flag: true },
 { symbol: '🦀 λ', leverage: -0, flag: false },
 { symbol: 'unknown/token', leverage: Infinity, flag: true },
 { symbol: 'same', leverage: NaN, flag: false },
];
function bits(value) { const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex'); }
async function invoke(exchange, name, input) {
 switch (name) {
  case 'name': return exchange.name;
  case 'getPositionMode': return exchange.getPositionMode();
  case 'fetchSymbolMaxLeverage': return exchange.fetchSymbolMaxLeverage(input.symbol);
  case 'setLeverage': return exchange.setLeverage(input.symbol, input.leverage);
  case 'changePositionMode': return exchange.changePositionMode(input.flag);
  case 'changeMarginType': return exchange.changeMarginType(input.flag);
  default: assert.fail('unconfigured source method');
 }
}
(async () => {
 const cases = [], failures = [];
 for (const name of names) {
  for (let iteration = 0; iteration < inputs.length; iteration++) {
   const input = inputs[iteration], calls = [], instance = new context.exports.Exchange();
   instance.log = (...args) => { calls.push(args); };
   const pending = invoke(instance, name, input);
   const immediateCalls = calls.length;
   const value = await pending;
   cases.push({ name, iteration, input: { symbol: input.symbol, leverageBits: bits(input.leverage), flag: input.flag }, type: typeof value, value: typeof value === 'number' ? null : value ?? null, bits: typeof value === 'number' ? bits(value) : null, immediateCalls, calls });
  }
 }
 for (const name of names.slice(3)) {
  const instance = new context.exports.Exchange(), calls = [];
  const error = { name: 'LogFailure', message: 'log listener threw', context: { marker: 7 } };
  instance.log = (...args) => { calls.push(args); throw error; };
  const pending = invoke(instance, name, inputs[1]);
  const immediateCalls = calls.length;
  try { await pending; assert.fail(`source_rejection_required:${name}`); }
  catch (actual) { assert.strictEqual(actual, error, `source_error_identity:${name}`); failures.push({ name, preserved: true, error: actual, immediateCalls, calls }); }
 }
 process.stdout.write(JSON.stringify({ capture: { architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]) }, cases, failures }));
})().catch(error => { console.error(error); process.exitCode = 1; });
