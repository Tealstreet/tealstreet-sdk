import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const context=vm.createContext({camelCase,snakeCase,memoize:f=>f});
const load=(path,name)=>{
 const file=ts.createSourceFile(path,readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
 const d=file.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(file)===name);
 return d.initializer.getText(file);
};
for(const name of ['s','c','v']) vm.runInContext(ts.transpileModule(`globalThis.${name}=${load('utils/get-key.ts',name)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
context.ENDPOINTS={FEE_RATE:'/v5/account/fee-rate',MARKETS:'/v5/market/instruments-info'};
const market={symbol:'BTCUSD',baseCoin:'BTC',quoteCoin:'USD',status:'Trading',lotSizeFilter:{qtyStep:'1',minOrderQty:'1',maxOrderQty:'100'},priceFilter:{tickSize:'.5'},leverageFilter:{minLeverage:'1',maxLeverage:'100'},deliveryTime:'0'};
const ok=list=>({retMsg:'OK',result:{list}});
const cases=[];
for(const variant of ['bybitinverse','bybitspot']) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`exchanges/bybit/${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(ts.isClassDeclaration);
 for(const name of ['fetchFees','fetchMarketsHelper']) {
  const member=cls.members.find(m=>m.name?.getText(file)===name);
  if(member)vm.runInContext(ts.transpileModule(`globalThis.${name}=async function(${member.initializer.parameters.map(p=>p.getText(file)).join(",")}) ${member.initializer.body.getText(file)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 }
 const add=async(responses,publicOnly=false,category='inverse')=>{
  const calls=[],emissions=[];let result,error,index=0;
  const owner={isPublicDataOnly:publicOnly,emitAndLogError:v=>emissions.push(v),xhr:{get:async(url,{params})=>{calls.push({url,params});const response=responses[index++];if(response?.throw)throw Error(response.throw);return {data:response};}}};
  owner.fetchFees=c=>context.fetchFees.call(owner,c);
  try{result=await context.fetchMarketsHelper.call(owner,category);}catch(e){error=e.message;}
  cases.push({variant,responses,publicOnly,category,calls,emissions,result,error});
 };
 for(const publicOnly of [false,true])for(const category of ['inverse','spot','linear']) {
  const fee=variant==='bybitinverse'&&!publicOnly?[ok([{symbol:'BTCUSD',takerFeeRate:'.01',makerFeeRate:'.02'}])]:[];
  const tail=variant==='bybitinverse'?[ok([{...market,status:'PreLaunch'}])]:[];
  for(const first of [ok([market]),ok([]),{retMsg:'BAD'},{ret_msg:'OK',result:{list:[market]}},{retMsg:null},{}, {throw:'first refused'}, {retMsg:'OK'},ok(null),ok({}),true,12,'OK'])await add([...fee,first,...tail],publicOnly,category);
  if(variant==='bybitinverse')for(const second of [{retMsg:'SECOND BAD'},{throw:'second refused'},ok([]),ok(null),ok({})])await add([...fee,ok([market]),second],publicOnly,category);
 }
 if(variant==='bybitinverse')for(const fee of [{throw:'fee refused'},ok(null),ok({}),{},ok([null])])await add([fee,ok([market]),ok([])]);
}
assert.ok(cases.length>100);
console.log(JSON.stringify(cases));
