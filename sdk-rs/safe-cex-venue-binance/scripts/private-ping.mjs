import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/',import.meta.url);
let now=0,events=[];
const host={performance:{now(){events.push('clock');return now;}},setTimeout:()=>1,clearTimeout:()=>{}};
function load(file,method){
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(ast)===method)),member=klass.members.find(n=>n.name?.getText(ast)===method);assert.ok(member);
 const constant=ast.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(ast)==='BINANCE_SPOT_HEARTBEAT_REQUEST_ID'));
 const exports={};vm.runInNewContext(ts.transpileModule(`${constant?constant.getText(ast).replace('export ',''):''}\nexports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...host});return exports.fn;
}
const pong=load('base.ws.ts','handlePongEvent'),sendWithLog=load('base.ws.ts','sendWithLog'),cases=[];
for(const variant of ['binance','binancepm','binancespot']){
 const ping=load('binance/'+variant+'.ws-private.ts','ping');
 for(const diagnostics of [false,true])for(const disposed of [false,true])for(const fail of [false,true])for(const initial of [0,99])for(const time of [0,0.5,1,1.5,2,3,100,1000]){
  now=time;events=[];const sent=[],diagnosticFrames=[];let error=null,latency=null;
  const instance={isDisposed:disposed,pingAt:initial,sendWithLog,emitWsDiagnosticSend(frame){if(diagnostics){events.push('diagnostic');diagnosticFrames.push(frame);}},send(frame){events.push('send');sent.push(frame);if(fail)throw Object.assign(Error('fixture socket send'),{code:'fixture-code'});},recordMessageHandlerStats(){},store:{update(v){latency=v.latency;}},shouldEnforcePongMissReconnect:()=>false};
  try{ping.call(instance);}catch(e){error={name:e.name,message:e.message,code:e.code};}
  const trace=[...events];now=10000;pong.call(instance);
  cases.push({variant,diagnostics,disposed,fail,initial,time,sent,diagnosticFrames,error,trace,latency});
 }
}
process.stdout.write(JSON.stringify(cases));
