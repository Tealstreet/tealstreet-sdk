import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
const income=loadSource(new URL('../../../packages/safe-cex/src/exchanges/okx/okx.income.ts',import.meta.url));
const cases=[];
for(const key of Object.getOwnPropertyNames(Object.prototype)) {
  const member=key==='__proto__'?Object.prototype:Object.prototype[key];
  const category=income.okxBillCategory(key);
  assert.equal(category,member,'income::source category identity before wire');
  assert.equal(typeof category,key==='__proto__'?'object':'function','income::source category type');
  for(const [mode,amounts] of [['empty',[0,0,0]],['fee',[-1,-1,0]],['pnl',[1,0,1]],['adj',[1,0,0]],['error',[1,0,1]]]) {
    const row={ts:'123',instId:'BTC-USDT-SWAP',ccy:mode==='error'?{toString:null}:'USDT',billId:'bill',type:key,balChg:String(amounts[0]),fee:String(amounts[1]),pnl:String(amounts[2])};
    let calls=0,records,error;
    try {records=income.mapOkxBillToIncomeRecords(row,()=>{calls++;return 'BTCUSDT';});}catch(e){error={name:e.name,message:e.message};}
    assert.equal(calls,1,'income::source resolver precedes closed category output');
    if(mode==='pnl'||mode==='adj')assert.equal(records[0].category,member,'income::source record category identity');
    if(mode==='empty')assert.equal(records.length,0,'income::source empty inherited bill');
    if(mode==='fee')assert.equal(records[0].category,'commission','income::source fee bypasses inherited category');
    if(mode==='error')assert.equal(error.name,'TypeError','income::source coercion precedes category');
    cases.push({key,type:typeof category,identity:category===member,mode,row,calls,records,error});
  }
}
assert.equal(cases.length,60,'income::source inherited floor');
process.stdout.write(JSON.stringify(cases));
