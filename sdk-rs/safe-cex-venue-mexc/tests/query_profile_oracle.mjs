import assert from 'node:assert/strict';
import {load} from './source.mjs';
const {serializeMexcParams,getQueryString}=load('packages/safe-cex/src/exchanges/mexc/mexc.api.ts');
const values=Array.from({length:11},(_,i)=>'v'+i);
const roots=[
 ['nonfinite',{symbol:'BTC_USDT',start_time:NaN,end_time:-Infinity,page_num:1,page_size:100},'end_time=-Infinity&page_num=1&page_size=100&start_time=NaN&symbol=BTC_USDT'],
 ['nested-array',{z:values},'z=v0&z=v1&z=v10&z=v2&z=v3&z=v4&z=v5&z=v6&z=v7&z=v8&z=v9'],
 ['root-array',values,'0=v0&1=v1&10=v10&2=v2&3=v3&4=v4&5=v5&6=v6&7=v7&8=v8&9=v9'],
 ['nested-object',{z:{b:2,a:1},a:'first'},'a=first&z%5Ba%5D=1&z%5Bb%5D=2'],
 ['empty-and-scalars',{b:null,u:undefined,z:[undefined,null,false,-0,'-0'],empty:{},a:[]},'b=&z=&z=false&z=0&z=-0'],
];
const output=[];let calls=0;
for(const[label,input,want]of roots){
 const keysBefore=Object.keys(input);
 const rendered=serializeMexcParams(input);calls++;
 assert.equal(rendered,want,'SOURCE actual qs recursive array/object sorting '+label);
 const viaSigner=getQueryString({params:input,url:'/query?ignored=1'});calls++;
 assert.equal(viaSigner,want,'SOURCE actual signer parameter query '+label);
 assert.deepEqual(Object.keys(input),keysBefore,'SOURCE rendering retains producer key order');
 if(label==='nonfinite'){
  assert(Number.isNaN(input.start_time),'SOURCE retained native NaN after query render');
  assert.equal(input.end_time,-Infinity,'SOURCE retained native infinity after query render');
 }
 output.push({label,rendered,keys:keysBefore});
}
assert.equal(calls,10,'SOURCE independent serializer/signer call floor');
assert.equal(output.length,5,'SOURCE independent rich-query root floor');
console.log(JSON.stringify(output));
