import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=['nativePositionId','findOpenPosition','formatChangeLeveragePayload','planOrderTriggerType','stopOrderTrend','isRegularPlaceOrder','getReplacementOrderAmount','isStopLikeOrder','isStopOrder','isPlanOrder','orderListEndpoint'];
const selected=[];
for(const node of ast.statements) {
 if(ts.isVariableStatement(node) && node.declarationList.declarations.some(d=>['extractData','extractRows','MEXC_TRIGGER_GTE','MEXC_TRIGGER_LTE','MEXC_TRIGGER_TREND_LATEST_PRICE'].includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name.text==='MexcExchange') {
  for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)} = ${member.initializer.getText(ast)};`);
 }
}
const bindings={...load('packages/safe-cex/src/types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts'),...load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts'),...load('packages/safe-cex/src/utils/omitUndefined.ts')};
const context=vm.createContext(bindings);
let selectedSource=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='positive-open-position') {
 const before='position.contracts > 0';
 if(!selectedSource.includes(before))throw Error('missing findOpenPosition mutation anchor');
 selectedSource=selectedSource.replace(before,'position.contracts >= 0');
}
if(process.env.MEXC_TS_CONTROL==='plan-classification'){
 const before='this.isStopLikeOrder(order) && !this.isStopOrder(order)';
 if(!selectedSource.includes(before))throw Error('missing plan classification mutation anchor');
 selectedSource=selectedSource.replace(before,'this.isStopLikeOrder(order)');
}
if(process.env.MEXC_TS_CONTROL==='leverage-payload-open-type'){
 const before='options.includeOpenTypeWithPositionId ? openType : undefined';
 if(!selectedSource.includes(before))throw Error('missing leverage payload optional-open-type mutation anchor');
 selectedSource=selectedSource.replace(before,'openType');
}
const js=ts.transpileModule(selectedSource+'\nthis.extractData=extractData;this.extractRows=extractRows;',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInContext(js,context,{filename});
const number=n=>{if(Number.isNaN(n))return {$number:'7ff8000000000000'};const b=Buffer.alloc(8);b.writeDoubleBE(n);return {$number:b.toString('hex')};};
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?number(v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const cases=[];const run=(name,args,extra={})=>{try{if(name==='formatChangeLeveragePayload')context.store={positions:args[6]};const result=context[name](...args);cases.push({name,args:encode(args),...extra,output:encode(name==='findOpenPosition'?result?.id:result)});}catch(e){cases.push({name,args:encode(args),...extra,error:{name:e.name,message:e.message}});}};
for(const payload of [undefined,null,{},[],false,0,'', {data:null},{data:false},{data:[]},{data:{orders:[1,2],items:[3]}},{data:{data:{orders:[4]},items:[5]}},{orders:[1],data:{orders:[2]}},{data:{data:{data:[3]}}}]) {
 run('extractData',[payload]);for(const keys of [[],['orders','items'],['items','orders']])run('extractRows',[payload,keys]);
}
for(const info of [undefined,null,{}, {id:0},{positionId:null,id:'123'}, {positionId:'',id:'fallback'},{positionId:{toString:null}},{id:'😃'}])run('nativePositionId',[{info}]);
for(const type of Object.values(bindings.OrderType)) {
 for(const side of ['buy','sell'])run('planOrderTriggerType',[{type,side}]);
 run('isRegularPlaceOrder',[{type}]);
 for(const tpTriggerType of [undefined,'mark_price','index_price','last_price'])for(const slTriggerType of [undefined,'mark_price','index_price'])run('stopOrderTrend',[{type,tpTriggerType,slTriggerType}]);
 for(const info of [undefined,{}, {__mexcPlanOrder:true},{__mexcStopOrder:true},{__mexcPlanOrder:'false'},{__mexcStopOrder:0}])for(const amount of [0,2]) {
  const order={type,info,amount,remaining:0};for(const name of ['isStopLikeOrder','isStopOrder','isPlanOrder'])run(name,[order]);
 }
}
for(const remainingNative of [undefined,0,-0,-1,3,NaN,Infinity])for(const info of [{},{vol:0},{vol:4},{vol:null,quantity:5},{origQty:6}])run('getReplacementOrderAmount',[{amount:7,extra:{remainingNative},info}]);
for(const symbol of [undefined,'','BTC_USDT','a?b','😃'])run('orderListEndpoint',[symbol]);
for(const key of ['0','00','-0','__proto__'])run('extractRows',[{data:{data:[[1,2]],items:[7]}},[key,'items']]);
for(const symbol of ['BTCUSDT','ETHUSDT'])for(const side of ['long','short'])for(const contracts of [-1,-0,0,NaN,Infinity,1]) {
 const otherSymbol=symbol==='BTCUSDT'?'ETHUSDT':'BTCUSDT';
 const otherSide=side==='long'?'short':'long';
 const rows=[
  {id:'wrong-symbol',symbol:otherSymbol,side,contracts:5},
  {id:'wrong-side',symbol,side:otherSide,contracts:5},
  {id:'candidate',symbol,side,contracts},
  {id:'later-positive',symbol,side,contracts:2},
 ];
 run('findOpenPosition',[symbol,side,rows]);
}
run('findOpenPosition',['BTCUSDT','long',[]]);
run('findOpenPosition',['BTCUSDT','long',[
 {id:'zero-match',symbol:'BTCUSDT',side:'long',contracts:0},
 {id:'wrong-side-last',symbol:'BTCUSDT',side:'short',contracts:5},
]]);
const marketRaw={symbol:'BTC_USDT',contractSize:0.0001,volUnit:3,minVol:2,maxVol:1000,priceScale:5,minLeverage:4,maxLeverage:100};
const market=bindings.normalizeMarket(marketRaw);
const position=(id,symbol,side,contracts,info)=>({id,symbol,side,contracts,info,entryPrice:1,notional:10,leverage:20,unrealizedPnl:0,liquidationPrice:5});
const scenarios=[
 [],
 [position('wrong-symbol','ETHUSDT','long',5,{positionId:'11'})],
 [position('wrong-side','BTCUSDT','short',5,{positionId:'12'})],
 [position('zero','BTCUSDT','long',0,{positionId:'13'}),position('match','BTCUSDT','long',2,{positionId:'14'})],
 [position('null-id','BTCUSDT','short',1,{positionId:null,id:'15'})],
 [position('zero-id','BTCUSDT','long',1,{positionId:0,id:'16'})],
 [position('empty-id','BTCUSDT','long',1,{positionId:'',id:'17'})],
 [position('first-positive','BTCUSDT','long',3,{positionId:'18'}),position('second-positive','BTCUSDT','long',2,{positionId:'19'})],
 [position('wrong-first','ETHUSDT','long',4,{positionId:'20'}),position('matching-positive','BTCUSDT','long',1,{positionId:'21'})],
];
for(const positionType of [1,2,3])for(const marginType of ['isolated','cross'])for(const include of [false,true])for(const rows of scenarios) {
 run('formatChangeLeveragePayload',['BTCUSDT',market,positionType,37,marginType,{includeOpenTypeWithPositionId:include},rows],{marketRaw});
}
for(const leverage of [-0,NaN,Infinity,-Infinity]) {
 run('formatChangeLeveragePayload',['BTCUSDT',market,1,leverage,'isolated',{includeOpenTypeWithPositionId:false},[position('nonfinite-leverage','BTCUSDT','long',1,{positionId:'22'})]],{marketRaw});
}
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
