import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';

if (process.argv.includes('--handler-registry')) {
  function declarations(file, className, names) {
    const source = fs.readFileSync(new URL('../../../packages/safe-cex/src/exchanges/'+file, import.meta.url), 'utf8');
    const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
    assert.equal(tree.parseDiagnostics.length, 0);
    const owner = tree.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === className);
    return names.map(name => {
      const member = owner.members.find(node => node.name?.getText(tree) === name);
      assert.ok(member, name);
      return member.getText(tree);
    }).join('\n');
  }
  const members = declarations('base.ws.ts', 'BaseWebSocket', [
    'recordMessageHandlerStats', 'reconcileMessageHandlerStats', 'cleanupMessageHandlerStats', 'handlePongEvent',
  ]);
  let now = 1700000000000;
  const context = {Date:{now:()=>now}, setTimeout:()=>1, clearTimeout:()=>{}};
  // Execute the maintained threshold declarations, rather than supplying new values.
  const source = fs.readFileSync(new URL('../../../packages/safe-cex/src/exchanges/base.ws.ts', import.meta.url), 'utf8');
  const tree = ts.createSourceFile('base.ws.ts', source, ts.ScriptTarget.Latest, true);
  const thresholds = tree.statements.filter(node => ts.isVariableStatement(node) &&
    node.declarationList.declarations.some(decl => ['STATS_RECONCILE_INTERVAL_MS','STATS_RECONCILE_FORCE_MIN_MS'].includes(decl.name.getText(tree))));
  assert.equal(thresholds.length, 2);
  const body = thresholds.map(node=>node.getText(tree)).join('\n') + '\nclass Base {'+members+'} globalThis.Base=Base;';
  vm.runInNewContext(ts.transpileModule(body, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
  const make = () => Object.assign(new context.Base(), {
    messageHandlers:{}, messageHandlersStats:{}, lastStatsReconcileAt:0, shouldEnforcePongMissReconnect:()=>false,
  });
  const socket = make();
  socket.messageHandlers = {book:[()=>{},()=>{}], pong:[()=>{}]};
  socket.recordMessageHandlerStats('book');
  socket.handlePongEvent();
  assert.equal(socket.messageHandlersStats.book.lastCallbacksCount, 2);
  assert.equal(socket.messageHandlersStats.pong.hadHandlers, true);
  socket.messageHandlers.book = [];
  now += 6000;
  socket.recordMessageHandlerStats('pong');
  assert.equal(socket.messageHandlersStats.book.lastCallbacksCount, 0);
  socket.messageHandlers.book = [()=>{}];
  socket.recordMessageHandlerStats('book');
  delete socket.messageHandlers.pong;
  now += 300;
  socket.cleanupMessageHandlerStats('book');
  assert.equal(Object.hasOwn(socket.messageHandlersStats, 'book'), false);
  assert.equal(socket.messageHandlersStats.pong.lastCallbacksCount, 0);
  const getters = make();
  getters.recordMessageHandlerStats('bare');
  let reads = 0;
  let lookups = 0;
  const list = {get length(){return ++reads;}};
  Object.defineProperty(getters.messageHandlers, 'selected', {configurable:true, get(){lookups++; return list;}});
  getters.recordMessageHandlerStats('selected');
  assert.equal(reads, 2);
  assert.equal(lookups, 1);
  assert.equal(getters.messageHandlersStats.selected.lastCallbacksCount, 2);
  const original = {marker:'original length getter error'};
  Object.defineProperty(getters.messageHandlers, 'selected', {value:{get length(){throw original;}}});
  assert.throws(()=>getters.recordMessageHandlerStats('selected'), error=>error===original);
  assert.equal(getters.messageHandlersStats.selected.lastCallbacksCount, 2);
  now += 300;
  assert.throws(()=>getters.cleanupMessageHandlerStats('other'), error=>error===original);
  now += 6000;
  assert.throws(()=>getters.recordMessageHandlerStats('bare'), error=>error===original);
  for (const [file, owner] of [['public','OKXPublicWebsocket'], ['business','OKXBusinessWebsocket']]) {
    const dispatch = declarations('okx/okx.ws-'+file+'.ts', owner, ['dispatchMessageHandler']);
    const child = {Base:context.Base};
    vm.runInNewContext(ts.transpileModule('class Derived extends Base {'+dispatch+'} globalThis.Derived=Derived;',
      {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, child);
    const receiver = Object.assign(new child.Derived(), make());
    let calls=0;
    const parser=()=>calls++;
    const table = {selected:[parser], get unrelated(){throw new Error('must not enumerate');}};
    receiver.messageHandlers=table;
    receiver.dispatchMessageHandler('selected', {});
    Object.defineProperty(table, 'selected', {value:[],writable:true,configurable:true});
    receiver.dispatchMessageHandler('selected', {});
    delete table.selected;
    receiver.dispatchMessageHandler('selected', {});
    table.selected=[parser];
    receiver.dispatchMessageHandler('selected', {});
    assert.equal(calls,2);
    delete table.selected;
    Object.setPrototypeOf(table, {selected:[parser]});
    receiver.dispatchMessageHandler('selected', {});
    assert.equal(calls,3);
    const trace=[];
    const callbacks=[()=>{
      trace.push('first');
      callbacks.push(()=>trace.push('later'));
      delete receiver.messageHandlers.selected;
    }];
    receiver.messageHandlers={selected:callbacks};
    receiver.dispatchMessageHandler('selected', {});
    receiver.dispatchMessageHandler('selected', {});
    assert.deepEqual(trace,['first','later']);
    Object.defineProperty(receiver.messageHandlers,'selected',{configurable:true,get(){throw original;}});
    assert.throws(()=>receiver.dispatchMessageHandler('selected', {}), error=>error===original);
    Object.defineProperty(receiver.messageHandlers,'selected',{configurable:true,value:[()=>{throw original;} ]});
    assert.throws(()=>receiver.dispatchMessageHandler('selected', {}), error=>error===original);
  }
  process.stdout.write('handler-registry');
  process.exit(0);
}

const base = '../../../packages/safe-cex/src/exchanges/okx/';
const {mapOkxHistoricalOrder} = loadSource(new URL(base+'okx.order-history.ts', import.meta.url));
const {mapOkxClosedPosition} = loadSource(new URL(base+'okx.closed-pnl.ts', import.meta.url));
const {mapOkxBillToIncomeRecords} = loadSource(new URL(base+'okx.income.ts', import.meta.url));
const {mapOkxPositionHistory} = loadSource(new URL('../../../packages/safe-cex/src/journal/okx.position-adapter.ts', import.meta.url));
const row = {instId:'BTC-USDT-SWAP',cTime:'1700000000000',uTime:'1700000001000',closeTotalPos:'2',openMaxPos:'3',closeAvgPx:'60000',openAvgPx:'59000',realizedPnl:'17',fee:'-1',fundingFee:'2',lever:'5',posId:'position-1',ccy:'',direction:'long'};
const market = {symbol:'BTCUSDT'};
let checked = 0;
function check(name, invoke) {
  const original = {message:'callback exploded',marker:'opaque cause'};
  let calls = 0;
  const callback = () => {calls++; throw original;};
  assert.throws(() => invoke(callback), error => error === original, name+' original throw identity');
  assert.equal(calls, 1, name+' stops at first failure');
  checked++;
}
check('history', callback => mapOkxHistoricalOrder({ordId:'history-1',instId:row.instId,side:'buy',posSide:'net',ordType:'limit',state:'filled',sz:'3',accFillSz:'2',px:'60011',avgPx:'60000',cTime:row.cTime,uTime:row.uTime,reduceOnly:'false'},market,callback));
check('closed', callback => mapOkxClosedPosition(row,market,callback,'account'));
check('income', callback => mapOkxBillToIncomeRecords({ts:row.cTime,instId:row.instId,balChg:'17',fee:'-1',pnl:'12',billId:'bill-1',type:'2',ccy:'USDT'},callback));
for (const slot of ['toBase','resolveSymbol','resolveSettleCurrency']) {
  check('position/'+slot, callback => mapOkxPositionHistory(row,{account:'account',toBase:()=>2,resolveSymbol:()=> 'BTCUSDT',resolveSettleCurrency:()=> 'USDT',[slot]:callback}));
}
assert.equal(checked,6);
process.stdout.write(String(checked));
