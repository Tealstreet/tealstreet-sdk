import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const encode=n=>Object.is(n,-0)?'-0':String(n);
const data=[undefined,null,{},
 {totalWalletBalance:'113.75tail',availableBalance:'29.5',totalUnrealizedProfit:'-7.25'},
 {totalWalletBalance:'-0',availableBalance:'-0',totalUnrealizedProfit:'-0'},
 {totalWalletBalance:'Infinity',availableBalance:{toString:null}},
 {totalWalletBalance:'NaN'},
 {totalWalletBalance:'8',availableBalance:'Infinity',totalUnrealizedProfit:'-Infinity'},
 {totalWalletBalance:{toString:null}},
 {totalWalletBalance:'20',availableBalance:{toString:null}},
 {totalWalletBalance:'20',availableBalance:'3',totalUnrealizedProfit:{toString:null}},
 {totalWalletBalance:[12.5],availableBalance:null,totalUnrealizedProfit:false}];
const payload={total:211,free:157,used:54,upnl:-13,tag:'opaque balance'};
const scenarios=[];
for(let index=0;index<data.length;index++)for(const key of [undefined,'','backup'])scenarios.push({index,key,primaryReject:false,secondaryReject:false,fetchKind:'value',emitReject:false,msg:'detail',message:'request failed'});
for(const msg of ['venue detail','',null,0,false,[],{}, {toString:null}])for(const message of ['request failed',''])for(const role of ['primary','secondary'])for(const emitReject of [false,true])scenarios.push({index:3,key:'backup',primaryReject:role==='primary',secondaryReject:role==='secondary',fetchKind:'value',emitReject,msg,message});
for(const fetchKind of ['value','missing','null-balance','undefined-result','null-result','reject'])scenarios.push({index:3,key:'backup',primaryReject:false,secondaryReject:false,fetchKind,emitReject:false,msg:'detail',message:'request failed'});
assert.equal(scenarios.length,106);
const output=[];
for(const variant of ['binance','binancepm','binancespot']){
 const ast=parse(`${variant}.exchange.ts`),klass=ast.statements.find(ts.isClassDeclaration),method=klass.members.find(n=>n.name?.getText(ast)==='validateAccount');assert.ok(method);
 const constants=parse(`${variant}.types.ts`),endpoint=constants.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(d=>d.name.getText(constants)==='ENDPOINTS');assert.ok(endpoint);
 const exports={};vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoint.initializer.getText(constants)};exports.fn=function(){return (${method.initializer.getText(ast)})();}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 for(const spec of scenarios){
  const trace=[],sentinel=Object.assign(RangeError('emitter failed'),{code:'emitter-code',response:{status:503}});
  const options={get secondary_key(){trace.push(['key']);return spec.key;}};
  const client=(name,reject)=>({async get(url){trace.push(['get',name,url]);if(reject)throw Object.assign(Error(spec.message),{response:{data:{msg:spec.msg}}});return {data:data[spec.index]};}});
  const fetch=async name=>{trace.push(['fetch',name]);switch(spec.fetchKind){case 'value':return {balance:payload};case 'missing':return {};case 'null-balance':return {balance:null};case 'undefined-result':return undefined;case 'null-result':return null;case 'reject':throw Error('balance failed');default:assert.fail(spec.fetchKind);}};
  const instance={options,_primaryXhr:client('primary',spec.primaryReject),_secondaryXhr:client('secondary',spec.secondaryReject),xhr:client('spot',spec.primaryReject),fetchBalance:()=>fetch('fetchBalance'),fetchPositionsAndBalance:()=>fetch('fetchPositionsAndBalance'),emitAndLogError(message){trace.push(['emit',message]);if(spec.emitReject)throw sentinel;}};
  let result,error=null;
  try{const raw=await exports.fn.call(instance);result={kind:raw.error===''?'valid':'invalid',error:raw.error,hasBalance:Object.hasOwn(raw,'balance'),balanceKind:raw.balance===undefined?'undefined':raw.balance===null?'null':'value'};
   if(raw.balance!==undefined&&raw.balance!==null)result.balance=variant==='binance'?Object.fromEntries(['total','free','used','upnl'].map(k=>[k,encode(raw.balance[k])])):raw.balance;
  }catch(e){error={name:e.name,message:e.message,code:e.code??null,response:e.response??null,same:e===sentinel};}
  output.push({variant,...spec,...(data[spec.index]===undefined?{}:{data:data[spec.index]}),payload,expected:{trace,result:result??null,error}});
 }
}
assert.equal(output.length,318);
const selection=process.argv.indexOf('--variant');
const variant=selection<0?null:process.argv[selection+1];
assert.ok(variant===null||['binance','binancepm','binancespot'].includes(variant));
process.stdout.write(JSON.stringify(variant===null?output:output.filter(c=>c.variant===variant)));
