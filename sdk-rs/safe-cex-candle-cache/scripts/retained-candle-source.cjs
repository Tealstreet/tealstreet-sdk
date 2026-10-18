const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function load(relative, globals = {}) {
  const source = fs.readFileSync(path.join(root, relative), 'utf8');
  const js = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const module = {exports:{}};
  vm.runInNewContext(js, {module,exports:module.exports,require(name){throw Error(`unexpected source import ${name}`);},console,...globals});
  return module.exports;
}
(async () => {
  const {KeyValueCandleCacheBackend} = load('packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts');
  const values = new Map();
  const kv = new KeyValueCandleCacheBackend({async get(key){return values.get(key) ?? null;},async set(key,value){values.set(key,value);},async delete(key){values.delete(key);},async listKeys(prefix){return [...values.keys()].filter(key=>key.startsWith(prefix));}});
  const numbers = {timestamp:0,open:-0,high:Infinity,low:-Infinity,close:1.25,volume:NaN,unknown:{n:NaN,z:-0},ownUndefined:undefined};
  await kv.persistChunks('numbers',new Map([[0,[numbers]]]));
  const restored = (await kv.loadChunks('numbers',[0],{chunkSize:2,intervalSeconds:60})).get(0)?.[0];
  assert(restored,'KV source must retain its null-valued candle');
  assert.strictEqual(restored.volume,null); assert.strictEqual(restored.high,null); assert.strictEqual(restored.low,null);
  assert(Object.is(restored.open,0)); assert(!Object.is(restored.open,-0));
  assert.strictEqual(restored.unknown.n,null); assert(!Object.hasOwn(restored,'ownUndefined'));
  const mixed = {timestamp:0,open:'text',high:true,low:[],close:{n:7},volume:null,unknown:{kept:true}};
  values.set('tealstreet-candles::mixed::0',JSON.stringify({candles:[mixed]}));
  const retainedMixed = (await kv.loadChunks('mixed',[0],{chunkSize:2,intervalSeconds:60})).get(0)?.[0];
  assert.deepEqual(JSON.parse(JSON.stringify(retainedMixed)),mixed,'KV source retains nonnumber fields and unknown members');
  const missing = {timestamp:0,unknown:'kept'};
  values.set('tealstreet-candles::missing::0',JSON.stringify({candles:[missing]}));
  const retainedMissing = (await kv.loadChunks('missing',[0],{chunkSize:2,intervalSeconds:60})).get(0)?.[0];
  assert(retainedMissing); assert(!Object.hasOwn(retainedMissing,'volume')); assert.equal(retainedMissing.unknown,'kept');

  const records = new Map();
  const db = {transaction(){const tx={objectStore(){return {
    put(record,key){records.set(key,structuredClone(record));},
    get(key){const request={result:structuredClone(records.get(key))};Object.defineProperty(request,'onsuccess',{set(callback){queueMicrotask(callback);}});return request;},
  };}};Object.defineProperty(tx,'oncomplete',{set(callback){queueMicrotask(callback);}});return tx;}};
  const {IndexedDBCandleCacheBackend} = load('packages/safe-cex-web/src/candleCache/IndexedDBCandleCacheBackend.ts',{indexedDB:{}});
  const idb = new IndexedDBCandleCacheBackend({reportError(error){throw error;}});
  idb.open = async () => db;
  await idb.persistChunks('numbers',new Map([[0,[numbers]]]));
  const native = (await idb.loadChunks('numbers',[0],{chunkSize:2,intervalSeconds:60})).get(0)?.[0];
  assert(native,'IDB source must retain its native-number candle');
  assert(Number.isNaN(native.volume)); assert.strictEqual(native.high,Infinity); assert.strictEqual(native.low,-Infinity); assert(Object.is(native.open,-0));
  assert(Number.isNaN(native.unknown.n)); assert(Object.is(native.unknown.z,-0));
  assert(Object.hasOwn(native,'ownUndefined')); assert.strictEqual(native.ownUndefined,undefined);
  assert(!Object.hasOwn(native,'absent'));
  console.log('CANDLE-RETAINED source: KV null/nonnumber/missing/unknown fields retained; IDB native number bits and own undefined retained PASS');
  console.log('IDB harness executes maintained methods with native structuredClone and injected transaction callbacks; no browser database execution claim.');
})().catch(error=>{console.error(error);process.exitCode=1;});
