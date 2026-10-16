const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'LIFECYCLE compiler path required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/cryptocom');
const source=path.join(root,'cryptocom.ws-private.ts');
const ast=ts.createSourceFile(source,fs.readFileSync(source,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='CryptocomPrivateWebSocket');assert(klass);
const names=['connectAndSubscribe','onOpen','authenticate','resubscribe','subscribeToOrderUpdates','subscribeToPositionUpdates','subscribeToBalanceUpdates','unsubscribe','dispose'];
const factories={};for(const name of names){const matches=klass.members.filter(n=>n.name?.getText(ast)===name);assert.equal(matches.length,1);assert(ts.isArrowFunction(matches[0].initializer));const compiled=ts.transpileModule('return ('+matches[0].initializer.getText(ast)+');',{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);factories[name]=new Function('WSS_URLS','virtualClock','generateWsAuthMessage','clearTimeout','setTimeout','"use strict";\n'+compiled.outputText);}
const types=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'cryptocom.types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const urlsInitializer=types.statements.flatMap(n=>ts.isVariableStatement(n)?Array.from(n.declarationList.declarations):[]).find(n=>n.name.getText(types)==='WSS_URLS').initializer;
const realUrls=new Function('return ('+urlsInitializer.getText(types)+')')();
const num=v=>v==='NaN'?NaN:v==='neg0'?-0:v;
const str=v=>typeof v==='string'?v:String.fromCharCode(...v.units);
const value=v=>v?.undefined||v?.hole?undefined:str(v);
const array=spec=>{const a=new Array(spec.length);spec.forEach((v,i)=>{if(!v?.hole)a[i]=value(v);});return a;};
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const observe=v=>v===undefined?{kind:'undefined'}:{kind:'string',units:units(v)};
const slots=a=>Array.from({length:a.length},(_,i)=>Object.hasOwn(a,i)?{kind:'value',value:observe(a[i])}:{kind:'hole'});
const fixtures=JSON.parse(fs.readFileSync(path.join(__dirname,'private_lifecycle_fixtures.json'),'utf8'));assert.equal(fixtures.length,84,'LIFECYCLE independent input floor');
const descriptor=Object.getOwnPropertyDescriptor(JSON,'stringify'),original=descriptor.value;
(async()=>{const rows=[];for(const spec of fixtures){
 const events=[],frames=[],urlsUsed=[],emitted=[],callbackResults=[],authMessages=[],closed=[],listeners=[];
 const input=array(spec.input??[]),initial=spec.count!==undefined?Array.from({length:spec.count},(_,i)=>'channel-'+i):array(spec.initial??[]);
 const state={subscriptions:new Set(initial),messageId:num(spec.messageId??1),isDisposed:!!spec.disposed,isAuthenticated:spec.authenticated??true,reconnectAuthTimer:spec.noTimer?undefined:{tag:'old-timer'}};
 const retained=state.subscriptions,oldTimer=state.reconnectAuthTimer,authMessage={opaque:true},originalError=Error('original '+spec.name),catchError=Error('catch '+spec.name);
 originalError.toString=()=>{event('coerce');catchFail('coerce');return spec.errorText?str(spec.errorText):'original '+spec.name;};
 const event=phase=>{events.push(phase);if(spec.fail===phase)throw originalError;};
 const catchFail=phase=>{if(spec.catchFail===phase)throw catchError;};
 let timerCallback,clockCount=0;
 const sockets={};for(const tag of ['old-socket','new-socket','replacement-socket']){const socket={tag};Object.defineProperty(socket,'addEventListener',{configurable:true,get(){event('resolve-add-'+currentEvent);return function(ev,handle){assert.strictEqual(this,socket);assert.strictEqual(handle,handles[ev]);event('add-'+ev);listeners.push([socket.tag,ev]);if(spec.replaceOnListener&&ev==='open')state.ws=sockets['replacement-socket'];};}});Object.defineProperty(socket,'close',{get(){event('resolve-close');return function(){assert.strictEqual(this,socket);event('close');closed.push(socket.tag);};}});sockets[tag]=socket;}
 state.ws=spec.noSocket?undefined:sockets['old-socket'];let currentEvent;
 const handles={open:{event:'open'},message:{event:'message'},close:{event:'close'}};
 for(const ev of ['open','message','close'])Object.defineProperty(state,'on'+ev[0].toUpperCase()+ev.slice(1),{get(){currentEvent=ev;event('listener-'+ev);return handles[ev];}});
 // addEventListener is resolved before its second argument; record the active event by call order.
 let listenerOrdinal=0;for(const socket of Object.values(sockets)){const d=Object.getOwnPropertyDescriptor(socket,'addEventListener');Object.defineProperty(socket,'addEventListener',{...d,get(){currentEvent=['open','message','close'][listenerOrdinal++];return d.get.call(this);}});}
 const options={};Object.defineProperty(options,'testnet',{get(){event('testnet');return spec.testnet??false;}});options.key='KEY';options.secret='SECRET';
 const parent={options};Object.defineProperty(parent,'emitAndLogError',{get(){event('resolve-emit');catchFail('emit-get');return function(message){assert.strictEqual(this,parent);event('emit');emitted.push(units(message));catchFail('emit');};}});state.parent=parent;
 const urlDeps={};for(const environment of ['PRODUCTION','TESTNET'])Object.defineProperty(urlDeps,environment,{get(){event('urls');const carrier={};Object.defineProperty(carrier,'USER',{get(){event('user-url');return realUrls[environment].USER;}});return carrier;}});
 Object.defineProperty(state,'createWebsocket',{get(){event('resolve-create');return function(url){assert.strictEqual(this,state);event('create');urlsUsed.push(units(url));assert.equal(url,realUrls[spec.testnet?'TESTNET':'PRODUCTION'].USER);return sockets['new-socket'];};}});
 state.trackLastReceived=()=>{event('track');if(spec.disposeDuringTrack)state.isDisposed=true;};
 const clear=timer=>{assert.strictEqual(timer,oldTimer);event('clear');if(spec.replaceSocketAtClear)state.ws=sockets['replacement-socket'];if(spec.clearSocketAtClear)state.ws=undefined;};
 const schedule=(callback,delay)=>{event('schedule');assert.equal(delay,1000);timerCallback=callback;if(spec.synchronousTimer){assert.equal(callback(),undefined);}return{tag:'new-timer'};};
 Object.defineProperty(state,'authenticate',{get(){event('timer-auth-get');return function(){assert.strictEqual(this,state);event('timer-auth-call');if(spec.rejectedAuthPromise){const promise=Promise.reject(originalError);promise.catch(()=>{});return promise;}return{ignored:true};};}});
 const clock={getCurrentTimeMs(){event('clock');clockCount++;if(spec.growAtClock)input.push(...array(spec.growAtClock));return 1700000000000+13*clockCount;}};
 Object.defineProperty(state,'send',{get(){event('resolve-send');if(spec.replaceSet)state.subscriptions=new Set(array(spec.replaceSet));if(spec.resetAuth)state.isAuthenticated=false;return function(wire){assert.strictEqual(this,state);assert.equal(typeof wire,'string');event('send');if(spec.growAtSend)input.push(...array(spec.growAtSend));if(spec.replaceSetAtSend)state.subscriptions=new Set(array(spec.replaceSetAtSend));if(spec.foreachGetterThrow)Object.defineProperty(input,'1',{configurable:true,get(){event('foreach-get-1');throw originalError;}});};}});
 if(spec.customForeach)Object.defineProperty(input,'forEach',{get(){event('resolve-foreach');return function(callback){assert.strictEqual(this,input);event('foreach');for(const channel of input)callbackResults.push(callback(channel));};}});
 Object.defineProperty(JSON,'stringify',{configurable:true,get(){event('resolve-json');return function(frame){events.push('stringify');if(spec.op==='auth'){assert.strictEqual(frame,authMessage);authMessages.push(true);}else{if(spec.op==='unsubscribe')assert.strictEqual(frame.params.channels,input);frames.push({id:bits(frame.id),method:units(frame.method),channels:slots(frame.params.channels),nonce:bits(frame.nonce)});}if(spec.fail==='stringify')throw originalError;return original.call(JSON,frame);};}});
 const generate=(key,secret)=>{event('generate');assert.equal(key,'KEY');assert.equal(secret,'SECRET');return authMessage;};
 const actual={};for(const name of names)actual[name]=factories[name].call(state,urlDeps,clock,generate,clear,schedule);
 const snapshot=()=>({subscriptions:Array.from(state.subscriptions,observe),retainedSubscriptions:Array.from(retained,observe),messageId:bits(state.messageId),disposed:state.isDisposed,authenticated:state.isAuthenticated,retainedTimer:oldTimer?.tag??null,urlsUsed:[...urlsUsed],timer:state.reconnectAuthTimer?.tag??null,socket:state.ws?.tag??null,events:[...events],frames:[...frames],emitted:[...emitted],authMessages:[...authMessages],listeners:[...listeners],closed:[...closed],callbackResults:[...callbackResults],input:Array.from({length:input.length},(_,i)=>{const d=Object.getOwnPropertyDescriptor(input,String(i));return !d?{kind:'hole'}:{kind:'value',value:observe(d.get?value(spec.input[i]):d.value)};})});
 let out,immediate,result='void',timerResult='not-fired';
 try{switch(spec.op){case'connect':out=actual.connectAndSubscribe();break;case'open':out=actual.onOpen();break;case'auth':if(spec.errorSymbol){actual.authenticate=factories.authenticate.call(state,urlDeps,clock,()=>{events.push('generate');throw Symbol('error');},clear,schedule);}out=actual.authenticate();break;case'replay':out=actual.resubscribe();break;case'order':out=actual.subscribeToOrderUpdates();break;case'position':out=actual.subscribeToPositionUpdates();break;case'balance':out=actual.subscribeToBalanceUpdates();break;case'unsubscribe':out=actual.unsubscribe(input);break;case'dispose':out=actual.dispose();break;default:throw Error('unknown operation');}immediate=snapshot();if(['connect','auth','replay'].includes(spec.op)){assert(out instanceof Promise);assert.equal(await out,undefined);}else assert.equal(out,undefined);
 }catch(error){immediate??=snapshot();if(error===originalError)result='original';else if(error===catchError)result='catch-body';else if(spec.errorSymbol&&error instanceof TypeError)result='coercion';else throw error;}
 if(spec.op==='open'&&spec.fireTimer&&timerCallback){if(spec.disposeBeforeFire)state.isDisposed=true;try{assert.equal(timerCallback(),undefined);timerResult='void';}catch(error){assert.strictEqual(error,originalError);timerResult='original';}}
 Object.defineProperty(JSON,'stringify',descriptor);
 const final=snapshot();assert.equal(final.frames.length,spec.expectedFrames,'LIFECYCLE independent source frame floor '+spec.name);assert.equal(final.authMessages.length,spec.expectedAuthMessages,'LIFECYCLE independent auth floor '+spec.name);
 rows.push({name:spec.name,immediate,final,result,timerResult});
 }
 assert.equal(rows.length,84);process.stdout.write(original(rows));})().catch(error=>{Object.defineProperty(JSON,'stringify',descriptor);console.error(error);process.exitCode=1;});
