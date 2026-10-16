const assert=require('node:assert/strict'), fs=require('node:fs'), path=require('node:path'), vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'isolated compiler required');const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/polymarket');
const constants=fs.readFileSync(path.join(root,'polymarket.constants.ts'),'utf8'), ca=ts.createSourceFile('constants.ts',constants,ts.ScriptTarget.Latest,true);
const declarations=['WS_CONFIG','WS_MESSAGE_TYPES'].map(name=>{const found=ca.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ca)===name));assert.equal(found.length,1,name);return found[0].getText(ca);}).join('\n');
const severity=fs.readFileSync(path.resolve(root,'../../../../logging/src/LogSeverity.ts'),'utf8');
const units=value=>Array.from({length:value.length},(_,index)=>value.charCodeAt(index));
const strings=[['api-A','secret-B','phrase-C'],['','',''],['🦀 A','λ B','☃ C'],['api','secret','phrase'],['"\\\b\t\n\f\r\0','s\\"','p\t'],['\ud800','x\udfff','pair😀'],['line\u2028','next\u2029','\u007f\u0085'],['high\ud800then','lo\udfff','trail\\'],['pair\ud800\udfff','wrong\udfff\ud800',''],['final-A','final-B','final-C']];
const cases=[];
for(const venue of ['public','private']){
 const text=fs.readFileSync(path.join(root,`polymarket.ws-${venue}.ts`),'utf8'), ast=ts.createSourceFile('ws.ts',text,ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===`Polymarket${venue==='public'?'Public':'Private'}WebSocket`);
 const names=['onOpen','onClose','ping',venue==='public'?'sendSubscription':'authenticate'];
 const methods=names.map(name=>{const found=klass.members.filter(n=>n.name?.getText(ast)===name);assert.equal(found.length,1,name);return found[0].getText(ast);});
 for(const method of names)for(let scenario=0;scenario<10;scenario++){
  const trace=[],timers=[], failure={marker:47,message:'original host failure'};
  const failurePoint=scenario===4?'log':scenario===5?'send':scenario===6?'timeout':scenario===7?'diagnostics':scenario===8?(venue==='private'&&['authenticate','onOpen'].includes(method)?'credentials':'clearPing'):'';
  let instance, callbackFailure=false, pendingFrame=null, socket={id:83};
  const credentials=scenario===3?null:{apiKey:strings[scenario][0],secret:strings[scenario][1],passphrase:strings[scenario][2]};
  const assetSet=new Set(scenario===1?[]:['base',strings[scenario][0],strings[scenario][1],'base']);
  const frame=value=>({channel:value.type,assets:value.assets_ids?.map(units)??null,feature:value.custom_feature_enabled??null,auth:value.auth?{apiKey:units(value.auth.apiKey),secret:units(value.auth.secret),passphrase:units(value.auth.passphrase)}:null});
  function event(tag,extra={}){trace.push({tag,...extra});if(failurePoint===tag||(callbackFailure&&['childPing','childConnect'].includes(tag)))throw failure;}
  const json={...JSON,stringify(value){pendingFrame=frame(value);return JSON.stringify(value);}};
  const context={exports:{},JSON:json,setTimeout(callback,delay){event('timeout',{delay});const handle={id:23+timers.length};timers.push({callback,delay,handle});return handle;}};
  vm.runInNewContext(ts.transpileModule(`${severity}\n${declarations}\nexports.Socket=class {${methods.join('\n')}};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
  instance=new context.exports.Socket();instance.isDisposed=scenario===2;instance.isConnected=scenario!==3;instance.pingInterval={id:17};
  Object.defineProperty(instance,'ws',{get(){return socket;},set(value){assert.equal(value,undefined,'source clears socket field');event('clearSocket');socket=undefined;if(scenario===9)instance.isDisposed=true;}});
  Object.defineProperty(instance,'subscribedAssets',{get(){event('assets');return assetSet;}});
  instance.getCredentials=()=>{event('credentials');return credentials;};
  instance.parent={log(text,severity){event('log',{text,severity});if(scenario===9&&severity==='info')instance.isDisposed=true;}};
  instance.send=text=>{const current=pendingFrame;pendingFrame=null;event('send',{text,frame:current});};
  instance.recordCloseDiagnostics=(eventValue,options)=>{assert.equal(eventValue,undefined);assert.equal(options.syntheticIfNoSocket,true);event('diagnostics',{eventUndefined:eventValue===undefined,synthetic:options.syntheticIfNoSocket});};
  instance.clearPingInterval=()=>{event('clearPing');instance.pingInterval=undefined;};
  instance.connectAndSubscribe=()=>event('childConnect');instance.emitWsReconnected=()=>event('reconnected');
  // Capture the real root ping body, then resolve its callback's receiver ping
  // at an opaque child-call boundary, avoiding a timer recursion in the oracle.
  const rootPing=instance.ping;instance.ping=()=>event('childPing');
  const steps=[];
  for(let iteration=0;iteration<2;iteration++){
   instance.isDisposed=scenario===2;instance.isConnected=scenario!==3;
   let error=false;try{assert.equal(method==='ping'?rootPing():instance[method](),undefined);}catch(caught){assert.strictEqual(caught,failure);error=true;}
   const ticks=[],timer=timers.at(-1);
   if(timer){
    const reconnect=method==='onClose';
    for(const [disposed,connected]of[[false,true],[false,false],[true,true],[true,false]]){
     instance.isDisposed=disposed;instance.isConnected=connected;const before=trace.length;let tickError=false;
     try{timer.callback();}catch(caught){assert.strictEqual(caught,failure);tickError=true;}
     assert.equal(trace.length-before,reconnect||(!disposed&&connected)?1:0,'independent callback guard');
     assert.equal(tickError,false);ticks.push({disposed,connected,error:tickError});
    }
    instance.isDisposed=false;instance.isConnected=true;callbackFailure=true;const before=trace.length;
    try{timer.callback();assert.fail('callback failure required');}catch(caught){assert.strictEqual(caught,failure);}
    assert.equal(trace.length-before,1);ticks.push({disposed:false,connected:true,error:true});callbackFailure=false;
   }
   let oldTick=false;
   if(method==='ping'&&iteration===1&&timers.length===2){
    instance.isDisposed=false;instance.isConnected=true;const before=trace.length;
    timers[0].callback();assert.equal(trace.length-before,1,'older timeout callback remains executable after replacement');oldTick=true;
   }
   steps.push({oldTick,trace:structuredClone(trace),error,pingHandle:instance.pingInterval?.id??null,socket:socket?.id??null,registered:timers.length,ticks});
  }
  cases.push({venue,method,scenario,failurePoint,strings:strings[scenario].map(units),steps});
 }
}
assert.equal(cases.length,80,'independent eight-method ten-scenario floor');assert.equal(cases[79].venue,'private');assert.equal(cases[79].method,'authenticate');assert.equal(cases[79].scenario,9,'last socket fixture');process.stdout.write(JSON.stringify(cases));
