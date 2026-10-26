import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const directory=resolve(process.argv[2]);
const server=createServer(async (req,res)=>{
  if(req.url === '/oracle.js') { const source=await readFile('packages/safe-cex-web/src/candleCache/IndexedDBCandleCacheBackend.ts','utf8'); res.setHeader('Content-Type','text/javascript');res.end(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText);return; }
  if(req.url === '/') {res.setHeader('Content-Type','text/html');res.end('<html><body>IndexedDB proof</body></html>');return;}
  try { const file=resolve(directory, '.'+req.url); if(!file.startsWith(directory+'/')) throw Error('path');res.setHeader('Content-Type',file.endsWith('.wasm')?'application/wasm':'text/javascript');res.end(await readFile(file)); } catch {res.statusCode=404;res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const browser=await chromium.launch({executablePath:process.env.CANDLE_CACHE_BROWSER || undefined,headless:true});
let timer;
try {
 const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 await page.goto(`http://127.0.0.1:${server.address().port}/`);
 const result=await Promise.race([page.evaluate(async()=>{const module=await import('/indexed_db_probe.js');await module.default();const indexed=await module.probe();
 await new Promise((resolve,reject)=>{const request=indexedDB.open('p3b-loaded-chunks-proof',1);request.onupgradeneeded=()=>request.result.createObjectStore('chunks');request.onerror=()=>reject(request.error);request.onsuccess=()=>{const db=request.result;const tx=db.transaction('chunks','readwrite');const store=tx.objectStore('chunks');const row=timestamp=>({candles:[{timestamp,open:1,high:3,low:0.5,close:2,volume:4}],lastAccessed:0});for(const [key,t] of [['k::1',6000000],['k::2tail',12000000],['k::9223372036854775808',6000000],['k::bad',6000000],['k::'+'9'.repeat(400),6000000],['other::3',18000000]])store.put(row(t),key);tx.oncomplete=()=>{db.close();resolve();};tx.onerror=()=>reject(tx.error);};});
 const loaded=await module.loaded_chunks_probe();const data=new Map([['unrelated','keep']]);const storage={getItem:async key=>data.get(key)??null,setItem:async(key,value)=>{data.set(key,value);},removeItem:async key=>{data.delete(key);},getAllKeys:async()=>[...data.keys()]};const mobile=await module.async_storage_probe(storage);if(data.size!==1||data.get('unrelated')!=='keep')throw Error('AsyncStorage prefix clear escaped namespace');const cancellation=await module.cancellation_probe();
 const originalCreate=IDBDatabase.prototype.createObjectStore;
 let upgradeProof;
 try {
   IDBDatabase.prototype.createObjectStore=function(){throw new DOMException('injected upgrade failure','ConstraintError');};
   upgradeProof=await module.upgrade_failure_probe();
 } finally {IDBDatabase.prototype.createObjectStore=originalCreate;}
 const originalGet=IDBObjectStore.prototype.get;
 const originalCursor=IDBObjectStore.prototype.openCursor;
 const originalTransaction=IDBDatabase.prototype.transaction;
 const record={candles:[{timestamp:6000000,open:1,high:3,low:0.5,close:2,volume:4}],lastAccessed:0};
 const patch=mode=>{
   IDBObjectStore.prototype.get=originalGet;
   IDBObjectStore.prototype.openCursor=originalCursor;
   IDBDatabase.prototype.transaction=originalTransaction;
   if(mode==='get'||mode==='missing') {
     const requests=[];
     IDBObjectStore.prototype.get=function(key){
       const request={result:mode==='missing'&&requests.length===0?{lastAccessed:0}:record};requests.push(request);
       if(requests.length===2)queueMicrotask(()=>{if(mode==='get')requests[0].onerror?.(new Event('error'));else {try{requests[0].onsuccess?.(new Event('success'));}catch(error){if(!(error instanceof TypeError))throw error;}} requests[1].onsuccess?.(new Event('success'));});
       return request;
     };
   }
   if(mode==='cursor'||mode==='throw')IDBObjectStore.prototype.openCursor=function(){
     const request={result:{key:'fault::1',value:record,continue(){if(mode==='throw')throw Error('injected cursor continue failure');queueMicrotask(()=>request.onerror?.(new Event('error')));}}};
     queueMicrotask(()=>{try{request.onsuccess?.(new Event('success'));}catch(error){if(mode!=='throw')throw error;}});return request;
   };
   if(mode==='abort')IDBDatabase.prototype.transaction=function(){
     const tx={objectStore(){return {put(){},openCursor(){const request={result:null};queueMicrotask(()=>request.onsuccess?.(new Event('success')));return request;}};}};
     setTimeout(()=>tx.onabort?.(new Event('abort')),0);return tx;
   };
 };
 let faults;try{faults=await module.error_path_probe(patch);}finally{patch('restore');}
 const originalContinue=IDBCursor.prototype.continue;
 const rollbackPatch=mode=>{
   IDBCursor.prototype.continue=originalContinue;
   if(mode==='rollback'){
     let continuations=0;
     IDBCursor.prototype.continue=function(...args){
       if(++continuations===2)throw new DOMException('injected cursor continuation failure','InvalidStateError');
       return originalContinue.apply(this,args);
     };
   }
 };
 let rollback;try{rollback=await module.cursor_rollback_probe(rollbackPatch);}finally{rollbackPatch('restore');}
 const {IndexedDBCandleCacheBackend}=await import('/oracle.js');
 const descriptor=Object.getOwnPropertyDescriptor(globalThis,'indexedDB');
 const factory=globalThis.indexedDB;
 const restoreFactory=()=>{if(descriptor)Object.defineProperty(globalThis,'indexedDB',descriptor);else delete globalThis.indexedDB;};
 const patchFactory=mode=>{restoreFactory();if(mode==='null')Object.defineProperty(globalThis,'indexedDB',{configurable:true,value:null});if(mode==='accessor')Object.defineProperty(globalThis,'indexedDB',{configurable:true,get(){throw Error('injected accessor failure');}});};
 try {
   for(const mode of ['null','accessor']) {
     patchFactory(mode);let reports=0;const unavailable=new IndexedDBCandleCacheBackend({dbName:'p3b-availability-'+mode,reportError:()=>reports++});
     if(mode==='null') {await unavailable.loadChunks('k',[0],{chunkSize:100,intervalSeconds:60});if(reports!==2||!unavailable.dbUnavailable)throw Error('TS null factory must report twice and latch');}
     else {let threw=false;try{await unavailable.loadChunks('k',[0],{chunkSize:100,intervalSeconds:60});}catch(error){threw=error.message==='injected accessor failure';}if(!threw||reports!==0||unavailable.dbUnavailable)throw Error('TS accessor must escape pre-try guard without report or latch');}
     restoreFactory();
   }
   await module.availability_probe(patchFactory);
 } finally {restoreFactory();}

 const oracle=new IndexedDBCandleCacheBackend({dbName:'p3b-error-proof'});
 const options={chunkSize:100,intervalSeconds:60};
 await oracle.loadChunks('fault',[1],options);
 try {
   patch('cursor');const prefix=await oracle.loadAllChunks('fault',options);
   if(prefix.size!==1||!prefix.has(1))throw Error('TS oracle lost cursor prefix');
   patch('throw');const callbackOutcome=await Promise.race([oracle.loadAllChunks('fault',options).then(()=> 'resolved'),new Promise(resolve=>setTimeout(()=>resolve('pending'),20))]);
   if(callbackOutcome!=='pending')throw Error('TS callback-exception pending oracle changed');
   patch('get');const siblings=await oracle.loadChunks('fault',[1,2],options);
   if(siblings.size!==1||!siblings.has(2))throw Error('TS oracle lost GET sibling');
   patch('missing');let missingReports=0;oracle.reportError=()=>missingReports++;
   const missingOutcome=await Promise.race([oracle.loadChunks('fault',[1,2],options).then(()=> 'resolved'),new Promise(resolve=>setTimeout(()=>resolve('pending'),20))]);
   if(missingOutcome!=='pending'||missingReports!==0)throw Error('TS missing-candles hang/report behavior changed');
   patch('abort');const pending=[oracle.persistChunks('fault',new Map()),oracle.clear('fault')];
   const outcome=await Promise.race([Promise.any(pending).then(()=> 'resolved'),new Promise(resolve=>setTimeout(()=>resolve('pending'),20))]);
   if(outcome!=='pending')throw Error('TS abort-only pending oracle changed');
 } finally {patch('restore');}
 faults+='; TypeScript oracle independently passed; '+rollback;
 return loaded+'\n'+upgradeProof+'\n'+indexed+'\n'+mobile+'\n'+cancellation+'\n'+faults;}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Browser probe timed out')),5000);})]);
 if(errors.length)throw Error(errors.join('\n')); console.log(result);
} finally {clearTimeout(timer);await browser.close();server.close();}
