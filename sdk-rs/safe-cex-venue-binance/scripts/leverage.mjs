import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
function load(file,deps){
 const exports={};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,name);return deps[name];},structuredClone});
 return exports;
}
const astTypes=parse('types.ts'),types={};
vm.runInNewContext(ts.transpileModule(astTypes.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(astTypes)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
const {DefaultStore}=load('store/store.base.ts',{'lodash/uniqBy':runtimeRequire('lodash/uniqBy'),'../types':types,'../utils/clone':load('utils/clone.ts',{}),'../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1000}}});
const {createPositionId}=load('utils/createPositionId.ts',{'../types':types});
const encode=n=>Object.is(n,-0)?'-0':String(n);
const inputs=[20,0,7,5,NaN,Infinity,-Infinity,-0],results=[],prototypeCases=[];
for(const [variant,client] of [['binance','xhr'],['binancepm','papiXhr']]){
 const ast=parse(`exchanges/binance/${variant}.exchange.ts`),klass=ast.statements.find(ts.isClassDeclaration);
 const method=klass.members.find(n=>n.name?.getText(ast)==='setLeverage');assert.ok(method);
 const constants=parse(`exchanges/binance/${variant}.types.ts`),endpoint=constants.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(d=>d.name.getText(constants)==='ENDPOINTS');assert.ok(endpoint);
 const exported={};
 vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoint.initializer.getText(constants)};exports.fn=function(...args){return (${method.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exported,createPositionId,PositionSide:types.PositionSide});
 const specs=inputs.flatMap((input,spec)=>[false,true].flatMap(reject=>[false,true].flatMap(emitReject=>['venue detail',''].map(msg=>({spec,input,min:spec===7?-0:1,listenerReject:false,market:true,positions:true,reject,emitReject,msg})))));
 specs.push(...[{market:false,positions:true},{market:true,positions:false},{market:false,positions:false}].map(guards=>({spec:8,input:7,min:1,reject:false,emitReject:false,msg:'',listenerReject:false,...guards})));
 specs.push(...[false,true].map(emitReject=>({spec:2,input:7,min:1,market:true,positions:true,reject:false,emitReject,msg:'',listenerReject:true})));
 for(const c of specs){
  const store=new DefaultStore();
  store.update({markets:c.market?[{id:'market-id',symbol:'BTCUSDT',limits:{leverage:{min:c.min,max:10}}}]:[],positions:c.positions?[
   {id:createPositionId('BTCUSDT',types.PositionSide.Long),symbol:'BTCUSDT',side:'long',leverage:5,contracts:1},
   {id:createPositionId('BTCUSDT',types.PositionSide.Short),symbol:'BTCUSDT',side:'short',leverage:11,contracts:2},
   {id:createPositionId('ETHUSDT',types.PositionSide.Long),symbol:'ETHUSDT',side:'long',leverage:13,contracts:3}]:[]});
  const trace=[],emitterError=Object.assign(RangeError('emitter failed'),{code:'emitter-code'});
  const unsubscribe=store.subscribe(data=>{trace.push(['notify',data.positions.map(p=>encode(p.leverage))]);if(c.listenerReject)throw Object.assign(Error('listener failed'),{response:{data:{msg:'subscriber detail'}}});});
  const instance={store,emitAndLogError(message){trace.push(['emit',message]);if(c.emitReject)throw emitterError;}};
  for(const name of ['xhr','papiXhr'])instance[name]={async post(url,body){trace.push(['post',name,url,body.symbol,encode(body.leverage)]);if(c.reject)throw Object.assign(Error('request failed'),{response:{data:{msg:c.msg}}});}};
  let error=null;
  try{await exported.fn.call(instance,'BTCUSDT',c.input);}catch(e){error={name:e.name,message:e.message,code:e.code??null,same:e===emitterError};}
  const posts=trace.filter(e=>e[0]==='post');assert.ok(posts.every(p=>p[1]===client));
  results.push({...c,input:encode(c.input),min:encode(c.min),variant,expected:{trace,error,positions:store.positions.map(p=>({id:p.id,leverage:encode(p.leverage)}))}});
  unsubscribe();
 }
 for(const present of [false,true]){
  const store=new DefaultStore();
  store.update({positions:present?[{id:'constructor:long',symbol:'constructor',side:'long',leverage:5,contracts:1}]:[]});
  const instance={store,xhr:{post(){throw Error('unexpected POST');}},papiXhr:{post(){throw Error('unexpected POST');}},emitAndLogError(){throw Error('unexpected emission');}};
  let error=null;
  try{await exported.fn.call(instance,'constructor',7);}catch(e){error={name:e.name,message:e.message};}
  prototypeCases.push({variant,present,error});
 }
}
assert.equal(prototypeCases.length,4);
assert.equal(results.length,138);
process.stdout.write(JSON.stringify(process.argv.includes('--prototype')?prototypeCases:results));
