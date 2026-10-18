import assert from 'node:assert/strict';
import {sourceStore} from '../../../safe-cex-store/tests/store_source.mjs';
const {Store} = sourceStore();
const base = {
 market:{id:'m',symbol:'BTC',base:'BTC',quote:'USDT',active:true,precision:{amount:0.001,price:0.1},limits:{amount:{min:0.001,max:100},leverage:{min:1,max:100}}},
 order:{id:'o',symbol:'BTC',status:'open',type:'limit',side:'buy',price:3,amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:1},
 position:{id:'p',symbol:'BTC',side:'long',entryPrice:3,notional:6,leverage:1,unrealizedPnl:0,contracts:2,liquidationPrice:0},
 balance:{used:0,free:10,total:10,upnl:0},
 ticker:{id:'t',symbol:'BTC',bid:1,ask:2,last:3,mark:4,index:5,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0},
};
const profiles = [
 ['absent',undefined],['undefined',undefined],['null',null],['nan',NaN],['infinity',Infinity],['negativeInfinity',-Infinity],
 ['negativeZero',-0],['zero',0],['false',false],['text','\ud800'],['array',[undefined,NaN,-0,'\udfff']],
 ['object',{nan:NaN,infinity:Infinity,zero:-0,ownUndefined:undefined,'\udfff':'\ud800','$serde_json::private::Number':'17'}],['emptyObject',{}],
];
function native(value) {
 if(value === undefined) return {kind:'undefined'};
 if(value === null) return {kind:'null'};
 if(typeof value === 'boolean') return {kind:'bool',value};
 if(typeof value === 'number') {const data=new DataView(new ArrayBuffer(8));data.setFloat64(0,value);return {kind:'number',bits:data.getBigUint64(0).toString(16)};}
 if(typeof value === 'string') return {kind:'text',units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};
 if(Array.isArray(value)) return {kind:'array',values:value.map(native)};
 return {kind:'object',entries:Object.keys(value).map(key=>({key:native(key).units,value:native(value[key])}))};
}
const info = row => ({own:Object.hasOwn(row,'info'),value:native(row.info),wire:JSON.stringify(row.info)??null});
const cases=[];
for(const [model, original] of Object.entries(base)) {
 for(const [profile,value] of profiles) {
  const row={...original};if(profile !== 'absent') row.info=value;
  const input=info(row),store=new Store();
  const collection=model==='balance'?'balance':`${model}s`;
  store.update({[collection]:model==='balance'?row:[row]});
  const stored=model==='balance'?store.balance:store[collection][0];
  cases.push({model,profile,base:original,input,stored:info(stored)});
 }
}
const patches=[];
for(const operation of ['order','position','positions','market']) {
 for(const [profile,value] of profiles) {
  const model=operation==='positions'?'position':operation,store=new Store();
  store.update({[`${model}s`]:[{...base[model]}]});
  const patch={};if(profile!=='absent') patch.info=value;
  if(operation==='positions') store.updatePositions([[{id:'p'},patch]]);
  else store[`update${model[0].toUpperCase()}${model.slice(1)}`]({id:base[model].id},patch);
  patches.push({operation,model,profile,base:base[model],patch:info(patch),stored:info(store[`${model}s`][0])});
 }
}
assert.equal(cases.length,65);assert.equal(patches.length,52);
const coercions=[];
const conversionProfiles=[...profiles,['numericText',' -0 '],['prefixText','12.5x'],['prefixSurrogate','12.5\ud800'],['hexText','0x10'],['arrayNumber',[12.5]],['arrayNull',[null]],['throwingObject',{toString:null}]];
for(const [profile,value] of conversionProfiles) {
 const result={profile,value:native(value)};
 for(const [name,convert] of [['number',Number],['parseFloat',parseFloat],['string',String]]) {
  try {result[name]={value:native(convert(value))};}
  catch(error) {result[name]={error:{name:error.name,message:error.message}};}
 }
 coercions.push(result);
}
assert.equal(coercions.length,20);
process.stdout.write(JSON.stringify({cases,patches,coercions}));
