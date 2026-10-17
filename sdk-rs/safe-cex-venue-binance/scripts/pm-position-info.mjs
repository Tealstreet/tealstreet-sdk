import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { sourceStore } from '../../safe-cex-store/tests/store_source.mjs';
import { ts } from '../../safe-cex-store/tests/source_loader.mjs';

const context = sourceStore();
const read = file => ts.createSourceFile(file,
  fs.readFileSync(new URL(`../../../packages/safe-cex/src/${file}`, import.meta.url), 'utf8'),
  ts.ScriptTarget.Latest, true);
const execute = code => vm.runInContext(ts.transpileModule(code.replace(/\bexport /g, ''), {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText, context);
const types = read('types.ts');
execute(['OrderStatus', 'OrderSide', 'OrderType', 'PositionSide'].map(name => {
  const item = types.statements.find(node => ts.isEnumDeclaration(node) && node.name.text === name);
  assert.ok(item, name);
  return item.getText(types);
}).join('\n'));
const stub = read('store/store.stub.ts');
execute(stub.statements.filter(node => !ts.isImportDeclaration(node))
  .map(node => node.getText(stub)).join('\n').replace(/\bexport /g, '')
  + '\nglobalThis.positionSeed = stubPosition;');
const source = read('exchanges/binance/binancepm.ws-private.ts');
const owner = source.statements.find(node => ts.isClassDeclaration(node)
  && node.name?.text === 'BinancePMPrivateWebsocket');
const method = owner.members.find(node => node.name?.getText(source) === 'handleAccountEvents');
assert.ok(method, 'maintained PM account handler');
execute(`globalThis.Subject = class { ${method.getText(source)} };`);
const store = new context.Store();
store.update({ positions: [{ ...context.positionSeed, id: 'BTCUSDTshort', symbol: 'BTCUSDT',
  side: 'short', entryPrice: 100, contracts: 3, notional: 300, leverage: 10,
  liquidationPrice: 200, info: { previous: true } }] });
const socket = new context.Subject();
socket.parent = { store };
socket.store = store;
socket.throttledFetchPositions = () => {};
for (let index = 0; index < 16; index++) {
  const amount = index % 2 === 0 ? 2 : -3;
  const entry = 110 + index;
  const upnl = 7 - index;
  socket.handleAccountEvents([{ a: { P: [{ s: 'BTCUSDT', pa: String(amount),
    ep: String(entry), up: String(upnl), ps: 'BOTH' }], B: [] } }]);
  const position = store.positions[0];
  assert.equal(position.info, position, 'source retains one constant identity cycle');
  assert.equal(position.info.entryPrice, entry, 'info reads this frame, not the previous frame');
  assert.equal(position.info.contracts, Math.abs(amount));
  assert.equal(position.info.notional, Math.abs(amount) * entry);
  assert.equal(position.info.unrealizedPnl, upnl);
  assert.equal(position.info.side, amount > 0 ? 'long' : 'short');
  assert.equal(position.info.leverage, 10);
  assert.equal(position.info.liquidationPrice, 200);
}
console.log('PASS: maintained PM handler + actual TS Store, 16 frames; info aliases the updated position; native depth-one snapshot intentionally differs in identity');
