const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const source = ts.createSourceFile('bybit.exchange.ts', fs.readFileSync(path.join(process.cwd(), 'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'), 'utf8'), 99, true);
assert.equal(source.parseDiagnostics.length, 0);
const klass = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BybitExchange');
const fields = klass.members.filter(n => n.name?.getText(source) === 'reconcileLocalReduceOnlyLimitOrder');
assert.equal(fields.length, 1);
const constants = source.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(source) === 'RECENTLY_PLACED_ORDER_GRACE_MS'));
assert.equal(constants.length, 1);
const program = constants[0].getText(source) + '\nclass Witness {reconcileLocalReduceOnlyLimitOrder=' + fields[0].initializer.getText(source) + ';}\nexports.Witness=Witness;';
const transformed = ts.transpileModule(program, {compilerOptions:{target:7,module:1},reportDiagnostics:true});
assert.equal(transformed.diagnostics.length, 0);
const loggingFile = path.join(process.cwd(), 'packages/logging/src/LogSeverity.ts');
const loggingSource = ts.createSourceFile(loggingFile, fs.readFileSync(loggingFile, 'utf8'), 99, true);
assert.equal(loggingSource.parseDiagnostics.length, 0);
const loggingProgram = ts.transpileModule(loggingSource.getText(), {compilerOptions:{target:7,module:1},reportDiagnostics:true});
assert.equal(loggingProgram.diagnostics.length, 0);
const logging = {exports:{}};
vm.runInNewContext(loggingProgram.outputText, logging, {filename:loggingFile});
const LogSeverity = logging.exports.LogSeverity;
assert.equal(LogSeverity.Warning, 'warning', 'actual maintained warning severity');
const names = ['missing-entry','disposed','not-eligible','full-fill','tolerance-boundary','below-tolerance','zero-target','within-grace','grace-boundary','fills-reject','fills-reject-lone-message','fills-reject-falsy','orders-reject','log-throws','disposed-after-fills','disposed-after-orders','retained-target','store-order-preferred'];
async function run(name) {
  const events = [];
  let now = name === 'within-grace' ? 7999 : 8000;
  const order = {id:'id-1',amount:10,remaining:10,filled:0};
  const entry = {placedAt:0,order};
  const storeOrder = {id:'id-1',amount:20,remaining:20,filled:0};
  const primary = name==='fills-reject-falsy' ? 0 : new Error('original REST refusal'+(name==='fills-reject-lone-message'?String.fromCharCode(0xd800):''));
  const reporting = new Error('warning listener refusal');
  let releaseFills, releaseOrders;
  const fillsHeld = new Promise(resolve => {releaseFills=resolve;});
  const ordersHeld = new Promise(resolve => {releaseOrders=resolve;});
  const context = vm.createContext({exports:{},virtualClock:{getCurrentTimeMs(){events.push(['clock']);return now;}},LogSeverity});
  vm.runInContext(transformed.outputText, context);
  const owner = new context.exports.Witness();
  owner.isDisposed = name === 'disposed';
  owner.recentlyPlacedOrders = new Map(name === 'missing-entry' ? [] : [['id-1',entry]]);
  owner.store = {
    orders: name === 'store-order-preferred' ? [{id:'unrelated',amount:999,remaining:999,filled:0},storeOrder] : [],
    loaded:{sentinel:true,orders:false},
    removeOrders(rows){assert.equal(this,owner.store,'source remove receiver');events.push(['remove',rows.map(row=>row.id)]);},
    update(value){assert.equal(this,owner.store,'source update receiver');events.push(['update',value.orders,value.loaded]);},
  };
  owner.shouldReconcileLocalReduceOnlyLimitOrder = function(actual) {assert.equal(this,owner,'source eligibility receiver');events.push(['eligible',actual===storeOrder?'store':'entry']);return name!=='not-eligible';};
  owner.getFilledOrderReconciliationTarget = function(actual) {assert.equal(this,owner,'source target receiver');events.push(['target',actual.amount]);return name==='zero-target'?0:actual.amount;};
  let filled = 1;
  if(name==='full-fill'||name==='retained-target')filled=10;
  if(name==='tolerance-boundary')filled=10-1e-7;
  if(name==='below-tolerance')filled=10-2e-7;
  if(name==='store-order-preferred')filled=15;
  owner.fetchFillsForOrderIds = function(ids) {
    assert.equal(this,owner,'source fills receiver');
    events.push(['fills',Array.from(ids)]);
    if(['fills-reject','fills-reject-lone-message','fills-reject-falsy','log-throws'].includes(name))return Promise.reject(primary);
    if(['disposed-after-fills','retained-target'].includes(name))return fillsHeld;
    return Promise.resolve(new Map([['id-1',filled]]));
  };
  owner.markOrderSeen = function(id) {assert.equal(this,owner,'source seen receiver');events.push(['seen',id]);};
  owner.fetchOrders = function() {
    assert.equal(this,owner,'source orders receiver');
    events.push(['orders']);
    if(name==='orders-reject')return Promise.reject(primary);
    if(name==='disposed-after-orders')return ordersHeld;
    return Promise.resolve([{id:'snapshot-row'}]);
  };
  owner.log = function(message,severity) {
    assert.equal(this,owner,'source log receiver');
    events.push(['log',{u16:Array.from({length:message.length},(_,i)=>message.charCodeAt(i))},severity]);
    if(name==='log-throws')throw reporting;
  };
  const started = owner.reconcileLocalReduceOnlyLimitOrder('id-1');
  if(name==='disposed-after-fills') {
    owner.isDisposed=true;
    releaseFills(new Map([['id-1',10]]));
  }
  if(name==='retained-target') {
    owner.recentlyPlacedOrders.set('id-1',{placedAt:now,order:storeOrder});
    releaseFills(new Map([['id-1',filled]]));
  }
  if(name==='disposed-after-orders') {
    for(let n=0;n<5;n++)await Promise.resolve();
    assert.equal(events.filter(e=>e[0]==='orders').length,1,'source reached second await');
    owner.isDisposed=true;
    releaseOrders([{id:'late-row'}]);
  }
  let rejected;
  try {await started;}catch(error){rejected=error;}
  assert.equal(rejected,name==='log-throws'?reporting:undefined,name+' result identity');
  const count = kind => events.filter(e=>e[0]===kind).length;
  const guarded = ['missing-entry','disposed','not-eligible'].includes(name);
  assert.equal(count('fills'),guarded?0:1,name+' fills floor');
  const full=['full-fill','tolerance-boundary','retained-target'].includes(name);
  assert.equal(count('seen'),Number(full),name+' seen floor');
  assert.equal(count('remove'),Number(full),name+' remove floor');
  const fetches = ['below-tolerance','zero-target','grace-boundary','orders-reject','disposed-after-orders','store-order-preferred'].includes(name);
  assert.equal(count('orders'),Number(fetches),name+' orders floor');
  assert.equal(count('update'),Number(fetches&&!['orders-reject','disposed-after-orders'].includes(name)),name+' update floor');
  assert.equal(count('log'),Number(['fills-reject','fills-reject-lone-message','fills-reject-falsy','orders-reject','log-throws'].includes(name)),name+' log floor');
  if(name==='retained-target')assert.deepEqual(events.find(e=>e[0]==='target'),['target',10]);
  if(name==='store-order-preferred')assert.deepEqual(events.find(e=>e[0]==='target'),['target',20]);
  return {name,events,rejected:rejected?.message};
}
(async()=>{
  const results=[];
  for(const name of names)results.push(await run(name));
  assert.equal(results.length,18);
  process.stdout.write(JSON.stringify(results)+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
