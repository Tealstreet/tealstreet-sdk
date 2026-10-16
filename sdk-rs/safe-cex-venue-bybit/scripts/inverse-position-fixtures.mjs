import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
import iteration from 'p-iteration';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const source=ts.createSourceFile('inverse',readFileSync(new URL('exchanges/bybit/bybitinverse.exchange.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(source)==='mapPosition'));
const method=cls.members.find(m=>m.name?.getText(source)==='mapPosition');assert.ok(method?.body);
let clockCalls=0;
const context=vm.createContext({camelCase,snakeCase,memoize:f=>f,virtualClock:{getCurrentTimeMs:()=>{clockCalls++;return 123456789;}}});
const types=ts.createSourceFile('types',readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
for(const name of ['PositionSide','PositionMode']) {
 const declaration=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text===name);assert.ok(declaration);
 vm.runInContext(ts.transpileModule(declaration.getText(types).replace(/^export /,'')+`;globalThis.${name}=${name};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
}
const tableSource=ts.createSourceFile('tables',readFileSync(new URL('exchanges/bybit/bybit.types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const table=tableSource.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(d=>d.name.getText(tableSource)==='POSITION_SIDE');assert.ok(table);
vm.runInContext(ts.transpileModule(`globalThis.POSITION_SIDE=${table.initializer.getText(tableSource)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
for(const [path,names] of [['utils/get-key.ts',['s','c','v']],['utils/createPositionId.ts',['createPositionId']]]){
 const src=ts.createSourceFile(path,readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
 for(const name of names){const d=src.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(d=>d.name.getText(src)===name);assert.ok(d);vm.runInContext(ts.transpileModule(`globalThis.${name}=${d.initializer.getText(src)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
}
vm.runInContext(ts.transpileModule(`globalThis.run=function(p)${method.body.getText(source)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const base={symbol:'BTCUSD',size:'1000',avgPrice:'20000',positionIdx:0,side:'Buy',unrealisedPnl:'.1',positionValue:'.05',leverage:'5',curRealisedPnl:'.2',liqPrice:'10000',positionIM:'.01'};
const mark=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':'finite';
const cases=[];
const add=(row,inverse=true,ticker={mark:20000,last:19000},margin='cross',boundary=null)=>{
 clockCalls=0;const logs=[];let result,error;
 try{result=context.run.call({store:{marketsBySymbol:{BTCUSD:{isInverse:inverse}},tickersBySymbol:{BTCUSD:ticker}},marginType:margin,log(s){logs.push(s);this.store.tickersBySymbol={};}},row);}catch(e){error=e.message;}
 cases.push({row,inverse,ticker,margin,logs,result,error,boundary,clockCalls,sideKind:typeof result?.side,sideTruthy:Boolean(result?.side),classes:result&&[result.entryPrice,result.notional,result.leverage,result.unrealizedPnl,result.realizedPnl,result.contracts,result.liquidationPrice,result.marginSize??0].map(mark)});
};
for(const inverse of [false,true])for(const key of ['size','avgPrice','entryPrice','positionIdx','unrealisedPnl','positionValue','leverage','curRealisedPnl','liqPrice','positionIM'])for(const value of [undefined,null,false,true,0,-0,1,-1,'0','12tail','Infinity','-Infinity','NaN','\uFEFF12','\u008512',[],[12],{}, {toString:null}]){
 const row={...base};if(value===undefined)delete row[key];else row[key]=value;add(row,inverse);
}
for(const size of ['-.000000005','-.000000015','.000000005','.000000015','1000'])for(const ticker of [{mark:1,last:2},{mark:0,last:1},{mark:-1,last:1},null])add({...base,size},true,ticker);
for(const side of ['Sell','Buy','unknown','',null])add({...base,side,positionIdx:3});
add({...base,avgPrice:null,avg_price:'2',entryPrice:'3',unrealisedPnl:null,unrealised_pnl:'5'});
for(const positionIdx of [0,1,2,3])for(const side of [['Sell'],['Buy'],[],{}, {toString:null}, {toString:[]}])add({...base,positionIdx,side});
for(const key of ['constructor','__proto__','toString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','toLocaleString','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__']){
 add({...base,side:key},true,{mark:20000,last:19000},'cross','inherited-side');
 for(const positionIdx of [1,2])add({...base,side:key,positionIdx});
}
for(const symbol of [undefined,null,0,12,[],['BTCUSD'],{}]){const row={...base};if(symbol===undefined)delete row.symbol;else row.symbol=symbol;add(row,true,{mark:20000,last:19000},'cross','typed-symbol');}
for(const symbol of ['OTHER','constructor','__proto__','toString'])add({...base,symbol});
add(base,true,{mark:20000,last:19000},'isolated');
// v's third spelling matters for acronym-bearing fields: positionIM -> positionIm.
for (const value of ['.03', null, 0, '', 'NaN', '\uFEFF.04']) {
 const row={...base,positionIm:value};delete row.positionIM;add(row);
 add({...row,position_im:null}); // Present null stops before the camel alias.
 add({...row,positionIM:'.02',position_im:'.05'}); // Exact spelling wins.
}
const fanout = process.argv.includes('--fanout');
if (process.argv.includes('--composition') || fanout) {
 context.mapSeries=iteration.mapSeries;
 const endpoints=tableSource.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(d=>d.name.getText(tableSource)==='ENDPOINTS');
 assert.ok(endpoints);
 const evaluate=code=>vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 evaluate(`globalThis.ENDPOINTS=${endpoints.initializer.getText(tableSource)}`);
 for(const name of ['fetchPositionsHelper','reinitOnThrow','fetchPositions']) {
  const node=cls.members.find(m=>m.name?.getText(source)===name);assert.ok(node?.initializer);
  evaluate(`globalThis.make_${name}=function(){return ${node.initializer.getText(source)}}`);
 }
 for(const name of ['supportedCategories','getSettleCoinsForCategory']) {
  const node=cls.members.find(m=>m.name?.getText(source)===name);assert.ok(node?.body);
  evaluate(`globalThis.source_${name}=function(${name==='getSettleCoinsForCategory'?'category':''})${node.body.getText(source)}`);
 }
 const output=[];
 const page=rows=>({retMsg:'OK',result:{list:rows}});
 const addBatch=async(label,symbols,pages,plan={})=>{
  const requests=[],logs=[],mapped=[],actions=[],events=[];clockCalls=0;let index=0,result,error;
  const host={store:{marketsBySymbol:{BTCUSD:{isInverse:true}},tickersBySymbol:{BTCUSD:{mark:20000,last:19000}}},marginType:'cross',needsReinit:false,
   log:s=>logs.push(s),getCategoryForSymbol:()=> 'inverse',accountType:'UNIFIED',
   get supportedCategories(){actions.push('categories');return plan.categories??context.source_supportedCategories.call(this);},
   getSettleCoinsForCategory(category){actions.push(`settle:${category}`);if(plan.failCategory===category)throw new Error('settlement getter rejected');return plan.emptyCoins?[]:context.source_getSettleCoinsForCategory.call(this,category);},
   emitAndLogError(value){events.push(value);if(plan.reportError)throw new Error('reporter rejected');},
   xhr:{get:async(url,{params})=>{requests.push({url,params});actions.push(`request:${params.category}:${params.settleCoin??''}`);const data=pages[index++];assert.ok(data,'missing fixture page');if(data.requestError)throw new Error(data.requestError);return {data};}},
   mapPosition(row){const value=context.run.call(this,row);mapped.push({kind:typeof value.side,truthy:Boolean(value.side)});return value;}};
  for(const name of ['fetchPositionsHelper','reinitOnThrow','fetchPositions'])host[name]=context[`make_${name}`].call(host);
  try{result=fanout?await host.fetchPositions(symbols):await host.fetchPositionsHelper(symbols,'inverse','BTC');}catch(e){error={name:e.name,message:e.message};}
  output.push({label,symbols,pages,plan,actions,events,requests,logs,mapped,clockCalls,error,needsReinit:host.needsReinit,resultCount:result?.length,ids:result?.map(p=>p.id)});
 };
 if(fanout){
  for(const symbols of [undefined,[],['BTCUSD'],['BTCUSD','ETHUSD']]){
   await addBatch('ordinary:concrete',symbols,[page([base]),page([{...base,side:'Sell'}])]);
   await addBatch('constructor:concrete-later-null',symbols,[page([{...base,side:'constructor'},null]),page([base])]);
  }
  for(const key of Object.getOwnPropertyNames(Object.prototype)){
   const inherited={...base,side:key};
   const plan={categories:['linear','inverse']};
   await addBatch(`${key}:later-coin-null`,undefined,[page([inherited]),page([null])],plan);
   await addBatch(`${key}:later-category-null`,undefined,[page([inherited]),page([base]),page([null])],plan);
   await addBatch(`${key}:later-category-request-error`,undefined,[page([inherited]),page([base]),{requestError:'later request rejected'}],plan);
   await addBatch(`${key}:all-category-requests`,[],[page([inherited]),page([base]),page([base])],plan);
   await addBatch(`${key}:later-getter-error`,undefined,[page([inherited]),page([base])],{...plan,failCategory:'inverse'});
   await addBatch(`${key}:later-reporter-error`,undefined,[page([inherited]),{retMsg:'denied'}],{...plan,reportError:true});
  }
  await addBatch('ordinary:empty-categories',undefined,[],{categories:[]});
  await addBatch('ordinary:empty-coins',[],[],{categories:['linear','inverse'],emptyCoins:true});
  await addBatch('ordinary:duplicate-category',undefined,[page([base]),page([{...base,side:'Sell'}])],{categories:['inverse','inverse']});
  await addBatch('ordinary:symbols-bypass-getter', ['BTCUSD'],[page([base])],{categories:['inverse'],failCategory:'inverse'});
  await addBatch('constructor:nonfatal-response',undefined,[page([{...base,side:'constructor'}]),{retMsg:'denied'},page([base])],{categories:['linear','inverse']});
 } else {
 const keys=Object.getOwnPropertyNames(Object.prototype);
 for(const key of keys){
  const inherited={...base,side:key};
  await addBatch(`${key}:later-null`,undefined,[page([inherited,null])]);
  await addBatch(`${key}:later-coercion`,undefined,[page([inherited,{...base,leverage:{toString:null}}])]);
  await addBatch(`${key}:same-row-coercion`,undefined,[page([{...inherited,leverage:{toString:null}}])]);
  await addBatch(`${key}:retained-record`,undefined,[page([inherited,base])]);
  await addBatch(`${key}:later-symbol-null`,['BTCUSD','ETHUSD'],[page([inherited]),page([null])]);
  await addBatch(`${key}:later-symbol-request-error`,['BTCUSD','ETHUSD'],[page([inherited]),{requestError:'later request rejected'}]);
  await addBatch(`${key}:all-requests`,['BTCUSD','ETHUSD','SOLUSD'],[page([inherited]),page([base]),page([base])]);
  await addBatch(`${key}:source-error-stops-requests`,['BTCUSD','ETHUSD'],[page([inherited,null]),page([base])]);
 }
 for(const [index,symbol] of [undefined,null,0,12,[],['BTCUSD'],{}].entries()) {
  const row={...base,symbol};
  await addBatch(`symbol-${index}:later-null`,undefined,[page([row,null])]);
  await addBatch(`symbol-${index}:same-row-coercion`,undefined,[page([{...row,leverage:{toString:null}}])]);
  await addBatch(`symbol-${index}:all-requests`,['BTCUSD','ETHUSD'],[page([row]),page([base])]);
  await addBatch(`symbol-${index}:later-symbol-request-error`,['BTCUSD','ETHUSD'],[page([row]),{requestError:'later request rejected'}]);
 }
 await addBatch('ordinary',undefined,[page([base,{...base,side:'Sell'}])]);
 }
 console.log(JSON.stringify(output));
} else console.log(JSON.stringify(cases));
