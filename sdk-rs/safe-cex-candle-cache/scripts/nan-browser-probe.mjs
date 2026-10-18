import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const {chromium}=process.env.CANDLE_CACHE_PLAYWRIGHT?await import(pathToFileURL(resolve(process.env.CANDLE_CACHE_PLAYWRIGHT))):await import('playwright');
const directory=resolve(process.argv[2]);
const server=createServer(async(req,res)=>{
  if(['/source-kv.js','/source-indexed.js'].includes(req.url)){const file=req.url==='/source-kv.js'?'packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts':'packages/safe-cex-web/src/candleCache/IndexedDBCandleCacheBackend.ts';res.setHeader('Content-Type','text/javascript');res.end(ts.transpileModule(await readFile(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.ES2022,target:ts.ScriptTarget.ES2022}}).outputText);return;}
  if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<html><body>NaN cache codec proof</body></html>');return;}
  try {const file=resolve(directory,'.'+req.url);if(!file.startsWith(directory+'/'))throw Error('path');res.setHeader('Content-Type',file.endsWith('.wasm')?'application/wasm':'text/javascript');res.end(await readFile(file));}catch{res.statusCode=404;res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;let timeout;
try {
  browser=await chromium.launch({executablePath:process.env.CANDLE_CACHE_BROWSER,headless:true,timeout:15000});
  const page=await browser.newPage();page.setDefaultTimeout(20000);
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result=await Promise.race([page.evaluate(async()=>{
    const {KeyValueCandleCacheBackend}=await import('/source-kv.js');
    const {IndexedDBCandleCacheBackend}=await import('/source-indexed.js');
    const values=new Map();const store={get:async key=>values.get(key),set:async(key,value)=>values.set(key,value),delete:async key=>values.delete(key),listKeys:async prefix=>[...values.keys()].filter(k=>k.startsWith(prefix))};
    let sourceProfiles=0;
    for(const field of ['volume','open','high','low','close']) {
      const row={timestamp:1700004000000,open:1,high:3,low:0.5,close:2,volume:4,[field]:NaN};
      await new KeyValueCandleCacheBackend(store).persistChunks(field,new Map([[283334,[row]]]));
      const kv=(await new KeyValueCandleCacheBackend(store).loadChunks(field,[283334],{chunkSize:100,intervalSeconds:60})).get(283334)?.[0];
      if(!kv||kv[field]!==null)throw Error('maintained KV must retain null encoding');
      const opts={dbName:'port-txflow-nan-source-'+field};const writer=new IndexedDBCandleCacheBackend(opts);
      await writer.persistChunks(field,new Map([[283334,[row]]]));
      const idb=(await new IndexedDBCandleCacheBackend(opts).loadChunks(field,[283334],{chunkSize:100,intervalSeconds:60})).get(283334)?.[0];
      if(!idb||!Number.isNaN(idb[field]))throw Error('maintained IndexedDB must preserve native NaN');
      sourceProfiles++;
    }
    if(sourceProfiles!==5)throw Error('source profile floor');
    const module=await import('/indexed_db_probe.js');await module.default();
    const storage=(name,scenario,field)=>new Promise((resolve,reject)=>{
      const open=indexedDB.open(name,1);open.onerror=()=>reject(open.error);open.onsuccess=()=>{
        const db=open.result;const transaction=db.transaction('chunks',scenario.startsWith('inspect-')?'readonly':'readwrite');const store=transaction.objectStore('chunks');const get=store.get('cache::283334');
        get.onerror=()=>reject(get.error);get.onsuccess=()=>{
          try {
            const record=get.result;if(!record||!Array.isArray(record.candles))throw Error('actual stored record missing');
            const candle=record.candles[0];
            if(scenario==='inspect-nan'){if(!Number.isNaN(candle[field]))throw Error('IndexedDB must preserve native NaN on write');}
            else if(scenario==='inspect-null'){if(candle[field]!==null)throw Error('read must leave persisted null unchanged');}
            else {if(scenario==='null')candle[field]=null;else if(scenario==='missing')delete candle[field];else if(scenario==='timestamp')candle.timestamp=null;else candle[field]='bad';store.put(record,'cache::283334');}
          }catch(e){reject(e);}
        };
        transaction.onerror=()=>reject(transaction.error);transaction.oncomplete=()=>{db.close();resolve();};
      };
    });
    const codec = await module.nan_codec_probe(storage);
    const negative = [];
    negative[-1] = {timestamp:-60000,open:11,high:17,low:7,close:13,volume:19};
    const sourceNegative = new IndexedDBCandleCacheBackend({dbName:'port-txflow-source-negative'});
    await sourceNegative.persistChunks('negative',new Map([[-1,negative]]));
    const open = indexedDB.open('port-txflow-source-negative',1);
    const sourceRecord = await new Promise((resolve,reject)=>{
      open.onerror=()=>reject(open.error);open.onsuccess=()=>{
        const db=open.result,tx=db.transaction('chunks','readonly'),get=tx.objectStore('chunks').get('negative::-1');
        get.onerror=()=>reject(get.error);get.onsuccess=()=>{resolve(get.result);db.close();};
      };
    });
    if(sourceRecord!==undefined)throw Error('source filter must drop negative array properties');
    const audit = await module.fix_audit_probe();
    if(!audit.includes('cancelled waiter ownership passed'))throw Error('native audit receipt absent');
    const cancellation = await module.cancellation_probe();
    return codec+'; '+audit+'; '+cancellation;
  }),new Promise((_,reject)=>{timeout=setTimeout(()=>reject(Error("NaN browser probe timed out")),20000);})]);
  assert.match(result,/26 real profiles/);assert.deepEqual(errors,[]);console.log(result);
}finally{clearTimeout(timeout);await browser?.close();server.close();}
