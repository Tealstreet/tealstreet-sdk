// Shared-contract finding: execute the real Store constructor and PM method.
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
const ast=ts.createSourceFile('binancepm.exchange.ts',fs.readFileSync(new URL('exchanges/binance/binancepm.exchange.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinancePMExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='changePositionMode');
const constants=ts.createSourceFile('binancepm.types.ts',fs.readFileSync(new URL('exchanges/binance/binancepm.types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=constants.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(constants)==='ENDPOINTS'));
assert.ok(member);assert.ok(endpoints);
let body=`${endpoints.getText(constants)}\nexports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`;
if(process.argv.includes('--bypass-store-write'))body=body.replace("this.store.setSetting('isHedged', hedged)",'void hedged');
const exported={};vm.runInNewContext(ts.transpileModule(body,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exported});
const results=[];
const contracts=[[],[0],[-0],[-1],[1],['NaN'],['Infinity'],['-Infinity'],[-1,1],[1,-1]];
const messages=['venue message','',null,0,false,[],{}, {detail:'venue'}];
for(const initial of [false,true])for(const hedged of [false,true])for(const positions of contracts)for(const reject of [false,true])for(const emitterThrows of [false,true])for(const msg of messages){
 const store=new DefaultStore();store.setSetting('isHedged',initial);
 store.update({positions:positions.map((contracts,index)=>({id:String(index),contracts:typeof contracts==='string'?Number(contracts):contracts}))});
 const trace=[],emitterError=Object.assign(Error('emitter failed'),{code:'emitter-code'});
 const unsubscribe=store.subscribe(data=>trace.push(['notify',data.options.isHedged]));
 const instance={store,papiXhr:{async post(url,data){trace.push(['post',url,data.dualSidePosition]);if(reject)throw Object.assign(Error('request failed'),{response:{data:{msg}}});}},emitAndLogError(message){trace.push(['emit',message]);if(emitterThrows)throw emitterError;}};
 let error=null;
 try{await exported.fn.call(instance,hedged);}catch(e){error={name:e.name,message:e.message,code:e.code??null,same:e===emitterError};}
 results.push({initial,hedged,positions,reject,emitterThrows,msg,expected:{trace,error,isHedged:store.options.isHedged}});unsubscribe();
}
assert.equal(results.length,1280);
process.stdout.write(JSON.stringify(results));
