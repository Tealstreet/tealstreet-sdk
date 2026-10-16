import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const read=path=>ts.createSourceFile(path,readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
const run=(code,context)=>vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const variables=(source,names,context)=>{for(const name of names){const d=source.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(source)===name);assert.ok(d?.initializer,name);run(`globalThis.${name}=${d.initializer.getText(source)}`,context);}};
const cases=[];
const mark=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':'finite';
for(const variant of ['bybitspot','bybitinverse']) {
 let calls=0;
 const context=vm.createContext({camelCase,snakeCase,memoize:f=>f,virtualClock:{getCurrentTimeMs:()=>123456789+calls++}});
 const types=read('types.ts');
 for(const name of ['OrderType','OrderSide','OrderStatus','OrderTimeInForce','OrderStopTriggerType']){const d=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name);assert.ok(d,name);run(d.getText(types).replace(/^export /,'')+`;globalThis.${name}=${name}`,context);}
 variables(read(`exchanges/bybit/${variant==='bybitspot'?'bybitspot':'bybit'}.types.ts`),['ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','ORDER_TIME_IN_FORCE','ORDER_TRIGGER_TYPE'],context);
 variables(read('exchanges/bybit/bybit.types.ts'),['POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE'],context);
 variables(read('utils/get-key.ts'),['s','c','v'],context);
 const math=read('utils/safe-math.ts');
 // Run the live cache initialization plus only the called arithmetic helpers.
 const cache=math.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(math)==='POW10_CACHE'));assert.ok(cache);
 const loop=math.statements.find(ts.isForStatement);assert.ok(loop);
 run(cache.getText(math)+loop.getText(math),context);
 variables(math,['pow10','afterDecimal','adjust','subtract'],context);
 const source=read(`exchanges/bybit/${variant}.exchange.ts`);
 const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(source)==='mapOrder'));assert.ok(cls);
 const method=cls.members.find(m=>m.name?.getText(source)==='mapOrder');
 run(`globalThis.map=function(o)${method.body.getText(source)}`,context);
 if(variant==='bybitinverse'){
  variables(source,['BYBIT_POSITION_TPSL_MODES','isBybitPositionTpslMode'],context);
  const helper=cls.members.find(m=>m.name?.getText(source)==='remainingInStoreUnits');assert.ok(helper?.initializer);
  run(`globalThis.remainingInStoreUnits=${helper.initializer.getText(source)}`,context);
 }
 const base={orderId:'order-1',symbol:'BTCUSD',orderType:'Limit',side:'Buy',orderStatus:'New',price:'20000',qty:'1000',cumExecQty:'3',timeInForce:'GTC',reduceOnly:false};
 const add=(row,marketInverse=false)=>{
  calls=0;let result,error,classes;
  try{result=context.map.call({store:{marketsBySymbol:{BTCUSD:{isInverse:marketInverse}}},remainingInStoreUnits:context.remainingInStoreUnits},row);classes=result.map(o=>[o.price,o.amount,o.filled,o.remaining,o.extra.remainingNative,o.extra.remainingNotional??0].map(mark));}catch(e){error=e.message;}
  cases.push({variant,row,marketInverse,result,error,classes,clockCalls:calls});
 };
 for(const marketInverse of [false,true])for(const mode of [undefined,'Full','Partial','other'])for(const stopOrderType of [undefined,'UNKNOWN','StopLoss','TakeProfit','TrailingStop'])for(const createType of [undefined,'CreateByTakeProfit','CreateByStopLoss','CreateByStopOrder'])for(const reduceOnly of [false,true]){
  add({...base,tpslMode:mode,stopOrderType,createType,triggerPrice:'18000',reduceOnly,closeOnTrigger:false,stopLoss:'17000',takeProfit:'23000',triggerBy:'MarkPrice',slTriggerBy:'IndexPrice',tpTriggerBy:'LastPrice'},marketInverse);
 }
 const values=[undefined,null,false,true,0,-0,1,-1,'0','12tail','Infinity','-Infinity','NaN','\uFEFF12','\u008512',[],[12],{}, {toString:null}];
 for(const key of ['leavesQty','price','stopLoss','takeProfit','triggerPrice'])for(const value of values){const row={...base,stopOrderType:'StopLoss'};if(value===undefined)delete row[key];else row[key]=value;add(row,true);}
 for(const key of ['qty','cumExecQty'])for(const value of [undefined,null,0,-1,'','0','1.2300','1e-8','12tail','Infinity','NaN']){const row={...base};if(value===undefined)delete row[key];else row[key]=value;add(row,true);}
 for(const key of ['orderType','side','orderStatus','timeInForce','triggerBy'])for(const value of Object.keys(context[({orderType:'ORDER_TYPE',side:'ORDER_SIDE',orderStatus:'ORDER_STATUS',timeInForce:'ORDER_TIME_IN_FORCE',triggerBy:'ORDER_TRIGGER_TYPE'})[key]])){add({...base,[key]:value,stopLoss:'2',takeProfit:'3'});}
 for(const leavesQty of ['-.000000005','-.000000015','.000000005','.000000015'])add({...base,leavesQty,price:'1',stopLoss:'2',takeProfit:'3'},true);
}
console.log(JSON.stringify(cases));
