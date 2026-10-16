// Execute the real Store constructor and Futures method with explicit callbacks.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function load(file,deps){
 const exports={};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,`unexpected import ${name}`);return deps[name];},structuredClone});
 return exports;
}
const typesAst=ts.createSourceFile('types.ts',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enumText=typesAst.statements.filter(n=>ts.isEnumDeclaration(n)).map(n=>n.getText(typesAst)).join('\n');
const types={};vm.runInNewContext(ts.transpileModule(enumText,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
const {DefaultStore}=load('store/store.base.ts',{'lodash/uniqBy':runtimeRequire(process.argv.includes('--missing-runtime-import')?'lodash/uniqBy-deliberately-absent':'lodash/uniqBy'),'../types':types,'../utils/clone':load('utils/clone.ts',{}),'../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1000}}});
const ast=ts.createSourceFile('binance.exchange.ts',fs.readFileSync(new URL('exchanges/binance/binance.exchange.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinanceExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='changePositionMode');
const helper=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='isBinancePositionModeAlreadySetError'));
const constants=ts.createSourceFile('binance.types.ts',fs.readFileSync(new URL('exchanges/binance/binance.types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=constants.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(constants)==='ENDPOINTS'));
assert.ok(member);assert.ok(helper);assert.ok(endpoints);
const exported={};vm.runInNewContext(ts.transpileModule(`${endpoints.getText(constants)}\n${helper.getText(ast)}\nexports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exported});
const results=[],contracts=[[],[0],[-1],[1],['NaN'],['Infinity'],['-Infinity'],[-1,1]];
const failures=[null,{code:-4059,msg:'already numeric'},{code:'-4059',msg:'already string'},{code:400,msg:''},{code:400,msg:[]},{code:400,msg:{detail:'venue'}}];
for(const hedged of [false,true])for(const symbol of [null,'','BTCUSDT'])for(const positions of contracts)for(const requestFailure of failures)for(const confirmations of [[],[{code:400,msg:'confirmation failed'}],[{code:-4059,msg:'retry confirmation'}],[{code:'-4059',msg:''},{code:400,msg:'second fails'}]])for(const emitterThrows of [false,true]){
 const store=new DefaultStore();store.update({positions:positions.map((contracts,index)=>({id:String(index),contracts:typeof contracts==='string'?Number(contracts):contracts}))});
 const trace=[],emitterError=Object.assign(Error('emitter failed'),{code:'emitter-code'});let applied=0;
 const failure=(data,phase)=>Object.assign(Error(`${phase} failed`),{response:{data},code:phase});
 const instance={store,xhr:{async post(url,data){trace.push(['post',url,data.dualSidePosition]);if(requestFailure)throw failure(requestFailure,'request');}},applyConfirmedPositionMode(h,s){trace.push(['confirm',h,s??null]);const error=confirmations[applied++];if(error)throw failure(error,'confirmation');},emitAndLogError(message){trace.push(['emit',message]);if(emitterThrows)throw emitterError;}};
 let error=null;try{await exported.fn.call(instance,hedged,symbol??undefined);}catch(e){error={name:e.name,message:e.message,code:e.code??null,same:e===emitterError};}
 results.push({hedged,symbol,positions,requestFailure,confirmations,emitterThrows,expected:{trace,error}});
}
assert.equal(results.length,2304);
process.stdout.write(JSON.stringify(results));
