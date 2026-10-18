// Fixed expectations are authored independently in the selected contract rows.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');
const load = require('./load-module.cjs');
const printer = ts.createPrinter();
const ast = relative => ts.createSourceFile(relative,
  fs.readFileSync(path.join(root, relative), 'utf8'), ts.ScriptTarget.Latest, true);
const shared = ast('packages/safe-cex/src/types.ts');
const enumSource = shared.statements.filter(ts.isEnumDeclaration)
  .map(n => printer.printNode(ts.EmitHint.Unspecified, n, shared)).join('\n');
function compile(source, bindings = {}) {
  const exports = {};
  new Function('exports', ...Object.keys(bindings), ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText)(exports, ...Object.values(bindings));
  return exports;
}
const enums = compile(enumSource);
const compilerRoot = path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH
  || path.join(root, 'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript'))), '..');
assert.equal(require(path.join(compilerRoot, 'package.json')).name, 'typescript');
const ethers = require(path.join(path.dirname(compilerRoot), 'ethers'));
const unavailable = new Proxy(function () { throw Error('Unselected dependency invoked'); }, {
  get(_, key) {
    if (key === '__esModule') return false;
    return () => { throw Error(`Unselected dependency invoked: ${String(key)}`); };
  },
});
const api = require('./api-source.cjs');
const fee = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts', [],
  { ethers, '../../types': enums });
const approval = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts', [], {
  ethers: unavailable, '../../transport/registry': unavailable, './asterdex.api': api,
  './asterdex.builder-fee': fee, './asterdex.types': values,
});
const exchange = ast('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const exchangeClass = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
assert.ok(exchangeClass);
const names = ['fetchStpMode', 'changeStpMode', 'changeMarginType', 'getFapiPath',
  'getAccount', 'fetchOrderBook', 'fetchBalance', 'getAccountEndpoint',
  'reverseMapOrderStatus', 'reverseMapOrderType', 'extractFilterValue', 'collectOpenInterestFromPayload'];
const members = names.map(name => {
  const node = exchangeClass.members.find(n => n.name?.getText(exchange) === name);
  assert.ok(node, name);
  return printer.printNode(ts.EmitHint.Unspecified, node, exchange);
});
const { Oracle } = compile(`${enumSource}\nexport class Oracle { ${members.join('\n')} }`, {
  getAsterDEXFapiPath: values.getAsterDEXFapiPath,
  isAsterDEXStpMode: values.isAsterDEXStpMode,
  LogSeverity: { Info: 'info', Error: 'error' },
});
const ex = new Oracle();
ex.log = () => {};
ex.emitAndLogError = () => {};
// Caller version gates and transport are outside the pure response/request owners.
ex.assertV3Feature = () => {};
const account = ast('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const accountClass = account.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXPrivateWebsocket');
const mirror = accountClass.members.find(n => n.name?.getText(account) === 'mirrorCumulativeOrderFill');
assert.ok(mirror);
const initializers = new Map();
for (const statement of mirror.body.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    const name = declaration.name.getText(account);
    if (name === 'filled' || name === 'amount') {
      assert.ok(declaration.initializer);
      const expression = printer.printNode(ts.EmitHint.Expression, declaration.initializer, account);
      initializers.set(name === 'filled' ? 'z' : 'q', compile(`export const observe = (orderData:any) => (${expression});`).observe);
    }
  }
}
assert.equal(initializers.size, 2);
const publicSource = ast('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts');
const publicClass = publicSource.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXPublicWebsocket');
const connection = publicClass.members.find(n => ts.isGetAccessorDeclaration(n) && n.name.getText(publicSource) === 'connectionURL');
assert.ok(connection);
let requestId;
const Public = compile(`export class Oracle { ${printer.printNode(ts.EmitHint.Unspecified, connection, publicSource)} }`, {
  ASTERDEX_ENDPOINTS: values.ASTERDEX_ENDPOINTS, nanoid: () => requestId,
}).Oracle;
const bits = number => {
  if (Number.isNaN(number)) return 'NaN';
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(number);
  return bytes.toString('hex');
};
const result = fn => {
  try { return { value: fn() ?? null }; }
  catch (error) { return { error: error.message }; }
};
const decision = async fn => {
  try { return { value: await fn() }; }
  catch (error) { return { error: error.name, message: error.message }; }
};
const numberResult = fn => {
  try { return { bits: bits(fn()) }; }
  catch (error) { return { error: error.name }; }
};
function nonce(input, fn) {
  return input.steps.map(([key, time]) => {
    api.setOracleClock(time === 'inf' ? Infinity : time === '-inf' ? -Infinity : Number(time));
    try { return { value: fn(`projection:${input.scope}:${key}`) }; }
    catch (error) { return { error: error.name }; }
  });
}
function lookup(value) {
  if (typeof value !== 'function' && value !== Object.prototype) return { mapped: value };
  const key = Object.getOwnPropertyNames(Object.prototype).find(key => ({})[key] === value);
  assert.ok(key, 'actual inherited referent must match a stock Object.prototype member');
  const inherited = key === '__proto__' ? 'Prototype' : key.replace(/^__|__$/g, '').replace(/^./, c => c.toUpperCase());
  return { inherited, type: typeof value };
}
const adapters = {
  'api_helpers::build_v3_nonce': input => nonce(input, api.buildV3Nonce),
  'api_helpers::build_aster_dex_v3_nonce': input => nonce(input, api.buildAsterDEXV3Nonce),
  'builder::build_aster_approve_builder_typed_data': input => approval.buildAsterApproveBuilderTypedData(input),
  'builder::is_aster_maker_only_order': input => fee.isAsterMakerOnlyOrder(input.opts, input.apiOrderType),
  'builder::get_aster_builder_fee_rate_for_order': input => fee.getAsterBuilderFeeRateForOrder(input.opts, input.apiOrderType, input.config) ?? null,
  'builder::resolve_aster_builder_fee_config': input => result(() => fee.resolveAsterBuilderFeeConfig(input)),
  'builder::get_aster_builder_approval_config': input => result(() => approval.getAsterBuilderApprovalConfig(input)),
  'exchange_helpers::parse_stp_mode_response': async input => {
    ex.apiVersion = 'v3';
    ex.primaryAPI = { get: async () => ({ data: input.data }) };
    const output = await decision(() => ex.fetchStpMode());
    if (Object.hasOwn(output, 'value')) output.value = {
      variant: ({ EXPIRE_TAKER: 'V0', EXPIRE_MAKER: 'V1', EXPIRE_BOTH: 'V2' })[output.value], wire: output.value,
    };
    return output;
  },
  'exchange_helpers::build_change_stp_mode_request': async input => {
    ex.apiVersion = 'v3';
    let effect;
    ex.primaryAPI = { post: async (path, body) => { effect = { method: 'POST', path, body }; } };
    const output = await decision(() => ex.changeStpMode(input.mode));
    return Object.hasOwn(output, 'value') ? { value: effect } : output;
  },
  'exchange_helpers::build_change_margin_type_request': async input => {
    ex.apiVersion = input.version;
    let effect;
    ex.primaryAPI = { post: async (path, body) => { effect = { method: 'POST', path, body }; } };
    const output = await decision(() => ex.changeMarginType(input.isolated, input.symbol));
    return Object.hasOwn(output, 'value') ? { value: effect } : output;
  },
  'exchange_helpers::get_fapi_path': input => {
    ex.apiVersion = input.version;
    return ex.getFapiPath(input.endpoint, input.override);
  },
  'ws_helpers::public_connection_url': input => {
    requestId = input.id;
    const socket = new Public();
    socket.parent = { options: { testnet: input.testnet } };
    return socket.connectionURL;
  },
  'exchange_helpers::get_account_from_alias': async input => {
    ex.options = { key: input.key };
    ex.apiVersion = 'v3';
    ex.primaryAPI = { get: async () => ({ data: { accountAlias: input.alias } }) };
    const account = await ex.getAccount();
    return { userId: account.userId, subId: account.subId ?? null, affiliateId: account.affiliateId ?? null };
  },
  'exchange_helpers::transform_order_book': async input => {
    ex.apiVersion = 'v3';
    ex.publicAPI = { get: async () => ({ data: input }) };
    const book = await ex.fetchOrderBook('BTCUSDT');
    const levels = rows => rows.map(row => ({ price: bits(row.price), amount: bits(row.amount), total: bits(row.total) }));
    return { bids: levels(book.bids), asks: levels(book.asks) };
  },
  'exchange_helpers::balance_from_account_response': async input => {
    ex.apiVersion = 'v3';
    ex.primaryAPI = { get: async () => ({ data: input }) };
    try {
      const balance = await ex.fetchBalance();
      return { value: {
        total: bits(balance.total), free: bits(balance.free), used: bits(balance.used), upnl: bits(balance.upnl),
        info: Object.hasOwn(balance, 'info'), currencies: Object.hasOwn(balance, 'currencies'), coins: Object.hasOwn(balance, 'coins'),
      } };
    } catch (error) { return { error: error.name }; }
  },
  'exchange_helpers::reverse_map_order_status': input => lookup(ex.reverseMapOrderStatus(input)),
  'exchange_helpers::reverse_map_order_type': input => lookup(ex.reverseMapOrderType(input)),
  'exchange_helpers::extract_filter_value': input => numberResult(() => ex.extractFilterValue(input.filters, input.type, input.field)),
  'exchange_helpers::collect_open_interest_from_payload': input => {
    const map = new Map(input.seed);
    let error = null;
    try { ex.collectOpenInterestFromPayload(input.payload, map); }
    catch (thrown) { error = thrown.name; }
    return { entries: [...map].map(([key, number]) => [key, bits(number)]), error };
  },
  'account_helpers::parse_order_number': input => {
    assert.ok(initializers.has(input.field), 'bounded actual z/q source initializer');
    return numberResult(() => initializers.get(input.field)(input.data));
  },
};
async function main() {
  assert.equal(process.argv.length, 2, 'projection source gate takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  assert.equal(Object.keys(adapters).length, 20);
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
  assert.equal(count, 274);
  console.log(`Aster projection actual source: 20 owners / ${count} expectations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
