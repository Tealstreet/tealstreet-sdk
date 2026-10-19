// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/ExchangeWorker.ts',import.meta.url);
const ast=ts.createSourceFile(String(path),fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const names=['isRecord','getWorkerSentryControl','splitWorkerLogExtra'];
const helpers=ast.statements.filter(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const method=cls.members.find(s=>ts.isMethodDeclaration(s)&&s.name.getText(ast)==='logError');
assert.equal(helpers.length,3,'WORKER-ERROR actual helper inventory');assert.ok(method,'WORKER-ERROR actual method inventory');
let methodText=method.getText(ast);
if(process.argv.includes('--strict-boundary'))methodText=methodText.replace('>= workerSentryControl.reportCooldownMs','> workerSentryControl.reportCooldownMs');
if(process.argv.includes('--drop-fallback'))methodText=methodText.replace("console.error('[ExchangeWorker] Failed to send error to main thread:', message, error, e);",'void e;');
class CapturedError extends Error {constructor(message){super(message);this.stack='host-stack';}}
function run(errorKind,postMode,extraKind,send){
 let now=0,events=[];
 const consoleFailure={kind:'console'},serializationFailure={kind:'serialization'},postFailure={kind:'post'};
 let original;
 const context=vm.createContext({Error:CapturedError,Date:{now(){events.push({clock:now});return now;}},console:{error(prefix,message,error,cause){assert.equal(error,original,'WORKER-ERROR fallback original identity');assert.equal(cause,postFailure,'WORKER-ERROR fallback post identity');events.push({fallback:{prefix,message}});if(postMode==='console-throw')throw consoleFailure;}}});
 vm.runInContext(ts.transpileModule(helpers.map(h=>h.getText(ast)).join('\n')+`\nclass Probe {${methodText}}\nglobalThis.Probe=Probe;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const worker=new context.Probe();worker._exchangeName='binance';worker._accountName='account';worker._sentryReportCooldowns=new Map();worker._ctx={postMessage(payload){events.push({post:payload});if(postMode!=='normal')throw postFailure;}};
 original=errorKind==='native'?new CapturedError('source message'):{};
 Object.assign(original,{safeCexSentryReportKey:'same',safeCexSentryReportCooldownMs:100,safeCexSentryTags:{source:'control',control:true},safeCexSentryExtra:{winner:'control'}});
 if(errorKind==='string-throw')original.toString=()=>{events.push({stringify:true});throw serializationFailure;};
 if(errorKind==='getter-throw')Object.defineProperty(original,'safeCexSentryReportKey',{get(){events.push({getter:true});throw serializationFailure;}});
 const extra=extraKind==='absent'?undefined:extraKind==='suppress'?{suppressConsoleLog:true}:{winner:'plain',sentryTags:{source:'split',module:'override',split:true},sentryExtra:{winner:'split',suppressedRepeatedReports:99}};
 const calls=[];
 for(now of [0,50,100,90,200]){
  events=[];let failure=null;try{worker.logError('worker message',original,extra,send);}catch(error){failure=error===serializationFailure?'serialization':error===consoleFailure?'console':'unexpected';}
  calls.push({now,events,failure,cooldowns:[...worker._sentryReportCooldowns]});
 }
 return{errorKind,postMode,extraKind,send,calls};
}
const rows=[];for(const error of ['native','plain','string-throw','getter-throw'])for(const mode of ['normal','post-throw','console-throw'])for(const extra of ['absent','suppress','mixed'])for(const send of [true,false])rows.push(run(error,mode,extra,send));
assert.equal(rows.length,72,'WORKER-ERROR source case inventory');
const row=(error,mode,extra,send=true)=>rows.find(r=>r.errorKind===error&&r.postMode===mode&&r.extraKind===extra&&r.send===send);
for(const r of rows)for(const call of r.calls){
 const serializes=r.errorKind!=='getter-throw';
 const posts=serializes&&r.errorKind!=='string-throw';
 assert.equal(call.events.filter(e=>'clock'in e).length,serializes&&r.send?1:0,'WORKER-ERROR clock effect count');
 assert.equal(call.events.filter(e=>'post'in e).length,posts?1:0,'WORKER-ERROR post effect count');
 assert.equal(call.events.filter(e=>'fallback'in e).length,posts&&r.postMode!=='normal'?1:0,'WORKER-ERROR fallback effect count');
}
const ordinary=row('native','normal','absent');
assert.equal(ordinary.calls[1].events.at(-1).post.data[1].sendToSentry,false,'WORKER-ERROR cooldown suppresses');
assert.equal(ordinary.calls[2].events.at(-1).post.data[1].sentryData.extra.suppressedRepeatedReports,1,'WORKER-ERROR equality flushes suppressed count');
assert.equal(ordinary.calls[3].events.at(-1).post.data[1].sendToSentry,false,'WORKER-ERROR backward time suppresses');
assert.equal(row('string-throw','normal','absent').calls[0].cooldowns[0][1].lastReportedAt,0,'WORKER-ERROR cooldown committed before serialization throw');
assert.deepEqual(row('getter-throw','normal','absent').calls[0].cooldowns,[],'WORKER-ERROR control getter precedes cooldown');
assert.equal(row('native','post-throw','absent').calls[0].failure,null,'WORKER-ERROR post failure swallowed');
assert.equal(row('native','console-throw','absent').calls[0].failure,'console','WORKER-ERROR fallback failure escapes');
assert.ok(row('native','normal','mixed',false).calls.every(c=>c.cooldowns.length===0&&c.events.every(e=>!('clock'in e))),'WORKER-ERROR disabled sentry never clocks');
const mixed=row('native','normal','mixed').calls[0].events.at(-1).post.data[1].sentryData;
assert.equal(mixed.tags.source,'control','WORKER-ERROR control tags precedence');
assert.equal(mixed.tags.module,'override','WORKER-ERROR split tags precedence');
assert.equal(mixed.extra.winner,'control','WORKER-ERROR extra precedence');
const bitBytes=new DataView(new ArrayBuffer(8));
const bits=n=>{bitBytes.setFloat64(0,n);return bitBytes.getBigUint64(0).toString(16).padStart(16,'0');};
const clockBits=[];
for(const token of ['-0','-0.0','1e400','-1e400','1e-400','-1e-400']){
 const value=JSON.parse(token),context=vm.createContext({Error:CapturedError,Date:{now:()=>value},console:{error(){throw new Error('unexpected console');}}});
 vm.runInContext(ts.transpileModule(helpers.map(h=>h.getText(ast)).join('\n')+`\nclass Probe {${methodText}}\nglobalThis.Probe=Probe;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const worker=new context.Probe();worker._sentryReportCooldowns=new Map();worker._ctx={postMessage(){}};
 worker.logError('bits',{safeCexSentryReportKey:'same',safeCexSentryReportCooldownMs:100},undefined,true);
 clockBits.push({token,last:bits(worker._sentryReportCooldowns.get('same').lastReportedAt)});
}
const aliasContext=vm.createContext({Error:CapturedError,Date:{now:()=>0},console:{error(){throw new Error('unexpected fallback');}}});
vm.runInContext(ts.transpileModule(helpers.map(h=>h.getText(ast)).join('\n')+`\nclass Probe {${methodText}}\nglobalThis.Probe=Probe;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,aliasContext);
const aliasWorker=new aliasContext.Probe();aliasWorker._sentryReportCooldowns=new Map();let aliasPosted;
aliasWorker._ctx={postMessage(payload){aliasPosted=payload;}};
const aliasTags={phase:'before'},aliasError={safeCexSentryTags:aliasTags,toString(){aliasTags.phase='after';return 'alias-error';}};
aliasWorker.logError('alias',aliasError);
const alias={caller:aliasTags.phase,reported:aliasPosted.data[1].sentryData.tags.phase};
assert.deepEqual(alias,{caller:'after',reported:'after'},'WORKER-ERROR live control tag alias during coercion');
const textCases=['',' λ🚀 ','\ud800'].map(message=>{
 const error=new CapturedError(message);aliasWorker.logError(message,error);
 return {message,reported:aliasPosted.data[1].sentryData.error.message};
});
assert.equal(textCases[2].reported.charCodeAt(0),0xd800,'WORKER-ERROR source lone surrogate retained');
const shared={value:Infinity};shared.self=shared;
let deep=17;for(let i=0;i<150;i++)deep={unread:deep};
const graph={first:shared,second:shared,missing:undefined,zero:-0,'\ud800':'\udc00',deep};
aliasWorker.logError('native-extra',new CapturedError('source-message'),graph);
const retained=aliasPosted.data[1].sentryData.extra;
const nativeGraph={first:retained.first===shared,second:retained.second===shared,self:retained.first.self===shared,missing:Object.hasOwn(retained,'missing')&&retained.missing===undefined,zero:bits(retained.zero),infinity:bits(retained.first.value),text:retained['\ud800'].charCodeAt(0),deep:retained.deep===deep};
shared.self=null;
aliasWorker._sentryReportCooldowns=new Map();
const changingObject={toString(){throw new Error('Map key must not stringify');}};
let keyReads=0;const keyError={safeCexSentryReportCooldownMs:100};
Object.defineProperty(keyError,'safeCexSentryReportKey',{get(){return ++keyReads%3===0?changingObject:'validated';}});
const sends=[];aliasWorker._ctx={postMessage(p){sends.push(p.data[1].sendToSentry);}};
for(let i=0;i<2;i++)aliasWorker.logError('changing-key',keyError);
const changingKey={reads:keyReads,size:aliasWorker._sentryReportCooldowns.size,identity:aliasWorker._sentryReportCooldowns.has(changingObject),sends};
process.stdout.write(JSON.stringify({rows,clockBits,alias,textCases,nativeGraph,changingKey}));
