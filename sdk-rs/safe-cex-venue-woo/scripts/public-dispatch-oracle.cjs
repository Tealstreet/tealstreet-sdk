const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function members(file,names){const source=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.members.some(m=>m.name?.getText(ast)===names[0]));return names.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);return m.getText(ast);}).join('\n');}
const body=members('packages/safe-cex/src/exchanges/woo/woo.ws-public.ts',['onMessage']);
const stats=members('packages/safe-cex/src/exchanges/base.ws.ts',['recordMessageHandlerStats']);
const parse=fs.readFileSync(path.join(root,'packages/json-parse/src/jsonParse.ts'),'utf8');
const names=['constructor','toString','toLocaleString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__'];
const inputs=[];
for(const event of [undefined,'ping','pong','subscribe','tickers',3,null])for(const topic of [undefined,'tickers','unknown',...names])inputs.push({frame:JSON.stringify({event,topic})});
for(const frame of ['null','false','0','1e400','[]','{}','"text"','{','{"topic":"tickers","n":-0}'])inputs.push({frame});
for(const options of [{disposed:true},{empty:true},{throwFirst:true},{removeKey:true},{append:true},{shadow:true}])for(const topic of ['tickers','constructor'])inputs.push({frame:JSON.stringify({topic}),...options});
const cases=inputs.map(input=>{
 const context=vm.createContext({input});
 const source=parse+'\nclass Witness {\n'+body+'\n'+stats+'\n}\n'+`
 const calls=[],recorded=[];let received=0;
 const socket=new Witness();socket.isDisposed=!!input.disposed;
 socket.messageHandlers={};socket.messageHandlersStats={};
 socket.trackLastReceived=()=>received++;
 socket.reconcileMessageHandlerStats=()=>{};
 Date.now=()=>123;
 const original=socket.recordMessageHandlerStats.bind(socket);
 socket.recordMessageHandlerStats=key=>{recorded.push(key);original(key)};
 for(const key of ['tickers','ping','pong',...(input.shadow?['constructor']:[])]){
  const callbacks=[()=>{calls.push(key+':0');if(input.removeKey)delete socket.messageHandlers[key];if(input.append)callbacks.push(()=>calls.push(key+':2'));if(input.throwFirst)throw new TypeError('callback failed')},()=>calls.push(key+':1')];
  Object.defineProperty(socket.messageHandlers,key,{value:input.empty?[]:callbacks,writable:true,configurable:true,enumerable:true});
 }
 let error=null;try{socket.onMessage({data:input.frame})}catch(e){error={name:e.name,message:e.message}}
 const observed=recorded.map(key=>{const entry=socket.messageHandlersStats[key];return {key,own:Object.hasOwn(socket.messageHandlersStats,key),count:entry.lastCallbacksCount,had:entry.hadHandlers}});
 globalThis.result={input,received,calls,error,stats:observed};`;
 const out=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 context.exports={};vm.runInContext(out,context);return JSON.parse(JSON.stringify(context.result));
});
const target=path.join(__dirname,'../tests/fixtures/public-dispatch.json');const bytes=JSON.stringify(cases,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO live public dispatch drift');else fs.writeFileSync(target,bytes);
console.log('WOO public dispatch cases:',cases.length);
// Capture the heartbeat methods through the actual base send implementation.
const heartbeatBody=members('packages/safe-cex/src/exchanges/woo/woo.ws-public.ts',['ping','handlePingEvent']);
const baseSend=members('packages/safe-cex/src/exchanges/base.ws.ts',['send']);
const basePath='packages/safe-cex/src/exchanges/base.ws.ts';
const baseSource=fs.readFileSync(path.join(root,basePath),'utf8');
const baseAst=ts.createSourceFile(basePath,baseSource,ts.ScriptTarget.Latest,true);
const classifier=baseAst.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='isHeartbeatFrame');
assert.ok(classifier,'actual heartbeat classifier');
const heartbeats=[];
for(const method of ['ping','handlePingEvent'])for(const disposed of [false,true])for(const ready of [null,0,1,2,3])for(const fail of [false,true]){
 const input={method,disposed,ready,fail};
 const context=vm.createContext({input,WebSocket:{OPEN:1},LogSeverity:{Debug:'debug',Warning:'warning'},performance:{now:()=>123}});
 const code=classifier.getText(baseAst)+'\nclass Witness {\n'+heartbeatBody+'\n'+baseSend+'\n}\n'+`
 const socket=new Witness(), sent=[],logs=[];let reconnects=0;
 socket.parent={isDisposed:input.disposed,log:(message,severity)=>logs.push({message,severity})};
 socket.isDisposed=input.disposed;socket.pingAt=7;socket.reconnect=()=>reconnects++;
 if(input.ready!==null)socket.ws={readyState:input.ready,send:message=>{sent.push(message);if(input.fail)throw new Error('send failure')}};
 let error=null;try{socket[input.method]()}catch(e){error={name:e.name,message:e.message}}
 globalThis.result={input,sent,logs,reconnects,pingAt:socket.pingAt,error};`;
 context.exports={};vm.runInContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
 heartbeats.push(JSON.parse(JSON.stringify(context.result)));
}
const heartbeatTarget=path.join(__dirname,'../tests/fixtures/public-heartbeats.json');
const heartbeatBytes=JSON.stringify(heartbeats,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(heartbeatTarget,'utf8'),heartbeatBytes,'WOO live public heartbeat drift');else fs.writeFileSync(heartbeatTarget,heartbeatBytes);
console.log('WOO public heartbeat cases:',heartbeats.length);

// Keep the actual Store writer capture in the existing WOO oracle gate.
require('./public-writes-oracle.cjs');

// Private heartbeats exercise the actual diagnostic wrapper, not a send stub.
require('./private-socket-oracle.cjs');
