// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts',import.meta.url);
const ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const source=ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');
const program=ts.transpileModule(source+'\nglobalThis.Manager=ChaserOrderManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const values=[undefined,null,false,true,0,1,'', 'x',[],{}, {privateInitialized:true},{privateInitialized:1},{privateInitialized:'true'},{privateHealthy:false,privateInitialized:true}];
for(const paused of [undefined,false,true,1,'true'])for(const initialized of [undefined,false,true])for(const code of [undefined,null,0,1006,'', '𐐀',[1,null,[2]],{}, {toString:null},[{toString:0}]])values.push({privateReconnectPaused:paused,privateInitialized:initialized,privateCloseCode:code});
const inputs=values.map(v=>JSON.stringify(v));inputs.push('1e400','{"privateReconnectPaused":true,"privateCloseCode":1e400}');
const rows=[];
for(const name of ['bybitdemo','bybit'])for(const initial of inputs){
 for(const states of [[initial],[initial,'{"privateInitialized":true}'],[initial,'{}','{"privateReconnectPaused":true,"privateCloseCode":1006}'],[initial,'{}','{"privateReconnectPaused":true,"privateCloseCode":{"toString":null}}']]){
  let next=0;const sleeps=[],exchange={name,store:{socketHealth:initial===undefined?undefined:JSON.parse(initial)}};
  const context=vm.createContext({sleep:async ms=>{sleeps.push(ms);next++;const state=states[Math.min(next,states.length-1)];exchange.store.socketHealth=state===undefined?undefined:JSON.parse(state);}});vm.runInContext(program,context);
  const manager=new context.Manager();let direct,result;
  try{direct={reason:manager.getBybitDemoDeadPrivateWsReason(exchange)??null};}catch(error){direct={error:{name:error.name,message:error.message}};}
  try{result={reason:await manager.waitForBybitDemoPrivateWs(exchange)??null};}catch(error){result={error:{name:error.name,message:error.message}};}
  rows.push({name,states:states.map(s=>s??null),direct,result,sleeps});
 }
}
const context=vm.createContext({sleep:async()=>{}});vm.runInContext(program,context);const manager=new context.Manager();
const inherited={name:'bybitdemo',store:{socketHealth:Object.create({privateInitialized:true})}};
const array=[];array.privateInitialized=true;
const surrogate={privateReconnectPaused:true,privateCloseCode:'\uD800'};
const witness={inherited:manager.getBybitDemoDeadPrivateWsReason(inherited)??null,namedArray:manager.getBybitDemoDeadPrivateWsReason({name:'bybitdemo',store:{socketHealth:array}})??null,surrogateInput:JSON.stringify(surrogate),surrogateOutput:JSON.stringify(manager.getBybitDemoDeadPrivateWsReason({name:'bybitdemo',store:{socketHealth:surrogate}}))};
witness.inheritedWait=await manager.waitForBybitDemoPrivateWs(inherited)??null;
witness.namedArrayWait=await manager.waitForBybitDemoPrivateWs({name:'bybitdemo',store:{socketHealth:array}})??null;
process.stdout.write(JSON.stringify({rows,witness}));
