import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(new URL('../../../packages/safe-cex/src/exchanges/binance/binance.orderbook.ts',import.meta.url),'utf8');
const exports = {};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:id=>{assert.equal(id,'../../utils/updateOrderBook');return {updateOrderBook(){throw Error('unexpected update');}};}});
const text=n=>({value:String(n),negativeZero:Object.is(n,-0)});
const book=b=>Object.fromEntries(Object.entries(b).map(([side,rows])=>[side,rows.map(row=>Object.fromEntries(Object.entries(row).map(([key,n])=>[key,text(n)])))]));
const numbers=['','0','-0','1','-2','1.5','1e21','0x10','12tail','Infinity','-Infinity','NaN','\ufeff12x','\u008512x','1e-999','1e999'];
const inputs=[[],[[]],[['1']],[['1','2','extra']],[['2','3'],['1','5'],['2','7']]];
for(const price of numbers) for(const amount of numbers) inputs.push([[price,amount]]);
const rows=[];
for(const bids of inputs) for(const side of ['bids','asks']) for(const id of [0,1,-1,1.5,123,9007199254740991]) {
 const snapshot={bids:[],asks:[],lastUpdateId:id};snapshot[side]=bids;
 const orderBook={bids:[{price:9,amount:9,total:9}],asks:[{price:8,amount:8,total:8}]};
 const state={snapshotLastUpdateId:1,lastAppliedUpdateId:99};
 const dirty={...state}; exports.resetBinanceOrderBookSyncState(dirty);
 exports.loadBinanceOrderBookSnapshot(orderBook,snapshot,state);
 const expected={book:book(orderBook),state:{...state}};
 exports.resetBinanceOrderBookSyncState(state);
 rows.push({snapshot,expected,reset:{...state},resetDirty:{...dirty}});
}
process.stdout.write(JSON.stringify(rows));
