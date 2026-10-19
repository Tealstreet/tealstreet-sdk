import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {root,ts,load} from '../tests/source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
const names=['onOpen','sendLogin','createLoginSignature','throttledFetchBalance'];
const bodies=names.map(name=>{const rows=owner.members.filter(n=>n.name?.getText(ast)===name);assert.equal(rows.length,1,name);return rows[0].getText(ast);});
let timers=[],id=0;
const context={virtualClock:{getCurrentTimeMs:()=>12345},nanoid:()=> 'source-id',
 setTimeout(callback,delay){const timer={id:++id,callback,delay};timers.push(timer);return timer.id;},
 clearTimeout(id){timers=timers.filter(timer=>timer.id!==id);}};
vm.runInNewContext(ts.transpileModule('class Socket {'+bodies.join('\n')+'}\nglobalThis.Socket=Socket;',
 {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
let release;const pending=new Promise(resolve=>{release=resolve;});
const trace=[],requests=[],updates=[];
const timerSocket=new context.Socket();
Object.assign(timerSocket,{isDisposed:false,parent:{fetchBalance(){requests.push('balance');return pending;}},store:{update(value){updates.push(value);}}});
timerSocket.throttledFetchBalance();timerSocket.throttledFetchBalance();
assert.equal(timers.length,1);assert.equal(timers[0].delay,1000);assert.equal(requests.length,0);
const timerWaiter=timers[0].callback();
assert.equal(requests.length,1,'source fetch prefix runs before the timer callback returns');
assert.equal(updates.length,0,'source awaits even a ready value');
const balance={free:17,total:23};release(balance);await timerWaiter;
assert.equal(updates.length,1);assert.equal(updates[0].balance,balance);
let signRelease;const signPending=new Promise(resolve=>{signRelease=resolve;});
const calls=[],sent=[];const socket=new context.Socket();
Object.assign(socket,{isDisposed:false,startHeartbeat(){trace.push('ping');},emitWsReconnected(){trace.push('reconnected');},
 sendWithDiagnostics(value){sent.push(JSON.parse(value));},parent:{options:{key:' fixture ',remoteSigner:{accountName:'account-A',sign(request){trace.push('sign');calls.push(request);return signPending;}}},fetchAndSetPrivateSnapshot(count){assert.equal(count,10);trace.push('snapshot');}}});
socket.onOpen();
assert.deepEqual(trace,['ping','sign','snapshot','reconnected']);assert.equal(calls.length,1);assert.equal(sent.length,0);
assert.equal(calls[0].accountName,'account-A');assert.equal(calls[0].exchangeName,'mexc');
assert.equal(calls[0].wsAuth.toSign,'fixture12345');assert.equal(calls[0].wsAuth.outputFormat,'hex');
signRelease({wsAuthSignature:'source-signature'});await new Promise(setImmediate);
assert.equal(sent.length,1);assert.equal(sent[0].param.apiKey,'fixture');assert.equal(sent[0].param.reqTime,'12345');
const exchangeFile=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const exchangeSource=fs.readFileSync(exchangeFile,'utf8');
const baseFile=root+'packages/safe-cex/src/exchanges/base.ts';
const baseSource=fs.readFileSync(baseFile,'utf8');
function select(source,filename,ownerName,names){
 const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===ownerName);
 return names.map(name=>{const matches=klass.members.filter(n=>n.name?.getText(ast)===name);assert.equal(matches.length,1,name);return matches[0].getText(ast);}).join('\n');
}
const baseBody=select(baseSource,baseFile,'BaseExchange',['fetchAndSetPrivateSnapshot']);
const parentBodies=select(exchangeSource,exchangeFile,'MexcExchange',['fetchOrders','fetchRegularOrders','orderListEndpoint','filterRecentlyCanceledStopLikeOrders']);
const rows=exchangeSource.slice(exchangeSource.indexOf('const extractData ='),exchangeSource.indexOf('\n\ntype MexcSymbolConfig'));
const astExchange=ts.createSourceFile(exchangeFile,exchangeSource,ts.ScriptTarget.Latest,true);
const lookback=astExchange.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(astExchange)==='MEXC_PLAN_ORDER_LOOKBACK_MS'));
assert.ok(lookback,'maintained lookback constant');
const require=createRequire(process.env.SAFE_CEX_TYPESCRIPT_PATH+'/package.json');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const parentContext={...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts'),
 omitUndefined:load('packages/safe-cex/src/utils/omitUndefined.ts').omitUndefined,uniqBy:require('lodash/uniqBy'),
 virtualClock:{getCurrentTimeMs:()=>12345},LogSeverity:{Warning:'warning'}};
vm.runInNewContext(ts.transpileModule(rows+'\n'+lookback.getText(astExchange)+'\nclass Parent{'+baseBody+'\n'+parentBodies+'}globalThis.Parent=Parent;',
 {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,parentContext);
const actualParent=new parentContext.Parent(),store=new DefaultStore(),snapshotRequests=[],warnings=[];
Object.assign(actualParent,{store,isPublicDataOnly:false,isDisposed:false,xhr:{get(url){snapshotRequests.push(url);return Promise.resolve({data:{data:[]}});}},
 log(message){warnings.push(message);},emitAndLogError(error){throw error;}});
const snapshotWaiter=actualParent.fetchAndSetPrivateSnapshot(10);
assert.equal(snapshotRequests.length,1,'source snapshot starts real regular-order HTTP before returning');
assert.equal(snapshotRequests[0],parentContext.ENDPOINTS.OPEN_ORDERS);
assert.equal(store.loaded.orders,false,'source ready HTTP never commits Store inline');
await snapshotWaiter;assert.equal(store.loaded.orders,true);assert.equal(snapshotRequests.length,3);assert.deepEqual(warnings,[]);
process.stdout.write(JSON.stringify({file,sha256:createHash('sha256').update(source).digest('hex'),names,timer:{delay:1000,reads:requests.length,updates:updates.length},signer:{trace,request:calls[0],sent},snapshot:{requests:snapshotRequests,loaded:store.loaded.orders}}));
