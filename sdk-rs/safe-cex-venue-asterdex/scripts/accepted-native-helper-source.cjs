// Independent expectations also checked against maintained source bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');
const printer = ts.createPrinter();
function compile(source, bindings = {}) {
  const exports = {};
  new Function('exports', ...Object.keys(bindings), ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText)(exports, ...Object.values(bindings));
  return exports;
}
function selectedClass(relative, names, bindings = {}) {
  const ast = ts.createSourceFile(relative, fs.readFileSync(path.join(root, relative), 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(ts.isClassDeclaration);
  assert.ok(cls, relative);
  const members = names.map(name => {
    const member = cls.members.find(node => node.name?.getText(ast) === name);
    assert.ok(member, name);
    return printer.printNode(ts.EmitHint.Unspecified, member, ast);
  });
  return new (compile(`export class Oracle { ${members.join('\n')} }`, bindings).Oracle)();
}
const exchange = selectedClass('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts',
  ['fetchBalance', 'fetchOrderBook', 'collectOpenInterestFromPayload'], { LogSeverity: { Error: 'error' } });
exchange.log = () => {};
exchange.getAccountEndpoint = () => '/account';
exchange.getFapiPath = name => name;
const orderBook = compile(fs.readFileSync(path.join(root, 'packages/safe-cex/src/utils/orderbook.ts'), 'utf8'));
const socket = selectedClass('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts',
  ['handleDepthSnapshot'], { calcOrderBookTotal: orderBook.calcOrderBookTotal });
const bits = value => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
function bookFields(book) {
  return Object.fromEntries(['bids', 'asks'].flatMap(side => ['price', 'amount', 'total'].map(field => [
    `${side === 'bids' ? 'bid' : 'ask'}${field[0].toUpperCase()}${field.slice(1)}s`,
    book[side].map(level => bits(level[field])),
  ])));
}
const observe = {
  'exchange_helpers::balance_from_account_response': async input => {
    exchange.primaryAPI = { get: async () => ({ data: input }) };
    try {
      const balance = await exchange.fetchBalance();
      return { ok: true, ...Object.fromEntries(['total', 'free', 'used', 'upnl'].map(field => [`${field}Bits`, bits(balance[field])])) };
    } catch (error) { return { ok: false, error: error.name }; }
  },
  'exchange_helpers::collect_open_interest_from_payload': input => {
    const map = new Map(input.seed);
    let error = null;
    try { exchange.collectOpenInterestFromPayload(input.payload, map); }
    catch (thrown) { error = thrown.name; }
    return { ok: error === null, symbols: [...map.keys()], valueBits: [...map.values()].map(bits), error };
  },
  'exchange_helpers::transform_order_book': async input => {
    exchange.publicAPI = { get: async () => ({ data: input }) };
    return { ok: true, ...bookFields(await exchange.fetchOrderBook('BTCUSDT')) };
  },
  'ws_helpers::public_depth_snapshot': input => {
    let result;
    socket.parent = { store: { tickersBySymbol: {} } };
    socket.orderBookSubscriptions = new Map([['btcusdt@depth20@100ms', { callbacks: new Map([[0, book => { result = book; }]]) }]]);
    // The pure Rust projection models whether a book is accepted; caller effects
    // and invalid-frame exceptions are covered separately by live_ws_helpers.
    try { socket.handleDepthSnapshot(input === null ? null : { ...input, s: 'BTCUSDT' }); }
    catch (error) { assert.ok(error instanceof TypeError); }
    return result ? { ok: true, ...bookFields(result) } : { ok: false };
  },
};
async function main() {
  assert.equal(process.argv.length, 2, 'native helper source gate takes no arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  let count = 0;
  for (const [name, actual] of Object.entries(observe)) {
    const owner = `safe_cex_venue_asterdex::${name}`;
    const cases = contracts[owner].nativeObservation.cases;
    for (const row of cases) {
      assert.deepEqual(await actual(structuredClone(row.input)), row.output, `${owner}: actual TS ${JSON.stringify(row.input)}`);
      count++;
    }
  }
  assert.equal(count, 20);
  console.log('Aster native-helper expectations: 4 maintained-source owners / 20 cases PASS.');
}
main().catch(error => { console.error(error); process.exitCode = 1; });
