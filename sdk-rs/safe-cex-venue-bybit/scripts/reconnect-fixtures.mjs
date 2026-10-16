import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const source=ts.createSourceFile('private.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const owner=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitPrivateWebsocket');
const method=owner.members.find(n=>n.name?.getText(source)==='sendReconnectSequence');
assert.ok(method?.initializer,'source reconnect operation required');
const cases=[];
for(const disposed of [false,true]) for(const reconnecting of [false,true]) for(const result of ['yes','no','throw']) {
 const trace=[];let release;
 const context=vm.createContext({Error,ENABLE_RECONNECTING_WEBSOCKET:reconnecting,
 virtualClock:{syncBybitClientServerTime(){trace.push('sync');return new Promise(r=>release=r);}},
 sleep:async n=>{trace.push(`sleep:${n}`);}});
 vm.runInContext(ts.transpileModule(`globalThis.run=${method.initializer.getText(source)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 // Arrow's lexical this is the vm global, as when initialized on the source instance.
 Object.assign(context,{isDisposed:disposed,auth:async()=>{trace.push('auth');if(result==='throw')throw new Error('signer rejected');return result==='yes';},
 emitAndLogError:e=>trace.push(`error:${e.message??e}`),directFallback:{disregardSilence:()=>trace.push('disregard')},
 reconnect:()=>trace.push('reconnect'),subscribe:()=>trace.push('subscribe'),ping:()=>trace.push('ping'),
 parent:{fetchAndSetPrivateSnapshot:()=>trace.push('snapshot')}});
 const pending=context.run(true);const prefix=[...trace];release?.();await pending;
 const settledTrace=[...trace];
 context.isDisposed=false;trace.length=0;
 const abandoned=context.run(true);context.isDisposed=true;release();await abandoned;
 const authAfterDispose=trace.includes('auth');
 assert.equal(authAfterDispose,true,'source promise continues auth after disposal');
 cases.push({disposed,reconnecting,result,prefix,trace:settledTrace,authAfterDispose});
}
assert.equal(cases.length,12);process.stdout.write(JSON.stringify(cases));
