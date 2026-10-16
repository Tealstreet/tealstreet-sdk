// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/ExchangeWorker.ts',import.meta.url);
const ast=ts.createSourceFile(String(path),fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const method=cls.members.find(s=>ts.isMethodDeclaration(s)&&s.name.getText(ast)==='_flushBufferedMessages');
assert.ok(method,'BUFFER actual method inventory');
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(`class Probe {${method.getText(ast)}}\nglobalThis.Probe=Probe;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const modes=['normal','log-append','log-clear','log-throw','handle-append','handle-throw','mutate-throw','string-throw','string-mutate','report-throw'];
function run(count,mode){
 const receiver=new context.Probe(),events=[];
 const original=Array.from({length:count},(_,i)=>({op:'__worker_ping__',data:{i}}));
 receiver._messageBuffer=[...original];
 const failure={toString(){events.push({stringify:true});if(mode==='string-mutate')original[0].op='error';if(mode==='string-throw')throw 'string failure';return 'TypeError: handler failed';}};
 receiver.log=(level,message,extra)=>{events.push({log:{level,message,extra}});if(mode==='log-throw')throw 'log failure';if(mode==='log-append')receiver._messageBuffer.push({op:'__worker_ping__',data:{i:99}});if(mode==='log-clear')receiver._messageBuffer=[];};
 receiver.handlePayload=payload=>{events.push({handle:payload.data.i});if(mode==='handle-append')receiver._messageBuffer.push({op:'__worker_ping__',data:{i:100+payload.data.i}});if(['handle-throw','mutate-throw','string-throw','string-mutate','report-throw'].includes(mode)&&payload.data.i===0){if(mode==='mutate-throw')payload.op='log';throw failure;}};
 receiver.logError=(message,error,extra)=>{assert.equal(error,failure,'BUFFER report preserves original failure');events.push({report:{message,extra}});if(mode==='report-throw')throw 'report failure';};
 let error=null;try{receiver._flushBufferedMessages();}catch(e){error=e;}
 return {count,mode,events,pending:receiver._messageBuffer,original,error};
}
const rows=[];for(const count of [0,1,2,10,11,12])for(const mode of modes)rows.push(run(count,mode));
const get=(count,mode)=>rows.find(r=>r.count===count&&r.mode===mode);
assert.equal(rows.length,60,'BUFFER case inventory');
assert.equal(get(11,'log-append').events.filter(e=>'handle'in e).length,12,'BUFFER log callback precedes snapshot');
assert.equal(get(11,'log-clear').events.filter(e=>'handle'in e).length,0,'BUFFER log clear precedes snapshot');
assert.equal(get(11,'log-throw').pending.length,11,'BUFFER log failure retains buffer');
assert.equal(get(2,'handle-append').pending.length,2,'BUFFER handler appends survive flush');
assert.equal(get(2,'handle-throw').events.filter(e=>'handle'in e).length,2,'BUFFER handler rejection continues');
assert.equal(get(2,'report-throw').events.filter(e=>'handle'in e).length,1,'BUFFER reporter failure aborts remaining snapshot');
assert.equal(get(2,'report-throw').pending.length,0,'BUFFER clear precedes dispatch');
assert.equal(get(2,'string-throw').error,'string failure','BUFFER coercion failure aborts before report');
assert.equal(get(2,'mutate-throw').events.find(e=>e.report).report.extra.payloadOp,'log','BUFFER report reads mutated payload');
assert.equal(get(2,'mutate-throw').original[0].op,'log','BUFFER source payload alias stays live');
assert.equal(get(2,'string-mutate').events.find(e=>e.report).report.extra.payloadOp,'error','BUFFER coercion-time alias changes reported payload op');
assert.equal(get(2,'string-mutate').original[0].op,'error','BUFFER coercion-time alias changes caller view');
const numeric=['-0','-0.0','1e400','-1e400','1e-400','-1e-400','1.25'].map(token=>{
 const receiver=new context.Probe();receiver._messageBuffer=[{op:'__worker_ping__',data:JSON.parse(`{"value":${token}}`)}];let observed;
 receiver.handlePayload=payload=>{const bytes=new DataView(new ArrayBuffer(8));bytes.setFloat64(0,payload.data.value);observed=bytes.getBigUint64(0).toString(16).padStart(16,'0');};
 receiver._flushBufferedMessages();return {token,bits:observed};
});
function errorText(text){const receiver=new context.Probe();receiver._messageBuffer=[{op:'__worker_ping__'}];receiver.handlePayload=()=>{throw {toString:()=>text};};let message;receiver.logError=value=>{message=value;};receiver._flushBufferedMessages();return message;}
const unicode=['ordinary','λ🚀',' \t\n',''].map(text=>({text,message:errorText(text)}));
const lone=JSON.parse('"\\ud800"');const surrogate={input:JSON.stringify(lone),message:JSON.stringify(errorText(lone))};
assert.equal(errorText(lone).charCodeAt(errorText(lone).length-1),0xd800,'BUFFER source error text retains lone surrogate');
process.stdout.write(JSON.stringify({rows,numeric,unicode,surrogate}));
