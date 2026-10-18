import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
import {load, root, ts} from './source.mjs';

const filename = path.join(root, 'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source = fs.readFileSync(filename, 'utf8');
const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'MexcExchange');
const names = ['fetchLeverageForSymbol', 'updateSymbolMetadata', 'cancelAllOrders', 'updateOrder'];
const members = names.map(name => {
  const member = owner.members.find(n => ts.isPropertyDeclaration(n) && n.name?.getText(ast) === name);
  assert.ok(member, `maintained member ${name}`);
  return member.getText(ast);
});
const utils = load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types = load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const {DefaultStore} = load('packages/safe-cex/src/store/store.base.ts');
const extractRows = source.slice(source.indexOf('const extractData ='), source.indexOf('\n\ntype MexcSymbolConfig'));
const compiled = ts.transpileModule(`${extractRows}\nclass Holder { ${members.join('\n')} }\nHolder`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const Holder = vm.runInNewContext(compiled, {
  toNumber: utils.toNumber, normalizeMarginType: utils.normalizeMarginType,
  ENDPOINTS: types.ENDPOINTS, REVERSE_MARGIN_TYPE: types.REVERSE_MARGIN_TYPE,
  LogSeverity: {Warning: 'warning'}, OrderStatus: {Canceled: 'canceled'},
});
function fixture(isPublicDataOnly) {
  const store = new DefaultStore();
  store.update({markets: [utils.normalizeMarket({symbol: 'BTC_USDT', contractSize: 1, maxLeverage: 125})]});
  const instance = new Holder();
  Object.assign(instance, {store, isPublicDataOnly, symbolSettings: new Map([['BTCUSDT', {
    buyLeverage: 20, sellLeverage: 20, marginType: 'cross', positionMode: 'hedged',
  }]]), log() {}});
  return instance;
}
const publicInstance = fixture(true);
let notifications = 0;
publicInstance.store.subscribe(() => { notifications++; throw Error('eager subscriber failed'); });
const rejected = publicInstance.updateSymbolMetadata('BTCUSDT');
assert.equal(notifications, 1, 'public metadata prefix');
assert.equal(publicInstance.store.fetchedLeverage.BTCUSDT.buyLeverage, 20);
await assert.rejects(rejected, {message: 'eager subscriber failed'});

const privateInstance = fixture(false);
let release;
let requests = 0;
privateInstance.xhr = {get() {requests++; return new Promise(resolve => {release = resolve;});}};
// Deliberately do not await the returned wrapper Promise.
privateInstance.updateSymbolMetadata('BTCUSDT');
assert.equal(requests, 1, 'private metadata HTTP prefix');
assert.equal(privateInstance.store.fetchedLeverage.BTCUSDT, undefined);
release({data: [{positionType: 1, leverage: 31, maxLeverageView: 80, openType: 2},
  {positionType: 2, leverage: 12, maxLeverage: 60, openType: 1}]});
await new Promise(setImmediate);
assert.equal(privateInstance.store.markets[0].limits.leverage.max, 60);
assert.equal(privateInstance.store.fetchedLeverage.BTCUSDT.buyLeverage, 31);
assert.equal(privateInstance.store.fetchedLeverage.BTCUSDT.sellLeverage, 12);

const unit = fixture(false);
unit.store.update({orders: [{id: 'cancel-me', symbol: 'BTCUSDT', status: 'open'}]});
unit.xhr = {post() {return new Promise(resolve => {release = resolve;});}};
unit.cancelAllOrders();
assert.equal(unit.store.orders[0].status, 'open');
release({});
await new Promise(setImmediate);
assert.equal(unit.store.orders[0].status, 'canceled');

const typed = new Holder();
const phases = [];
Object.assign(typed, {
  cancelOrders() {phases.push('cancel'); return new Promise(resolve => {release = resolve;});},
  placeOrder(order) {phases.push(['place', order.price]); return Promise.resolve(['replacement']);},
});
typed.updateOrder({order: {symbol: 'BTCUSDT', amount: 1}, update: {amount: 1, price: 59100}});
assert.deepEqual(phases, ['cancel'], 'typed wrapper prefix');
release();
await new Promise(setImmediate);
assert.deepEqual(phases, ['cancel', ['place', 59100]], 'detached typed continuation');
process.stdout.write(JSON.stringify({members: names, sourceSha256: createHash('sha256').update(source).digest('hex'), publicNotifications: notifications, detachedPrivate: true, detachedUnit: true, detachedTyped: true}));
