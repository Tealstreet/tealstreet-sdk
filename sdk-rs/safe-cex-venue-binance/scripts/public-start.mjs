import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url),cases=[];
for(const variant of ['binance','binancepm','binancespot']) {
 const ast=ts.createSourceFile(variant,fs.readFileSync(new URL(variant+'.exchange.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(ts.isClassDeclaration),member=klass.members.find(n=>n.name?.getText(ast)==='_startPublicDataOnly');assert.ok(member);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(){return (${member.initializer.getText(ast)})();}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 for(let mask=0;mask<32;mask++)for(const disposed of [false,true])for(const action of mask===0&&!disposed?['none','loaded','dispose']:['none']) {
  const loaded=Object.fromEntries(['balance','orders','markets','tickers','positions'].map((k,i)=>[k,!!(mask&(1<<i))]));
  const logs=[],updates=[],events=[];
  const instance={isDisposed:disposed,store:{loaded,update(changes){updates.push(changes);events.push("update");if(changes.loaded)this.loaded=changes.loaded;}}};
  // BaseExchange.dispose resets the store and notifies subscribers before the source resumes.
  instance.log=message=>{logs.push(message);events.push("log");if(action==='loaded')instance.store.update({loaded:{...instance.store.loaded,markets:true,tickers:true}});if(action==='dispose'){instance.isDisposed=true;instance.store.update({loaded:Object.fromEntries(Object.keys(loaded).map(key=>[key,false]))});}};
  await exports.fn.call(instance);
  cases.push({variant,disposed,action,loaded,logs,updates,events});
 }
}
process.stdout.write(JSON.stringify(cases));
