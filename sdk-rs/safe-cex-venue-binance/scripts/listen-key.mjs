import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const ts=createRequire(import.meta.url)(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const fetchNames=['normal','alias-mutated','key-undefined','key-null','key-object','response-undefined','response-null','data-undefined','data-null','data-primitive','response-primitive','post-sync-fail','post-reject','schedule-fail'];
const updateNames=['disposed','normal','recover','disposed-during-put','flag-during-put','put-sync','put-reject','repeat-error','schedule-success-fail','warning-fail','emit-fail','retry-fail','undefined-error','null-error','number-error','string-error','array-error','object-error','coercion-fail','logger-changes-flag','schedule-changes-flag'];
const severityExports={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL('../../../packages/logging/src/LogSeverity.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:severityExports});
const results=[];
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.ws-private.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 const members=['fetchListenKey','updateListenKey'].map(name=>{const m=decl.members.find(m=>m.name?.getText(ast)===name);assert.ok(m);return m.getText(ast);});
 const initializer=(tree,name)=>{const declaration=tree.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(tree)===name);assert.ok(declaration?.initializer);return declaration.initializer.getText(tree);};
 const typesFile=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.types.ts`,import.meta.url),typesAst=ts.createSourceFile(typesFile.pathname,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
 const declarations=`const ENDPOINTS=${initializer(typesAst,'ENDPOINTS')}; const LISTEN_KEY_REFRESH_MS=${initializer(ast,'LISTEN_KEY_REFRESH_MS')}; const LISTEN_KEY_RETRY_MAX_MS=${initializer(ast,'LISTEN_KEY_RETRY_MAX_MS')};`;
 const exports={};vm.runInNewContext(ts.transpileModule(`${declarations} exports.Subject=class {${members.join('\n')}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Error,LogSeverity:severityExports.LogSeverity});
 for(const name of fetchNames){
  const instance=new exports.Subject(),trace=[],original=RangeError('fixture error'),first={id:11},second={id:22};
  let data={listenKey:first},response={data};
  if(name==='key-undefined')data.listenKey=undefined;
  if(name==='key-null')data.listenKey=null;
  if(name==='key-object')data.listenKey=first;
  if(name==='response-undefined')response=undefined;
  if(name==='response-null')response=null;
  if(name==='data-undefined')response={};
  if(name==='data-null')response={data:null};
  if(name==='data-primitive')response={data:7};
  if(name==='response-primitive')response=7;
  const client={post:path=>{trace.push(['post',path]);if(name==='post-sync-fail')throw original;return Promise.resolve().then(()=>{if(name==='post-reject')throw original;return response;});}};
  instance.parent={xhr:client,papiXhr:client};
  instance.scheduleListenKeyRefresh=delay=>{trace.push(['schedule',delay]);if(name==='alias-mutated')data.listenKey=second;if(name==='schedule-fail')throw original;};
  let result,failure=null,errorName=null,errorMessage=null,identity=null;
  try{result=await instance.fetchListenKey();if(result&&typeof result==='object'){assert.equal(result,name==='alias-mutated'?second:first);identity=true;}}catch(e){failure=e===original?'original':'generated';errorName=e.name;errorMessage=e.message;}
  results.push({variant,method:'fetch',name,trace,key:result===undefined?'undefined':result,identity,failure,errorName,errorMessage});
 }
 for(const name of updateNames){
  const instance=new exports.Subject(),trace=[],observed=[],original=RangeError('fixture error');
  instance.isDisposed=name==='disposed';instance.keepaliveFailing=['recover','repeat-error','warning-fail'].includes(name);
  let thrown=original;
  if(name==='undefined-error')thrown=undefined;
  if(name==='null-error')thrown=null;
  if(name==='number-error')thrown=42;
  if(name==='string-error')thrown='wire failure';
  if(name==='array-error')thrown=[1,null,'x'];
  if(name==='object-error')thrown={code:7};
  if(name==='coercion-fail')thrown={toString:0};
  const success=['disposed','normal','recover','disposed-during-put','schedule-success-fail','schedule-changes-flag'].includes(name);
  const client={put:path=>{trace.push(['put',path]);if(name==='put-sync')throw thrown;return Promise.resolve().then(()=>{if(name==='disposed-during-put')instance.isDisposed=true;if(name==='flag-during-put')instance.keepaliveFailing=true;if(!success)throw thrown;});}};
  instance.parent={xhr:client,papiXhr:client,log:(message,severity)=>{trace.push(['warning',message,severity]);observed.push(instance.keepaliveFailing);if(name==='warning-fail')throw original;if(name==='logger-changes-flag')instance.keepaliveFailing=false;}};
  instance.emitAndLogError=message=>{trace.push(['emit',message]);observed.push(instance.keepaliveFailing);if(name==='emit-fail')throw original;if(name==='logger-changes-flag')instance.keepaliveFailing=false;};
  instance.scheduleListenKeyRefresh=delay=>{trace.push(['schedule',delay]);observed.push(instance.keepaliveFailing);if((name==='schedule-success-fail'&&delay===1800000)||(name==='retry-fail'&&delay===60000))throw original;if(name==='schedule-changes-flag')instance.keepaliveFailing=true;};
  let failure=null,errorName=null,errorMessage=null;
  try{await instance.updateListenKey();}catch(e){failure=e===original?'original':'generated';errorName=e.name;errorMessage=e.message;}
  results.push({variant,method:'update',name,trace,observed,disposed:instance.isDisposed,failing:instance.keepaliveFailing,failure,errorName,errorMessage});
 }
}
assert.equal(results.length,70,'independently authored 2*(14+21) floor');
process.stdout.write(JSON.stringify(results));
