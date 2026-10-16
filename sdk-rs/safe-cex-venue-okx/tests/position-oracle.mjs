import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
const {mapOkxPositionHistory,num} = loadSource(new URL('../../../packages/safe-cex/src/journal/okx.position-adapter.ts', import.meta.url),['num']);
const cases = [];
const reasonCodes = ['1','2','3','4','5','unknown','constructor','toString','valueOf','hasOwnProperty','__proto__'];
for (const type of reasonCodes) for (const posSide of ['net','long','short']) for (const fee of ['-0.0098889','0.0098889','0']) for (const fundingFee of ['0.002','-0.002']) {
  const row = {instId:'DOGE-USDC-SWAP',direction:'short',uTime:'1735689600000',cTime:'1735603200000',closeTotalPos:'0.42',openMaxPos:'0.28',fee,fundingFee,realizedPnl:'-0.0112889',posSide,posId:'42',ccy:'',lever:'50',openAvgPx:'0.08',closeAvgPx:'0.07168',type};
  const calls = [];
  const deps = {account:'replay',toBase:(n,id)=>{calls.push(['base',n,id]);return n*1000;},resolveSymbol:id=>{calls.push(['symbol',id]);return 'DOGEUSDC';},resolveSettleCurrency:id=>{calls.push(['settle',id]);return 'USDC';}};
  const record = mapOkxPositionHistory(row,deps);
  cases.push({row,known:true,scale:1000,record,calls});
}
for (const known of [false,true]) for (const closeTotalPos of ['0','NaN','-0.42','0.14suffix']) for (const posId of [null,'',0,'42']) {
  const row = {instId:'DOGE-USDC-SWAP',direction:'long',uTime:0,cTime:'NaN',closeTotalPos,openMaxPos:'0',posSide:'net',posId,ccy:'USD',openAvgPx:'0',fee:'0.01'};
  const calls = [];
  const deps = {account:'replay',toBase:(n,id)=>{calls.push(['base',n,id]);return n*1000;},resolveSymbol:id=>{calls.push(['symbol',id]);return known?'DOGEUSDC':undefined;},resolveSettleCurrency:id=>{calls.push(['settle',id]);return 'USDC';}};
  cases.push({row,known,scale:1000,record:mapOkxPositionHistory(row,deps),calls});
}
const byType = cases.filter(x=>x.row.type==='constructor');
assert.equal(byType.length,18,'POSITION-FILTER constructor executions');
assert.ok(byType.every(x=>typeof x.record.closeReason==='function'),'ts-bug: inherited closeReason changed');
const byPrototype = cases.filter(x=>x.row.type==='__proto__');
assert.equal(byPrototype.length,18,'POSITION-FILTER prototype executions');
assert.ok(byPrototype.every(x=>x.record.closeReason===Object.prototype));
assert.equal(num('\uFEFF1.25suffix'),1.25);
assert.equal(cases.length,230);
const seed={instId:'DOGE-USDC-SWAP',direction:'short',uTime:'1735689600000',cTime:'1735603200000',closeTotalPos:'0.42',openMaxPos:'0.28',fee:'-0.01',fundingFee:'0.002',realizedPnl:'1',posSide:'net',posId:'42',ccy:'USDC',lever:'50',openAvgPx:'0.08',closeAvgPx:'0.07',type:'1'};
for(const field of Object.keys(seed)) for(const bad of [{toString:null},{toString:'12'},{toString:false,valueOf:1},{toString:{}},[{toString:null}]]) {
 const row={...seed,[field]:bad};let error;
 try{mapOkxPositionHistory(row,{account:'replay',toBase:n=>n*1000,resolveSymbol:()=> 'DOGEUSDC',resolveSettleCurrency:()=> 'USDC'});}catch(thrown){error=thrown;}
 assert.equal(error?.name,'TypeError',`position.${field}`);
 cases.push({row,known:true,scale:1000,error:{name:error.name,message:error.message}});
}
process.stdout.write(JSON.stringify(cases));
