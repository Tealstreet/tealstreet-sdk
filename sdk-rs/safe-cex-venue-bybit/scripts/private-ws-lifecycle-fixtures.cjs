const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function subject(family,enabled,trace){
 const [file,name]=family==='inverse'?['bybitinverse.ws-private.ts','BybitInversePrivateWebsocket']:['bybitspot.ws-private.ts','BybitPrivateWebsocket'];
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit',file),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const body=['onOpen','sendReconnectSequence','ping','subscribe'].map(n=>{const found=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(found.length,1,`${name}.${n}`);return found[0].getText(ast);}).join('\n');
 const context=vm.createContext({Error,ENABLE_RECONNECTING_WEBSOCKET:enabled,virtualClock:{},sleep:()=>{},performance:{now:()=>0}});
 vm.runInContext(ts.transpileModule(`class Subject {${body}} globalThis.subject=new Subject();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 return {api:context.subject,context};
}
function number(value){const b=Buffer.alloc(8);b.writeDoubleBE(value);return Number.isNaN(value)?'nan':b.toString('hex');}
const failure=(phase)=>Object.assign(new Error(`${phase} failed`),{name:`${phase}Error`,code:17});
const result=(e,errors)=>({error:e.name,message:e.message,code:e.code,originalPhase:Object.keys(errors).find(k=>errors[k]===e)||null});
async function main(){
 const opens=[],pings=[],subscriptions=[],reconnects=[];
 for(const family of ['inverse','spot'])for(const reject of [false,true]){
  const trace=[],{api}=subject(family,false,trace),original=failure('start');let started=0;
  api.sendReconnectSequence=()=>{started++;trace.push('start');if(reject)throw original;return new Promise(()=>{});};
  let outcome;try{const value=api.onOpen();outcome={ok:true,undefined:value===undefined};}catch(e){outcome=result(e,{start:original});}
  opens.push({family,reject,started,trace,result:outcome});
 }assert.equal(opens.length,4,'LIFECYCLE independent open floor');
 for(const family of ['inverse','spot'])for(const disposed of [false,true])for(const token of ['0','-0','-7.25','NaN','Infinity'])for(const phase of ['','clock','setter','send']){
  const trace=[],{api,context}=subject(family,false,trace),errors=Object.fromEntries(['clock','setter','send'].map(k=>[k,failure(k)]));let pingAt=3,reads=0,clockCalls=0,setterCalls=0,sends=[];
  Object.defineProperty(api,'isDisposed',{get(){reads++;trace.push('disposed');return disposed;}});
  Object.defineProperty(api,'pingAt',{set(value){setterCalls++;trace.push('setter');if(phase==='setter')throw errors.setter;pingAt=value;}});
  context.performance.now=()=>{clockCalls++;trace.push('clock');if(phase==='clock')throw errors.clock;return Number(token);};
  api.sendWithLog=frame=>{trace.push('send');sends.push(frame);if(phase==='send')throw errors.send;};
  let outcome;try{api.ping();outcome={ok:true};}catch(e){outcome=result(e,errors);}
  pings.push({family,disposed,token,phase,pingAt:number(pingAt),reads,clockCalls,setterCalls,sends,trace,result:outcome});
 }assert.equal(pings.length,80,'LIFECYCLE independent ping floor');
 for(const family of ['inverse','spot'])for(const reject of [false,true]){
  const trace=[],{api}=subject(family,false,trace),original=failure('send'),sends=[];
  api.sendWithLog=frame=>{trace.push('send');sends.push(frame);if(reject)throw original;};
  let outcome;try{api.subscribe();outcome={ok:true};}catch(e){outcome=result(e,{send:original});}
  subscriptions.push({family,reject,sends,trace,result:outcome});
 }assert.equal(subscriptions.length,4,'LIFECYCLE independent subscribe floor');
 const inputs=[];
 for(const disposed of [false,true])for(const enabled of [false,true])for(const auth of ['yes','no','throw'])inputs.push({disposed,enabled,auth,syncTime:true});
 inputs.push({disposed:false,enabled:true,auth:'yes',syncTime:false});
 for(const phase of ['sync','sleep','log','disregard','reconnect','subscribe','ping','snapshot'])inputs.push({disposed:false,enabled:true,auth:['log','disregard','reconnect'].includes(phase)?'throw':'yes',syncTime:false,phase});
 for(const disposeAt of ['sync','sleep','auth'])inputs.push({disposed:false,enabled:true,auth:'yes',syncTime:false,disposeAt});
 for(const enabled of [false,true])inputs.push({disposed:false,enabled,auth:'yes',syncTime:false,changeFlag:true});
 assert.equal(inputs.length,26,'LIFECYCLE independent reconnect input floor');
 for(const family of ['inverse','spot'])for(const input of inputs){
  const trace=[],{api,context}=subject(family,input.enabled,trace),errors=Object.fromEntries(['sync','sleep','auth','log','disregard','reconnect','subscribe','ping','snapshot'].map(k=>[k,failure(k)]));
  let disposed=input.disposed,reads=0,authCalls=0,disregards=0,reconnectStarts=0,snapshotStarts=0;const sleeps=[],sends=[],logs=[];api.pingAt=7;context.performance.now=()=>33;api.sendWithLog=frame=>{trace.push('send');sends.push(frame);};const subscribe=api.subscribe,ping=api.ping;
  Object.defineProperty(api,'isDisposed',{get(){reads++;trace.push('disposed');return disposed;}});
  context.virtualClock.syncBybitClientServerTime=async()=>{trace.push('sync');await Promise.resolve();if(input.disposeAt==='sync')disposed=true;if(input.phase==='sync')throw errors.sync;};
  context.sleep=async delay=>{trace.push('sleep');sleeps.push(delay);await Promise.resolve();if(input.disposeAt==='sleep')disposed=true;if(input.phase==='sleep')throw errors.sleep;};
  api.auth=async()=>{trace.push('auth');authCalls++;await Promise.resolve();if(input.disposeAt==='auth')disposed=true;if(input.changeFlag)context.ENABLE_RECONNECTING_WEBSOCKET=!input.enabled;if(input.auth==='throw')throw errors.auth;return input.auth==='yes';};
  api.emitAndLogError=e=>{trace.push('log');logs.push({name:e.name,message:e.message,code:e.code,sameAuth:e===errors.auth});if(input.phase==='log')throw errors.log;};
  api.directFallback={disregardSilence(){trace.push('disregard');disregards++;if(input.phase==='disregard')throw errors.disregard;}};
  api.reconnect=()=>{trace.push('reconnect');reconnectStarts++;if(input.phase==='reconnect')throw errors.reconnect;return new Promise(()=>{});};
  api.subscribe=()=>{trace.push('subscribe');if(input.phase==='subscribe')throw errors.subscribe;subscribe();};
  api.ping=()=>{trace.push('ping');if(input.phase==='ping')throw errors.ping;ping();};
  api.parent={fetchAndSetPrivateSnapshot(){trace.push('snapshot');snapshotStarts++;if(input.phase==='snapshot')throw errors.snapshot;return new Promise(()=>{});}};
  let outcome;const pending=api.sendReconnectSequence(input.syncTime),prefix=[...trace];
  try{await pending;outcome={ok:true};}catch(e){outcome=result(e,errors);}
  reconnects.push({family,input,disposed,pingAt:number(api.pingAt),reads,authCalls,disregards,reconnectStarts,snapshotStarts,sleeps,sends,logs,prefix,trace,result:outcome});
 }assert.equal(reconnects.length,52,'LIFECYCLE independent reconnect floor');
 process.stdout.write(JSON.stringify({opens,pings,subscriptions,reconnects}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
