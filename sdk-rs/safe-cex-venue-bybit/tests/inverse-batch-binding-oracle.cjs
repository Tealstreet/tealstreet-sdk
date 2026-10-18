// Maintained class bodies run together; scripted I/O supplies the same envelopes
// as the native physical transport. No batch or request-format policy is copied.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = file => fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
const compilerOptions = {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS};
function moduleFrom(file, dependencies = {}) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(file), {compilerOptions}).outputText, {
    exports, require(name) {
      assert(Object.hasOwn(dependencies, name), `${file}: ${name}`);
      return dependencies[name];
    },
  });
  return exports;
}
const ast = file => ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true);
const variables = tree => tree.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]);
const exchange = ast('exchanges/bybit/bybitinverse.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitInverseExchange');
function member(name) {
  const nodes = cls.members.filter(n => n.name?.getText(exchange) === name);
  assert.equal(nodes.length, 1, name);
  return nodes[0].getText(exchange);
}
function declaration(tree, name) {
  const node = variables(tree).find(n => n.name.getText(tree) === name);
  assert(node, name);
  return `const ${node.getText(tree)};`;
}
const typeTree = ast('types.ts');
const enumSource = typeTree.statements.filter(ts.isEnumDeclaration).map(n => n.getText(typeTree).replace(/^export\s+/, '')).join('\n');
const tables = ast('exchanges/bybit/bybit.types.ts');
const source = enumSource + '\n' + [
  'getBybitTriggerBy', 'getBybitTriggerDirectionPrice',
].map(name => declaration(exchange, name)).join('\n') + '\n' + [
  'ENDPOINTS', 'ORDER_SIDE', 'ORDER_TYPE', 'ORDER_TIME_IN_FORCE_OUTBOUND',
].map(name => declaration(tables, name)).join('\n') + '\n' +
  `globalThis.Subject = class {${[
    'placeOrders', 'placeBatchOrders', 'placeBatchOrdersForCategory', 'processBatch',
    'formatOrderForBatch', 'getOrderExecutionParams', 'getOrderPositionIdx',
    'getCategoryForSymbol', 'batchLimit', 'remainingInStoreUnits',
    'recordRecentlyPlacedOrderFromOpts', 'recordRecentlyPlacedOrder',
  ].map(member).join('\n')}};`;
const enumContext = vm.createContext({});
vm.runInContext(ts.transpileModule(enumSource, {compilerOptions}).outputText, enumContext);
const enums = vm.runInContext('({OrderType,OrderTimeInForce})', enumContext);
const params = moduleFrom('exchanges/bybit/bybit.order-params.ts', {'../../types':enums});
const {inverseObj} = moduleFrom('utils/inverseObj.ts');
const {omitUndefined} = moduleFrom('utils/omitUndefined.ts', {'lodash/omitBy':{__esModule:true,default:require('lodash/omitBy')}});
const {adjust} = moduleFrom('utils/safe-math.ts', {'@tealstreet/math/src/isNotNumber': moduleFrom('../../math/src/isNotNumber.ts')});
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
assert.equal(cases.length, 36, 'independent source batch case floor');
(async () => {
  const outputs = [];
  for (const fixture of cases) {
    const requests = [], errors = [], sleeps = [], recorded = [], warnings = [];
    const context = vm.createContext({
      ...params, inverseObj, omitUndefined, adjust,
      console:{warn:message=>warnings.push(message)},
      virtualClock:{getCurrentTimeMs:()=>1700000000000},
      chunk:require('lodash/chunk'), sleep:async ms => {sleeps.push(ms)},
    });
    vm.runInContext(ts.transpileModule(source, {compilerOptions}).outputText, context);
    const target = new context.Subject();
    target.name = 'bybitinverse';
    target.options = {enableInverseMarkets:true};
    target.store = {
      markets:fixture.markets,
      marketsBySymbol:Object.fromEntries(fixture.markets.map(m => [m.symbol, m])),
      tickersBySymbol:Object.fromEntries(fixture.markets.map(m => [m.symbol, {last:100,mark:101,index:102}])),
    };
    target.assertCallerClientOrderIds = () => {};
    target._hasFullSizedPositionOppositeSide = () => false;
    target.fetchPositionMode = async () => fixture.mode;
    target.assertOptionOrderPlaceable = () => {throw Error('unexpected option in regular batch corpus')};
    target.placeOrder = async () => {throw Error('unexpected single placement in regular batch corpus')};
    target.emitAndLogError = value => errors.push(value);
    target.recentlyPlacedOrders = new Map();
    target.shouldUseLocalRestOrderLifecycleFallback = () => false;
    target.xhr = {post:async (endpoint, payload) => {
      requests.push({endpoint,payload});
      assert(fixture.replies.length > 0, 'batch reply supplied');
      return {data:fixture.replies[Math.min(requests.length - 1, fixture.replies.length - 1)]};
    }};
    const orders = fixture.orders.map(order => fixture.numericAmount === undefined ? order : {...order, amount:Number(fixture.numericAmount)});
    let result;
    try { result = {ids:await target.placeOrders(orders)}; }
    catch (error) {result = {error:error.message};}
    for (const {order} of target.recentlyPlacedOrders.values()) {
      recorded.push([order.id,order.symbol,String(order.amount),String(order.remaining),
        String(order.extra.remainingNative),String(order.extra.remainingNotional)]);
    }
    outputs.push({...result,requests,errors,sleeps,recorded,warnings});
  }
  assert.equal(outputs.length, 36);
  process.stdout.write(JSON.stringify(outputs));
})().catch(error => {console.error(error);process.exitCode=1});
