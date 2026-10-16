const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Subject,setNow,setBeforeNow}=require('./exchange-state.cjs');
const s=new Subject();setNow(42);setBeforeNow(()=>s.suppressCancelledOrder('nested'));
s.suppressCancelledOrder('outer');
const result={source:'actual suppressCancelledOrder under one reentrant host clock callback',nested:s.isOrderCancelled('nested'),outer:s.isOrderCancelled('outer'),timestamps:Array.from(s._cancelledOrders)};
assert.equal(result.nested,true);assert.equal(result.outer,true);
const output=JSON.stringify(result,null,2)+'\n',target=path.join(__dirname,'../tests/fixtures/exchange-state-reentry.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'),'source clock reentry drift');
