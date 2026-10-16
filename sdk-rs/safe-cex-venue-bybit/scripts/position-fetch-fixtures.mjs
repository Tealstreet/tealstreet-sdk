import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const read=path=>ts.createSourceFile(path,readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
const source=read('exchanges/bybit/bybitinverse.exchange.ts');
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(source)==='fetchPositionsHelper'));assert.ok(cls);
const run=(code,context)=>vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const context=vm.createContext({camelCase,snakeCase,memoize:f=>f,mapSeries:async (rows,fn)=>{const result=[];for(const row of rows)result.push(await fn(row));return result;}});
for(const [path,names] of [['utils/get-key.ts',['s','c','v']],['exchanges/bybit/bybit.types.ts',['ENDPOINTS']]]){
 const src=read(path);for(const name of names){const d=src.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(src)===name);assert.ok(d);run(`globalThis.${name}=${d.initializer.getText(src)}`,context);}
}
for(const name of ['fetchPositionsHelper','reinitOnThrow','getCategoryForSymbol']){const m=cls.members.find(m=>m.name?.getText(source)===name);assert.ok(m?.initializer);run(`globalThis.make_${name}=function(){return ${m.initializer.getText(source)}}`,context);}
const ok=(rows,cursor)=>({retMsg:'OK',result:{list:rows,nextPageCursor:cursor}});
const cases=[];
const add=async (symbols,pages,initial=false,mapFail=null)=>{
 const requests=[],events=[],mapped=[],warnings=[];let requestIndex=0,result,error,originalError;
 context.console={warn:s=>warnings.push(s)};
 const host={needsReinit:initial,store:{markets:[]},xhr:{get:async(url,{params})=>{
  requests.push({url,params});const page=pages[requestIndex++];assert.ok(page!==undefined,'fixture page inventory exhausted');
  if(page.requestError){originalError=Object.assign(new Error(page.requestError.message),{response:page.requestError.response});throw originalError;}
  return {data:page};
 }},emitAndLogError:value=>events.push({kind:'error',value}),mapPosition:row=>{mapped.push(row);if(row?.id===mapFail)throw new Error('mapper refused');return {mapped:row};}};
 for(const name of ['fetchPositionsHelper','reinitOnThrow','getCategoryForSymbol'])host[name]=context[`make_${name}`].call(host);
 try{result=await host.fetchPositionsHelper(symbols,'inverse','BTC');}catch(e){error={name:e.name,message:e.message,response:e.response,sameRequestError:e===originalError};}
 cases.push({symbols,pages,initial,mapFail,requests,events,mapped,warnings,result,error,needsReinit:host.needsReinit});
};
for(const symbols of [undefined,[],['BTCUSD'],['BTCUSD','ETHUSD']])for(const initial of [false,true]){
 const copies=symbols?.length||1;
 for(const sequence of [[ok([{id:1}])],[ok([{id:1}],'next'),ok([{id:2}],'next')],[ok([], 'next'),ok([{id:2}])],[ok('not-array')],[ok(null)],[ok([{id:1}],'next'),{retMsg:'Denied'}],[{ret_msg:'Denied'}],[{}],[{retMsg:null}],[{retMsg:42}],[{retMsg:{toString:null}}],[{retMsg:{}}],[12],[true],['bad response'],[{requestError:{message:'network refused',response:{status:503,data:{retMsg:'separate venue rejection'}}}}]]){
  await add(symbols,Array.from({length:copies},()=>sequence).flat(),initial);
 }
 await add(symbols,Array.from({length:copies},()=>[ok([{id:1},{id:2},{id:3}])]).flat(),initial,2);
}
for(const cursor of [0,false,'',null,1,true,[],{},['next']])await add(undefined,[ok([{id:1}],cursor),ok([{id:2}])]);
for(const cursor of [[],{page:1},1,true])await add(undefined,[ok([{id:1}],structuredClone(cursor)),ok([{id:2}],structuredClone(cursor)),ok([{id:3}])]);
await add(undefined,[ok([{id:1}],'a'),ok([{id:2}],'b'),ok([{id:3}],'a'),ok([{id:4}])]);
console.log(JSON.stringify(cases));
