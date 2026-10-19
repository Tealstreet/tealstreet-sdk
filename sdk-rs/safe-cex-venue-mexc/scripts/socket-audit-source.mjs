import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
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
process.stdout.write(JSON.stringify({file,sha256:createHash('sha256').update(source).digest('hex'),names,timer:{delay:1000,reads:requests.length,updates:updates.length},signer:{trace,request:calls[0],sent}}));
