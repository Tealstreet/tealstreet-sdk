// Live capture architecture: process.arch; recorded sweep baseline darwin arm64. NaN payload only is canonicalized.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const {roundUSD}=loadSource(new URL('../../../packages/safe-cex/src/utils/roundUSD.ts',import.meta.url));
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts',import.meta.url);
let source=fs.readFileSync(url,'utf8');
if(process.argv.includes('--mutate-volume'))source=source.replace('volume: parseFloat(update.volCcy24h)','volume: parseFloat(update.vol24h)');
if(process.argv.includes('--mutate-source'))source=source.replace('openInterest: parseFloat(update.oiCcy)','openInterest: parseFloat(update.fundingRate)');
if(process.argv.includes('--mutate-order')) {
 const before='this.parent.markPriceLoaded(update.instId);\n    this.store.updateTicker({ id: update.instId }, { mark: parseFloat(update.markPx) });';
 const after='this.store.updateTicker({ id: update.instId }, { mark: parseFloat(update.markPx) });\n    this.parent.markPriceLoaded(update.instId);';
 assert.ok(source.includes(before),'source order mutation must apply');source=source.replace(before,after);
}
const ast=ts.createSourceFile(url.pathname,source,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='OKXPublicWebsocket');
const names=['handleOpenInterestEvents','handleFundingRateEvents','handleMarkPriceEvents','handleIndexTickerEvents','handleTickerEvents'];
const members=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(n,name);return n.getText(ast);});
const module={exports:{}};
vm.runInThisContext('(function(module,exports,roundUSD){'+ts.transpileModule('export class Selected{'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'})')(module,module.exports,roundUSD);
const bits=n=>{
 if(Number.isNaN(n))return '7ff8000000000000';
 if(process.argv.includes('--mutate-ignore-signed-zero')&&Object.is(n,-0))n=0;
 if(process.argv.includes('--mutate-ignore-finite')&&Number.isFinite(n))n=0;
 const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');
};
assert.equal(bits(1.25),'3ff4000000000000','capture finite payload identity');
assert.equal(bits(-0),'8000000000000000','capture signed zero identity');
assert.equal(bits(-NaN),'7ff8000000000000','capture canonical NaN only');
const values=['1.25','0.3333','-0.0001','-0','1e400','-1e400','-1e-4000','0x10','12abc','','\uFEFF12','\u008512',null,0,true,[],['12'],{}, {toString:null}];
const cases=[];
const assertionExecutions=[0,0,0];
for(const [method,field] of [[names[0],'oiCcy'],[names[1],'fundingRate'],[names[1],'fundingTime'],[names[2],'markPx'],[names[3],'idxPx'],...['open24h','last','bidPx','askPx','volCcy24h','vol24h'].map(field=>[names[4],field])])for(const value of values){
 const update={instId:'BTC',oiCcy:'321',fundingRate:'0.125',fundingTime:'1700000000123',markPx:'999',idxPx:'888',open24h:'2',last:'3',bidPx:'4',askPx:'5',volCcy24h:'6',vol24h:'7'};update[field]=value;
 const input={data:[update,{instId:'BTC',oiCcy:'999',fundingRate:'999',fundingTime:'999'}]};
 const calls=[],trace=[];const receiver=new module.exports.Selected();receiver.parent={markPriceLoaded:id=>trace.push('loaded:'+id),indexPriceLoaded:id=>trace.push('loaded:'+id),getMarketsForIndexTickerInstId:id=>{trace.push('select:'+id);return [{id:'ETH'},{id:'BTC'}];}};receiver.store={updateTicker:(id,patch)=>{trace.push('write:'+id.id);calls.push({id:id.id,patch:Object.fromEntries(Object.entries(patch).map(([k,v])=>[k,bits(v)]))});}};
 let error=null;try{receiver[method](input);}catch(e){error=e.name;}
 if(method===names[2]){assertionExecutions[0]++;assert.equal(trace[0],'loaded:BTC','source mark loaded before write and before throwing coercion');}
 if(method===names[3]&&!error){assertionExecutions[1]++;assert.deepEqual(trace,['select:BTC','loaded:ETH','write:ETH','loaded:BTC','write:BTC'],'source per-market loaded before its write');}
 if(method===names[3]&&error){assertionExecutions[2]++;assert.deepEqual(trace,[],'source index coercion precedes market selection');}
 cases.push({method,input,calls,error,trace});
}
assert.equal(cases[0].calls[0].patch.openInterest,bits(1.25),'source oiCcy field must decide openInterest');
const ticker=cases.find(c=>c.method==='handleTickerEvents'&&c.input.data[0].open24h==='1.25');
assert.equal(ticker.calls[0].patch.volume,bits(6),'source volume currency units must not swap');
assert.equal(ticker.calls[0].patch.quoteVolume,bits(7),'source quoteVolume contract units must not swap');
assert.deepEqual(assertionExecutions,[19,18,1],'TICKER-BRANCH assertion executions');
assert.equal(cases.length,209);process.stdout.write(JSON.stringify(cases));
