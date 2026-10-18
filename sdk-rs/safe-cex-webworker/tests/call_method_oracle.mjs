// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
function body(path) {
 const source=fs.readFileSync(new URL(path,root),'utf8');const ast=ts.createSourceFile(path,source,ts.ScriptTarget.Latest,true);
 return ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
}
const source=body('packages/safe-cex/src/webworker/handlers/CallMethodHandler.ts');
const cases=[...source.matchAll(/case '([^']+)':/g)].map(m=>m[1]).filter(name=>name!=='callMethod');
if(cases.length!==18 || new Set(cases).size!==18) throw new Error('RECOVERY source 18-command inventory');
const inputs={setLeverage:['BTC',3,'short'],scheduleCancel:[],updateSymbolMetadata:['BTC'],changeMarginType:[true,'BTC'],changePositionMode:[true,'ETH'],fetchOHLCV:[{symbol:'BTC',interval:'1m'}],fetchHistoricalFills:[{}],fetchOrders:[['BTC','ETH']],fetchAndSetOrders:[['ETH','BTC']],fetchHistoricalOrders:[{}],fetchIncomeHistory:[{}],removeOrders:[[{id:'o'}]],fetchClosedPnl:[{symbol:'BTC'}],fetchIncomeHistoryRange:[0,100,'funding']};
class BaseHandler {constructor(exchange,post){this._exchange=exchange;this._ctx={postMessage:post};}dispose(){}}
const context=vm.createContext({BaseHandler,Error,console:{log(){},error(){}},setTimeout:()=>0,clearTimeout(){}});
vm.runInContext(ts.transpileModule(body('packages/safe-cex/src/webworker/handlers/ThrottleManager.ts')+'\n'+source+'\nglobalThis.Handler=CallMethodHandler;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const replies=[];const calls=[];const exchange={};
for(const method of cases) exchange[method]=(...args)=>{
 calls.push({method,args});
 if(args[0]==='FAIL'){const error=new Error('bad');error.stack='trace';error.code=42;throw error;}
 if(method==='recoverAfterOffline') return true;
 return method.startsWith('fetch')&&!['fetchAndSetPrivateSnapshot'].includes(method)?[]:undefined;
};
const handler=new context.Handler(exchange,reply=>replies.push(reply));
const records=[];
for(const method of [...cases,'unknown']) {
 const args=inputs[method]||[];await handler.handleCallMethod(method,method,args);records.push({method,args,expected:replies.pop(),calls:calls.splice(0)});
}
await handler.handleCallMethod('failure','setLeverage',['FAIL',3]);records.push({method:'setLeverage',requestId:'failure',args:['FAIL',3],expected:replies.pop(),calls:calls.splice(0)});
let recoveryCases=0;
for(const outer of [false,true]) for(const outcome of [true,false,'error']) {
 let invoked=0;
 exchange.recoverAfterOffline=()=>{invoked++;if(outcome==='error') throw new Error('recovery failed');return outcome;};
 const requestId=`recovery-${outer}-${outcome}`;
 if(outer) handler.handle({op:'callMethod',data:{requestId,method:'recoverAfterOffline',args:[]}});
 else await handler.handleCallMethod(requestId,'recoverAfterOffline',[]);
 const reply=replies.pop();
 assert.equal(invoked,1,'RECOVERY source invoked once');
 assert.equal(reply.data.requestId,requestId,'RECOVERY source reply identity');
 assert.equal(reply.data.success,outcome!=='error','RECOVERY source success');
 if(outcome==='error') assert.equal(reply.data.result.message,'recovery failed','RECOVERY source error');
 else assert.equal(reply.data.result,outcome,'RECOVERY source native boolean');
 recoveryCases++;
}
assert.equal(recoveryCases,6,'RECOVERY source six inner/outer outcomes');
const basePath='packages/safe-cex/src/exchanges/base.ts';
const baseText=fs.readFileSync(new URL(basePath,root),'utf8');
const baseAst=ts.createSourceFile(basePath,baseText,ts.ScriptTarget.Latest,true);
const baseClass=baseAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseExchange');
const recovery=baseClass.members.find(n=>n.name?.getText(baseAst)==='recoverAfterOffline');
assert.ok(recovery?.initializer,'RECOVERY source BaseExchange initializer');
const initializer=ts.transpileModule('const recovery='+recovery.initializer.getText(baseAst),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const visits=[];
const owner={getAllSockets:()=>[null,{recoverAfterOffline(){visits.push(1);return false;}},{recoverAfterOffline(){visits.push(2);return true;}}]};
const result=new Function(initializer+';return recovery();').call(owner);
assert.equal(result,false,'RECOVERY source base false policy');
assert.deepEqual(visits,[1,2],'RECOVERY source visits every socket before reduction');
if(process.argv[2]==='timing') {
 const profiles=[];
 for(const outer of [false,true]) for(const [method,mode] of [['setLeverage','ready'],['setLeverage','error'],['setLeverage','held'],['recoverAfterOffline','ready'],['recoverAfterOffline','error'],['unknown','ready']]) {
  let release;const calls=[],replies=[];
  const exchange={
   setLeverage(){calls.push('setLeverage');if(mode==='held')return new Promise(resolve=>release=resolve);return mode==='error'?Promise.reject(new Error('bad')):Promise.resolve();},
   recoverAfterOffline(){calls.push('recoverAfterOffline');if(mode==='error')throw new Error('recovery failed');return true;}
  };
  const handler=new context.Handler(exchange,reply=>replies.push(reply));
  const args=method==='setLeverage'?[mode==='error'?'FAIL':'BTC',3]:[];
  const invoke=()=>outer?handler.handle({op:'callMethod',data:{requestId:'timing',method,args}}):handler.handleCallMethod('timing',method,args);
  const waiter=invoke();
  const prefix={calls:[...calls],replies:replies.length};
  if(release)release();
  if(!outer)await waiter;else for(let i=0;i<8;i++)await Promise.resolve();
  const reply=replies[0];assert.equal(replies.length,1,'TIMING exactly one source reply');
  profiles.push({outer,method,mode,prefix,success:reply.data.success,message:reply.data.result?.message??null});
 }
 process.stdout.write(JSON.stringify(profiles));
} else process.stdout.write(JSON.stringify(records));
