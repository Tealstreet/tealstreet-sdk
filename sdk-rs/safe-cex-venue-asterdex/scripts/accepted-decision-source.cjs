// Independently authored cases checked against actual maintained TS bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');
const printer = ts.createPrinter();
function selectedClass(file, names, extra = '') {
  const ast = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
  const owner = ast.statements.find(ts.isClassDeclaration);
  assert.ok(owner);
  const members = names.map(name => {
    const node = owner.members.find(item => item.name?.getText(ast) === name);
    assert.ok(node, `${file}: ${name}`);
    return printer.printNode(ts.EmitHint.Unspecified, node, ast);
  });
  return { ast, code: `${extra}\nexport class Oracle { ${members.join('\n')} }` };
}
const sharedFile = 'packages/safe-cex/src/types.ts';
const shared = ts.createSourceFile(sharedFile, fs.readFileSync(path.join(root, sharedFile), 'utf8'), ts.ScriptTarget.Latest, true);
const enums = shared.statements.filter(ts.isEnumDeclaration)
  .map(node => printer.printNode(ts.EmitHint.Unspecified, node, shared)).join('\n');
const exchange = selectedClass('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts',
  ['isAlgoOrder', 'reverseMapTimeInForce', 'fetchPositionMode', 'fetchSymbolMaxLeverage']);
const unknown = exchange.ast.statements.find(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(item => item.name.getText(exchange.ast) === 'isAsterDEXUnknownOrderError'));
assert.ok(unknown);
const code = `${enums}\n${printer.printNode(ts.EmitHint.Unspecified, unknown, exchange.ast)}\n${exchange.code}\nexport {isAsterDEXUnknownOrderError};`;
function compile(source, dependencies = {}) {
  const exports = {};
  const compiled = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  new Function('exports', ...Object.keys(dependencies), compiled)(exports, ...Object.values(dependencies));
  return exports;
}
const exchangeExports = compile(code, { LogSeverity: { Error: 'Error' } });
const ex = new exchangeExports.Oracle();
// Transport/logging are fixture boundaries for the two pure Rust projections.
ex.getFapiPath = endpoint => endpoint;
ex.log = () => {};
let clockCalls = 0;
let clockNow = 1234;
const account = selectedClass('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts',
  ['parseFiniteNumber', 'getOrderUpdateTime', 'getPositionSide'], enums);
const accountExports = compile(account.code, {
  virtualClock: { getCurrentTimeMs: () => { clockCalls++; return clockNow; } },
});
const ws = new accountExports.Oracle();
const bits = number => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(number);
  return bytes.toString('hex');
};
const numeric = value => typeof value === 'string' ? Number(value) : value;
const adapters = {
  'account_helpers::parse_finite_number': input => {
    const number = ws.parseFiniteNumber(input.value);
    return number === undefined ? null : bits(number);
  },
  'account_helpers::get_order_update_time': input => {
    clockCalls = 0;
    clockNow = input.now;
    try { return { bits: bits(ws.getOrderUpdateTime(input.data)), clockCalls }; }
    catch (error) { return { error: error.name, clockCalls }; }
  },
  'account_helpers::get_position_side': input => {
    try { return { side: ws.getPositionSide(input.position, numeric(input.amount)) }; }
    catch (error) { return { error: error.name }; }
  },
  'exchange_helpers::is_aster_dex_unknown_order_error': input => {
    try {
      const value = Object.hasOwn(input, 'instanceMessage') ? new Error(input.instanceMessage) : input.error;
      return { value: exchangeExports.isAsterDEXUnknownOrderError(value) };
    } catch (error) { return { error: error.name }; }
  },
  'exchange_helpers::is_algo_order': input => ex.isAlgoOrder(input),
  'exchange_helpers::reverse_map_time_in_force': input => ex.reverseMapTimeInForce(input),
  'exchange_helpers::parse_position_mode_response': async input => {
    ex.primaryAPI = { get: async () => ({ data: input.data }) };
    try { return { value: await ex.fetchPositionMode() }; }
    catch (error) { return { error: error.name, message: error.message }; }
  },
  'exchange_helpers::fetch_symbol_max_leverage': async input => {
    ex.fetchMarkets = async () => input.markets.map(([symbol, max]) => ({
      symbol, limits: { leverage: { max: max === null ? undefined : numeric(max) } },
    }));
    try { return { bits: bits(await ex.fetchSymbolMaxLeverage(input.symbol)) }; }
    catch (error) { return { error: error.message }; }
  },
};
async function main() {
  assert.equal(process.argv.length, 2, 'decision source gate takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  assert.equal(Object.keys(adapters).length, 8);
  let count = 0;
  for (const [name, observe] of Object.entries(adapters)) {
    const owner = `safe_cex_venue_asterdex::${name}`;
    assert.equal(contracts[owner]?.category, 'decoder', owner);
    for (const row of contracts[owner].cases) {
      assert.deepEqual(await observe(structuredClone(row.input)), row.output,
        `${owner}: actual TS output for ${JSON.stringify(row.input)}`);
      count++;
    }
  }
  assert.equal(count, 98);
  console.log(`Aster decision actual source: 8 owners / ${count} expectations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
