import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const source = new URL('../../../packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts', import.meta.url);
let text = fs.readFileSync(source, 'utf8');
const mutation = process.argv[2];
if (mutation) {
  const guards = {'--mutate-null-element':'if (candle === null) continue;',
    '--mutate-null-timestamp':'if (candle.timestamp === null) continue;'};
  assert.ok(guards[mutation], 'unknown oracle mutation');
  const next = text.replace('for (const candle of record.candles) {',
    'for (const candle of record.candles) {'+guards[mutation]);
  assert.notEqual(next, text, 'mutation must reach the maintained TypeScript loop');
  text = next;
}
const code = ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module = {exports:{}};
vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const {KeyValueCandleCacheBackend} = module.exports;
const healthy = { timestamp:6000000, open:1,high:2,low:0,close:1,volume:1 };
const values = new Map();
const backend = new KeyValueCandleCacheBackend({get: async key=>values.get(key),set:async()=>{},delete:async()=>{},listKeys:async()=>[]});
const options = {chunkSize:100,intervalSeconds:60};
values.set('tealstreet-candles::null-proof::1',JSON.stringify({candles:[healthy]}));
values.set('tealstreet-candles::null-proof::0',JSON.stringify({candles:[null]}));
await assert.rejects(backend.loadChunks('null-proof',[0,1],options), TypeError);
const nullTimestamp = {...healthy,timestamp:null};
values.set('tealstreet-candles::null-proof::0',JSON.stringify({candles:[nullTimestamp]}));
const chunks = await backend.loadChunks('null-proof',[0,1],options);
assert.equal(chunks.size,2,'source null timestamp must retain both chunks');
assert.deepEqual(chunks.get(0)[0],nullTimestamp);
assert.deepEqual(chunks.get(1)[0],healthy);
console.log('TS null-element whole-load rejection and null-timestamp zero coercion pinned');

const slotBackend=new KeyValueCandleCacheBackend({get:async()=>JSON.stringify({candles:[healthy]}),set:async()=>{},delete:async()=>{},listKeys:async()=>[]});
for(const [label,width,key] of [['NaN',NaN,0],['zero',0,2**63],['negative-zero',-0,-(2**63)],['finite-overflow',1e-300,2**63]]) {
 const result=await slotBackend.loadChunks('slots',[key],{chunkSize:1,intervalSeconds:width});
 assert.equal(result.size,0,`invalid derived index ${label} must not report a cache hit`);
}
for(const [width,key] of [[60,100],[Infinity,0]]) {
 const result=await slotBackend.loadChunks('slots',[key],{chunkSize:1,intervalSeconds:width});
 assert.deepEqual(result.get(key)[0],healthy,'source accepts a finite index even when width is Infinity');
}
console.log('TS public slot options NaN/zero/overflow and finite-index Infinity-width cases pinned');
