import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetPrivateWebsocket');
const field=name=>{const f=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);assert(f?.initializer,'THROTTLE actual source '+name);return f.initializer.getText(ast);};
const names=['throttledFetchPositions','throttledFetchBalance','throttledFetchOrders'];
assert.deepEqual(['position','balance','orders'].map(k=>Number(field(k+'FetchThrottleMs'))),[500,300,500],'THROTTLE independent source default delays');
const code=ts.transpileModule('class Source {'+names.map(n=>n+'='+field(n)+';').join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Source=new Function(code+';return Source;')();
const decode=v=>v==='nan'?NaN:v;
const wire=v=>{if(Number.isNaN(v))return 'nan';const b=Buffer.alloc(8);b.writeDoubleLE(v);return [...b];};
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(cases.length,190,'THROTTLE independent input floor');
const outputs=cases.map(c=>{
 const trace=[],delays=[],error={units:[65,0xd800,66,0xdc00]};let disposed=c.disposed,scheduled=c.scheduled,pending=c.pending,reads=0,launches=0,queued=null,callbackRan=false,setupError=false,callbackError=false;
 const record=e=>{trace.push(e);if(trace.length-1===c.fail)throw error;};
 const stem=c.kind==='positions'?'position':c.kind==='balance'?'balance':'orders';
 const capital=stem[0].toUpperCase()+stem.slice(1);
 const target=new Source();
 Object.defineProperties(target,{
  isDisposed:{get(){record('disposed');return disposed;}},
  [stem+'FetchScheduled']:{get(){record('scheduled');return scheduled;},set(v){record('scheduled:'+v);scheduled=v;}},
  ['last'+capital+'FetchTime']:{get(){record('last');return decode(c.last);}},
  [stem+'FetchThrottleMs']:{get(){record('throttle');reads++;return decode(reads===1?c.first:c.second);}},
  ['hasPending'+(c.kind==='orders'?'Order':capital)+'Events']:{get(){record('pending');return pending;},set(v){record('pending:'+v);pending=v;}}
 });
 target['execute'+(c.kind==='orders'?'Orders':capital)+'Fetch']=()=>{record('launch');launches++;return {then(){throw Error('THROTTLE must not await fetch');}};};
 const mutate=()=>{disposed=c.callbackDisposed;pending=c.callbackPending;};
 const oldNow=Date.now,oldSet=globalThis.setTimeout;
 Date.now=()=>{record('now');return decode(c.now);};
 globalThis.setTimeout=(fn,delay)=>{delays.push(delay);record('set');if(c.inline){mutate();callbackRan=true;fn();}else queued=fn;return {opaque:[0xd800]};};
 try {target[names[['positions','balance','orders'].indexOf(c.kind)]]();}catch(e){assert.equal(e,error);setupError=true;}
 if(!setupError&&queued){const fn=queued;queued=null;mutate();callbackRan=true;try{fn();}catch(e){assert.equal(e,error);callbackError=true;}}
 Date.now=oldNow;globalThis.setTimeout=oldSet;
 return {trace,delays:delays.map(wire),scheduled,pending,launches,callbackRan,setupError,callbackError};
});
assert.equal(outputs.length,190,'THROTTLE independent source floor');
console.log(JSON.stringify(outputs));
