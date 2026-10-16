const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const venue = path.join(root, 'packages/safe-cex/src/exchanges/apexomni');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file).exports;
  let source = fs.readFileSync(file, 'utf8');
  if (process.env.APEXOMNI_MUTATE_TS === 'protocol' && file === path.join(venue, 'apexomni.types.ts')) {
    assert(source.includes('20_000'), 'protocol mutation must apply');
    source = source.replace('20_000', '20_001');
  }
  if (process.env.APEXOMNI_MUTATE_TS === 'ws-add' && file === path.join(venue, 'apexomni.types.ts')) {
    assert(source.includes('linear: {'), 'WebSocket leaf mutation must apply');
    source = source.replace('linear: {', "linear: { added: 'wss://new.example',");
  }
  const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS }, reportDiagnostics: true });
  assert(!compiled.diagnostics.some(item => item.category === ts.DiagnosticCategory.Error), `${file}: TypeScript transpilation failed`);
  const module = { exports: {} }; cache.set(file, module);
  new vm.Script(`(function(require,module,exports){${compiled.outputText}\n})`, { filename: file }).runInNewContext()(
    specifier => {
      if (specifier === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
      assert(specifier.startsWith('.'), `unexpected oracle dependency ${specifier}`);
      const resolved = path.resolve(path.dirname(file), `${specifier}.ts`);
      assert(resolved.startsWith(path.join(root, 'packages/safe-cex/src/')), 'oracle import must stay in reference source');
      return load(resolved);
    }, module, module.exports);
  return module.exports;
}
const tables = load(path.join(venue, 'apexomni.types.ts'));
function leaves(value, prefix = []) {
  if (typeof value === 'string') return [[prefix, value]];
  assert(value && typeof value === 'object' && !Array.isArray(value), 'WebSocket URL branch must be a nonempty object or string');
  const entries = Object.entries(value);
  assert(entries.length, 'empty WebSocket URL branch requires classification');
  return entries.flatMap(([key, child]) => leaves(child, [...prefix, key]));
}
for (const [name, value] of Object.entries(tables)) {
  if (name === 'BASE_WS_URL') tables[name] = leaves(value);
  else if (name !== 'ORDER_CANCEL_REASON' && value && typeof value === 'object' && !Array.isArray(value)) tables[name] = Object.entries(value);
}
const cancelReasons = tables.ORDER_CANCEL_REASON;
tables.ORDER_CANCEL_REASON = Object.values(tables.ORDER_CANCEL_REASON);
const index = load(path.join(venue, 'interface/apexomni.index.interface.ts'));
// Type aliases are erased at runtime: inspect the actual declaration separately.
const indexFile = path.join(venue, 'interface/apexomni.index.interface.ts');
const options = { strict: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
const host = ts.createCompilerHost(options);
const readFile = host.readFile.bind(host);
host.readFile = file => {
  let source = readFile(file);
  if (file === indexFile && process.env.APEXOMNI_MUTATE_TS === 'market-type') {
    assert(source.includes('export type Market = ApexMarket;'), 'Market type mutation must apply');
    source = source.replace('export type Market = ApexMarket;', 'export type Market = OrderType;');
  }
  return source;
};
const program = ts.createProgram([indexFile], options, host);
const errors = ts.getPreEmitDiagnostics(program).filter(d => d.category === ts.DiagnosticCategory.Error);
assert.equal(errors.length, 0, errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')).join('\n'));
const source = program.getSourceFile(indexFile);
const aliases = source.statements.filter(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'Market');
const enums = source.statements.filter(n => ts.isEnumDeclaration(n) && n.name.text === 'ApexMarket');
assert.equal(aliases.length, 1, 'Market type declaration census');
assert.equal(enums.length, 1, 'ApexMarket declaration census');
const checker = program.getTypeChecker();
const typeIdentity = checker.getTypeAtLocation(aliases[0]) === checker.getTypeAtLocation(enums[0]);
process.stdout.write(JSON.stringify({ marketAlias: { typeIdentity, identity: index.Market === index.ApexMarket, members: Object.entries(index.Market) }, variants: { ORDER_CANCEL_REASON: cancelReasons, ...Object.fromEntries(['ApexMarket', 'OrderSide', 'OrderType', 'TimeInForce', 'OrderStatus'].map(name => [name, index[name]])) }, tables, unions: Object.fromEntries(['ApexMarket', 'OrderSide', 'OrderType', 'TimeInForce', 'OrderStatus'].map(name => [name, Object.values(index[name])])) }));
