import fs from 'node:fs';import path from 'node:path';import {createRequire} from 'node:module';import {fileURLToPath} from 'node:url';import assert from 'node:assert/strict';
const require=createRequire(import.meta.url),root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetExchange');
const names=['markPrivateStreamDegraded','schedulePrivateStreamDegradedReconcile','schedulePrivateStreamWriteCheck'];
const body=names.map(name=>{const field=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);assert(field?.initializer,'TIMERS actual '+name);return name+' = '+field.initializer.getText(ast)+';';}).join('\n');
const constants=ast.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).filter(n=>['BITGET_PRIVATE_STREAM_DEGRADED_MESSAGE','BITGET_PRIVATE_STREAM_DEGRADED_RECONCILE_MS','BITGET_PRIVATE_STREAM_WRITE_GRACE_MS'].includes(n.name.getText(ast)));
assert.equal(constants.length,3,'TIMERS source literals');
const code=ts.transpileModule(constants.map(n=>'const '+n.getText(ast)+';').join('\n')+'class Source {'+body+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const logging=ts.transpileModule(fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8').replace(/export\s+/g,''),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const severity=new Function(logging+';return LogSeverity;')();
const {Source,message}=new Function('LogSeverity',code+';return {Source,message:BITGET_PRIVATE_STREAM_DEGRADED_MESSAGE};')(severity);
const units=[0x41,0xd800,0x42,0xdc00];const text=String.fromCharCode(...units);
function value(kind){return kind==='undefined'?undefined:kind==='null'?null:kind==='text'?text:kind==='object'?{units}:kind==='true'?true:false;}
function reasonWire(reason,original){if(reason===message)return {kind:'default',units:[...message].map(c=>c.charCodeAt(0)),same:true};return {kind:reason===null?'null':typeof reason==='string'?'text':'object',units:typeof reason==='string'?units:reason?.units??[],same:reason===original};}
function run(input){
 const trace=[],error={units},seen=[],delays=[];let degraded=input.op==='reconcile'?input.degraded:false,warned=input.warned,disposed=input.disposed,kind=input.old?'old':'none',reads=0,queued=null,handle=null,timerSame=false,clearedSame=false;
 let callbackRan=false,setupError=false,callbackError=false,markCount=0,launchCount=0;
 const reason=value(input.reason),force=value(input.force),oldHandle={id:'old-guard',units},clearHandle={id:'old-argument',units},newHandle={id:'new',units};
 const record=name=>{trace.push(name);if(trace.length-1===input.fail)throw error;};
 let primitiveReads=0;
 const clock={number:10,valueOf(){record(primitiveReads++===0?'coerce-current':'coerce-captured');return this.number;}};
 const newer={number:20,valueOf(){record(primitiveReads++===0?'coerce-current':'coerce-captured');return this.number;}};let current=clock;
 const target=new Source();
 Object.defineProperties(target,{
  isPrivateStreamDegraded:{get(){record('degraded?');return degraded;},set(v){record('degraded:'+v);degraded=v;}},
  hasWarnedPrivateStreamDegraded:{get(){record('warned?');return warned;},set(v){record('warned:'+v);warned=v;}},
  isDisposed:{get(){record('disposed?');return disposed;}},
  lastPrivateStreamMessageAt:{get(){record('frame?');return current;}},
 });
 const timerKey=input.op==='write'?'privateStreamWriteCheckTimeout':'privateStreamDegradedReconcileTimeout';
 Object.defineProperty(target,timerKey,{get(){record('timer?');reads++;return kind==='new'?newHandle:kind==='old'?(reads===1?oldHandle:clearHandle):0;},set(v){if(v===undefined){record('unset');kind='none';}else{assert.equal(v,newHandle);record('assign');timerSame=true;kind='new';}}});
 target.store={updateSocketHealth(patch){assert(Object.hasOwn(patch,'privateDegradedReason'));seen.push({at:'health',...reasonWire(patch.privateDegradedReason,reason)});record('health');}};
 target.log=(v,s)=>{assert.equal(s,severity.Warning);seen.push({at:'log',...reasonWire(v,reason),severity:s});record('log');};
 target.privateWebsocket={recoverMissingPrivateStream(v){seen.push({at:'recover',...reasonWire(v,reason)});record('recover');}};
 target.startPrivateStreamDegradedReconcile=()=>record('start');
 target.emitter={emit(name,v,options){assert.equal(name,'warning');seen.push({at:'emit',...reasonWire(v,reason),force:options.forceToast,own:Object.hasOwn(options,'forceToast')});record('emit');}};
 target.reconcilePrivateStateFromRest=()=>{record('launch');launchCount++;return {then(){throw Error('TIMERS launch must not await');}};};
 if(input.op==='write')target.markPrivateStreamDegraded=(v,f)=>{seen.push({at:'mark',...reasonWire(v,reason),force:f});record('mark');markCount++;};
 const mutate=()=>{disposed=input.callbackDisposed;if(input.op==='reconcile'){degraded=false;}else if(input.mode==='newer'){current=newer;}else if(input.mode==='alias'){clock.number=30;current=newer;}else if(input.mode==='nan'){newer.number=NaN;current=newer;}};
 const oldSet=globalThis.setTimeout,oldClear=globalThis.clearTimeout;
 globalThis.clearTimeout=v=>{assert.equal(v,clearHandle,'TIMERS source second timer read');clearedSame=true;record('clear');};
 globalThis.setTimeout=(fn,delay)=>{delays.push(delay);record('set');if(input.inline){mutate();callbackRan=true;fn();}else queued=fn;handle=newHandle;return newHandle;};
 try{if(input.op==='mark')target.markPrivateStreamDegraded(reason,force);else target[input.op==='write'?names[2]:names[1]]();}catch(e){assert.equal(e,error);setupError=true;}
 if(!setupError&&queued){const fn=queued;queued=null;mutate();callbackRan=true;try{fn();}catch(e){assert.equal(e,error);callbackError=true;}}
 globalThis.setTimeout=oldSet;globalThis.clearTimeout=oldClear;
 return {input,output:{trace,seen,delays,degraded,warned,kind,timerSame,clearedSame,setupError,callbackError,callbackRan,markCount,launchCount,pending:queued!==null}};
}
const scenarios=[];
for(const reason of ['undefined','text','object','null'])for(const force of ['undefined','false','true','object'])for(const warned of [false,true])scenarios.push({op:'mark',reason,force,warned,old:false,disposed:false,degraded:false,callbackDisposed:false,mode:'equal',inline:false,floor:(force==='true'||force==='object')?(warned?6:8):5});
for(const disposed of [false,true])for(const degraded of [false,true])for(const old of [false,true])scenarios.push({op:'reconcile',reason:'undefined',force:'undefined',warned:false,old,disposed,degraded,callbackDisposed:true,mode:'equal',inline:false,floor:disposed?1:!degraded?2:old?3:7});
for(const old of [false,true])for(const callbackDisposed of [false,true])for(const mode of ['equal','newer','alias','nan'])for(const inline of [false,true])scenarios.push({op:'write',reason:'undefined',force:'undefined',warned:false,old,disposed:false,degraded:false,callbackDisposed,mode,inline,floor:(old?6:4)+(callbackDisposed?2:mode==='newer'?5:6)});
const cases=[];for(const {floor,...input} of scenarios)for(let fail=-1;fail<floor;fail++)cases.push(run({...input,fail}));
assert.equal(cases.length,566,'TIMERS fixed source floor');console.log(JSON.stringify({cases}));
