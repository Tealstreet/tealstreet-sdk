// Shared-contract finding: execute the real Store constructor and public startup methods.
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
const results=[];
for(const variant of ['binance','binancepm','binancespot']){
 const ast=ts.createSourceFile(variant,fs.readFileSync(new URL(`exchanges/binance/${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(ts.isClassDeclaration),member=klass.members.find(n=>n.name?.getText(ast)==='_startPublicDataOnly');assert.ok(member);
 let body=`exports.fn=function(){return (${member.initializer.getText(ast)})();}`;
 if(process.argv.includes('--bypass-store-write'))body=body.replace('this.store.update(', 'void (');
 const exported={};vm.runInNewContext(ts.transpileModule(body,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exported});
 for(const disposed of [false,true])for(const listenerThrows of [false,true]){
  const store=new DefaultStore(),trace=[],sentinel=Error('startup subscriber failed');
  store.update({balance:{used:1,free:2,total:3,upnl:4}});
  const unsub1=store.subscribe(data=>{trace.push(['first',data.balance.total,data.loaded.orders]);if(listenerThrows)throw sentinel;});
  const unsub2=store.subscribe(data=>trace.push(['second',data.balance.total,data.loaded.orders]));
  let rejected=false;
  try{await exported.fn.call({isDisposed:disposed,store,log:message=>trace.push(['log',message])});}
  catch(error){assert.equal(error,sentinel,'startup preserves subscriber error identity');rejected=true;}
  const expected=disposed?[]:[['log','Ready to watch public data on Binance'],['first',0,true],...listenerThrows?[]:[['second',0,true]]];
  assert.deepEqual(trace,expected,'startup commits before notification and skips later subscribers after throw');
  assert.equal(store.balance.total,disposed?3:0,'startup state remains committed after rejection');
  assert.equal(rejected,!disposed&&listenerThrows,'startup subscriber throw rejects async method');
  results.push({variant,disposed,listenerThrows,trace,rejected,total:store.balance.total,loaded:store.loaded});
  unsub1();unsub2();
 }
}
assert.equal(results.length,12);
process.stdout.write(JSON.stringify(results));
