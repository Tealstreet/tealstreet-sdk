const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const source = file => ts.createSourceFile(file, fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges', file), 'utf8'), ts.ScriptTarget.Latest, true);
const run = (code, context) => vm.runInContext(ts.transpileModule(code, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, context);
const context = vm.createContext({exports:{}, setTimeout, LogSeverity:{Warning:'warning'}});
const base = source('base.ts');
const klass = base.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BaseExchange');
const wait = klass.members.find(n => n.name?.getText(base) === 'waitForRemoteSigner');
assert(wait, 'PREFIX actual signer member');
run(`class Base { ${wait.getText(base)} }; globalThis.Base=Base;`, context);
const socket = source('base.ws.ts');
const resync = socket.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d=>d.name.getText(socket)==='runReopenResync'));
assert(resync, 'PREFIX actual resync declaration');
run(`const reopenResyncInFlight=new WeakMap(); ${resync.getText(socket)}; globalThis.resync=runReopenResync;`, context);
const pager = source('income-paging.ts');
run(pager.getText(), context);
(async () => {
  let checks=0, callback, removed=0;
  const events=[];
  const signer={isAvailable(){checks++;return false;},onAvailabilityChange(cb){callback=cb;events.push('subscribe');return()=>{removed++;};}};
  const instance=new context.Base();
  Object.assign(instance,{options:{remoteSigner:signer},emitter:{emit(event,value){assert.equal(event,'remoteSignerStatus');events.push(value);}},isDisposed:false,remoteSignerAvailabilityCleanups:new Set()});
  const promise=instance.waitForRemoteSigner();
  assert.equal(checks,2,'PREFIX source signer availability checks before return');
  assert.deepEqual(events,['disconnected','subscribe'],'PREFIX source signer registration before return');
  callback(true); assert.equal(await promise,true);assert.equal(removed,1);
  let cases=0;
  for(const [budget,start,expected] of [[1,1,1],[NaN,1,1],[0,1,0],[-1,1,0],[1,5,0],[1,NaN,0]]) {
    let calls=0;
    const pending=context.exports.paginateIncomeRange(params=>{
      calls++;assert.deepEqual({...params},{startTime:1,endTime:5,limit:7},'PREFIX source pager first request');return Promise.resolve([]);
    },{startTime:start,endTime:5,chunkMs:8,pageLimit:7,maxPagesPerChunk:budget});
    assert.equal(calls,expected,'PREFIX source pager calls before returned Promise observation');
    const result=await pending;assert.equal(result.requests,expected);cases++;
  }
  assert.equal(cases,6,'PREFIX source independent pager case floor');
  let ledgerCalls=0;
  const ledger=await context.exports.paginateIncomeRange(()=>Promise.resolve(++ledgerCalls===1 ? [
    {symbol:'BTC',incomeType:'funding',income:1,asset:'USD',timestamp:2,tranId:'\ud800'},
    {symbol:'BTC',incomeType:'funding',income:1,asset:'USD',timestamp:2,tranId:'\ufffd'},
  ] : []),{startTime:1,endTime:5,chunkMs:8,pageLimit:7,maxPagesPerChunk:5});
  assert.equal(ledger.records.length,2,'PREFIX source independent returned income identity floor');
  assert.equal(ledger.records[0].tranId.charCodeAt(0),0xd800,'PREFIX source public pager dedupe keeps materialized unit');
  assert.equal(ledger.records[1].tranId.charCodeAt(0),0xfffd,'PREFIX source public pager dedupe distinguishes replacement');
  const reactions=[];
  context.resync({isDisposed:false,fetchAndSetPrivateSnapshot(n){assert.equal(n,10);reactions.push('fetch');return Promise.reject(new Error('snapshot rejected'));},log(){reactions.push('log');}});
  assert.deepEqual(reactions,['fetch'],'PREFIX source resync reactions are deferred');
  await new Promise(setImmediate);
  assert.deepEqual(reactions,['fetch','log'],'PREFIX source resync rejection logs on continuation');
  process.stdout.write('EXCHANGE_PREFIX_SOURCE signer=1 pager=6 resync=1\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
