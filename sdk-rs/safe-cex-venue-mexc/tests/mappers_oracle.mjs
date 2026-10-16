import {load} from './source.mjs';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const number=value=>{if(Number.isNaN(value))return {$number:'7ff8000000000000'};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {$number:b.toString('hex')};};
const encode=value=>value===undefined?{$undefined:true}:typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const cases=[];
const marketRaw={symbol:'BTC_USDT',contractSize:0.0001,volUnit:3,minVol:2,maxVol:100000,priceScale:5,minLeverage:4,maxLeverage:100};
function run(name,raw,market=marketRaw,mode=undefined){
 // The wire object has absent keys, not own properties containing undefined.
 raw=Object.fromEntries(Object.entries(raw).filter(([,v])=>v!==undefined));
 try {const result=api[name](raw,market===undefined?undefined:api.normalizeMarket(market),mode);cases.push({name,raw:encode(raw),marketRaw:market,mode:encode(mode),output:encode(result)});}
 catch(error){cases.push({name,raw:encode(raw),marketRaw:market,mode:encode(mode),error:{name:error.name,message:error.message}});}
}
const names=['normalizePosition','normalizeOrder','normalizePlanOrder','normalizeStopOrder','normalizeExecution'];
const base={symbol:'BTC_USDT',id:'42',orderId:'43',side:1,positionType:2,vol:5,holdVol:7,price:123,createTime:'2024-01-02T03:04:05Z'};
for(const name of names) {
 for(const raw of [{},base,{...base,symbol:null,contract:'ETH_USDC'},{...base,symbol:''},
  {...base,id:0,orderId:44},{...base,side:'constructor',positionType:'__proto__'},
  {...base,createTime:0},{...base,createTime:'2024-03-10T02:30:00'},
  {...base,vol:'1e308',holdVol:'5e307',price:'2e307',holdAvgPrice:'3e307'},
  {...base,vol:-3,holdVol:-4},{...base,vol:'-0',dealVol:'2',holdVol:'3'},
  {...base,vol:'2',dealVol:'-0',holdVol:'3'},
  {...base,vol:'2',dealVol:'3',holdVol:'-0'},{...base,side:{toString:null}},
  {...base,updateTime:{toString:null},createTime:{valueOf:null}}])run(name,raw);
 for(const side of [0,1,2,3,4,5,'2abc',null,'toString'])run(name,{...base,side,positionType:side===1?2:1});
}
for(const state of [undefined,null,0,1,2,3,4,5,6,'2bad'])for(const name of ['normalizeOrder','normalizePlanOrder','normalizeStopOrder'])run(name,{...base,state});
for(const type of [undefined,null,0,1,2,3,4,5,6,'5bad'])for(const reduceOnly of [false,true,'false',0,{}])run('normalizeOrder',{...base,type,reduceOnly,dealVol:7,price:0,triggerPrice:321});
for(const volType of [undefined,null,1,2,'2abc'])for(const triggerSide of [undefined,1,2])run('normalizeStopOrder',{...base,volType,triggerSide,takeProfitPrice:12,stopLossPrice:8});
for(const triggerType of [undefined,1,2,3])for(const side of [1,2,3,4])run('normalizePlanOrder',{...base,triggerType,side});
for(const positionMode of [undefined,null,0,1,2,'one-way'])for(const accountMode of ['hedged','oneway'])run('normalizePosition',{...base,positionMode,leverage:0,im:undefined,holdAvgPrice:5},marketRaw,accountMode);
for(const role of [undefined,null,0,1,2,'1bad'])for(const taker of ['sell','SELL','buy',false,{}])run('normalizeExecution',{...base,side:0,taker,role,fee:-2,feeCurrency:'USDT',profit:3});
run('normalizePosition',{...base,leverage:4,holdAvgPrice:50});
for(const feeAsset of [null,false,0,{},['USDT']])run('normalizeExecution',{...base,feeAsset});
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
