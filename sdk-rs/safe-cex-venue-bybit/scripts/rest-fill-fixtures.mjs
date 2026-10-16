import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const good={execId:'fill',orderId:'order',symbol:'BTCUSD',execType:'Trade',execQty:'2',execPrice:'3',execTime:'1700000000123',side:'Buy',execFee:'.1'};
const cases=[];
for(const variant of ['bybit','bybitinverse']){
 const source=ts.createSourceFile(variant,readFileSync(new URL(`exchanges/bybit/${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(source)==='fetchHistoricalFills'));
 const context=vm.createContext({OrderSide:{Buy:'buy',Sell:'sell'},LogSeverity:{Warning:'warning'},ENDPOINTS:{USER_TRADES:'/v5/execution/list',USER_ORDERS:'/v5/order/history'},camelCase,snakeCase,memoize:f=>f});
 const utility=ts.createSourceFile('v',readFileSync(new URL('utils/get-key.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
 for(const n of ['s','c','v']){const d=utility.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(utility)===n);vm.runInContext(ts.transpileModule(`globalThis.${n}=${d.initializer.getText(utility)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
 for(const name of ['fetchHistoricalFills','getExecutionQueryWindows']){const m=cls.members.find(m=>m.name?.getText(source)===name);vm.runInContext(ts.transpileModule(`globalThis.${name}=${name==='fetchHistoricalFills'?'async ':''}function(${m.initializer.parameters.map(p=>p.getText(source)).join(',')})${m.initializer.body.getText(source)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
 const sourceConstant=name=>vm.runInContext(cls.members.find(m=>m.name?.getText(source)===name).initializer.getText(source),context);
 const add=async(opts={},responses=[],fallbacks=[[],[]],publicOnly=false)=>{
  opts={symbol:'BTCUSD',...opts};const calls=[],diagnostics=[],fallbackCalls=[];let i=0,f=0,result,error;
  context.console={warn:m=>diagnostics.push(['console',m])};
  const owner={name:variant,DEFAULT_EXECUTIONS_LOOKBACK_MS:sourceConstant('DEFAULT_EXECUTIONS_LOOKBACK_MS'),EXECUTION_QUERY_MAX_RANGE_MS:sourceConstant('EXECUTION_QUERY_MAX_RANGE_MS'),isPublicDataOnly:publicOnly,getCategoryForSymbol:()=>variant==='bybitinverse'?'inverse':'linear',log:(m,s)=>diagnostics.push([s??'log',m]),xhr:{get:async(url,{params})=>{calls.push({url,params});const response=responses[i++]??{retMsg:'OK',result:{list:[]}};if(response.throw)throw Error(response.throw);return {data:response};}},fetchExecutionsFallbackFromOrderHistory:async(o,lookback)=>{fallbackCalls.push(lookback??null);const response=fallbacks[f++]??[];if(response.throw)throw Error(response.throw);return response;}};
  owner.getExecutionQueryWindows=(from,to)=>context.getExecutionQueryWindows.call(owner,from,to);
  try{result=await context.fetchHistoricalFills.call(owner,opts);}catch(e){error=e.message;}
  cases.push({variant,opts,responses,fallbacks,publicOnly,calls,diagnostics,fallbackCalls,result,error});
 };
 const page=(list,cursor)=>({retMsg:'OK',result:{list,nextPageCursor:cursor}});
 for(const limit of [undefined,0,1,2,50,101,-1])for(const list of [[],[good],[good,good],[{...good,execType:'Funding'}],[good,{...good,side:{toString:null}}]])await add({limit},[page(list,'next'),page([good])]);
 for(const response of [{throw:'network refused'},{retMsg:'BAD'},{ret_msg:'OK',result:{list:[good]}},{retMsg:{toString:null}},true,12,'OK',page(null),page({})])await add({},[response]);
 for(const opts of [{from:1},{to:1700000000000},{orderId:'one'},{orderId:''},{symbol:''},{from:0,to:2*604800000},{from:30,to:12}])await add(opts,[page([])]);
 await add({},[page([], 'loop'),page([], 'loop'),page([], 'loop'),page([], 'loop'),page([], 'loop'),page([good])]);
 await add({},[],[{throw:'fallback refused'}]);await add({},[],[[],{throw:'extended refused'}]);
 await add({},[page([good])],[[],[]],true);
 await add({},[],[[{timestamp:1,symbol:'BTCUSD',price:3,qty:2,side:'buy'}]]);
}
console.log(JSON.stringify(cases));
