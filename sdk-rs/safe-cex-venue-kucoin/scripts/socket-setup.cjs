const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),{root,load}=require('./tables.cjs');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const input=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(input.connects.length,76);assert.equal(input.reconnects.length,16);assert.equal(input.constructors.length,8);assert.equal(input.pools.length,10);assert.equal(input.subscriptions.length,6);
function selected(file,names,constructor=false){
 const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration);
 const members=names.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+name);return m.getText(ast);});
 if(constructor){members.unshift(...cls.members.filter(n=>['messageHandlers','token','socketEndpoint','role','shardIndex','shardCount'].includes(n.name?.getText(ast))).map(n=>n.getText(ast)));const ctor=cls.members.find(ts.isConstructorDeclaration);assert.ok(ctor);members.push(ctor.getText(ast));}
 return ts.transpileModule(`export class Selected ${constructor?'extends BaseWebSocket':''} {${members.join('\n')}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
}
const files=['kucoin.ws-public','kucoin.ws-private','kucoinspot.ws-public','kucoinspot.ws-private'].map(name=>path.join(root,'packages/safe-cex/src/exchanges/kucoin',name+'.ts'));
const connectionCodes=files.map(file=>selected(file,['connectAndSubscribe'])),reconnectCodes=files.slice(0,2).map(file=>selected(file,['reconnect']));
function owner(code,sandbox={}){const module={exports:{}};vm.runInNewContext(code,{...sandbox,module,exports:module.exports});return new module.exports.Selected();}
function wire(v){if(v===undefined)return {undefined:true};if(v===null)return {null:true};if(typeof v==='string')return {text:units(v)};if(typeof v==='number')return {number:v};if(typeof v==='boolean')return {bool:v};return v.native||{object:v.tag};}
function error(){return Object.assign(new TypeError('setup failure'),{code:'SETUP',ids:[7,3]});}
async function connection(c){
 const {kind,scenario}=c,spot=kind>=2,privateSocket=kind%2===1,events=[],failure=error();let disposed=scenario==='disposed',token='OLD',endpoint='OLD-ENDPOINT',ping=5000,socket={tag:'old'};
 const boxed=(s,label)=>({native:{text:units(s)},[Symbol.toPrimitive](){events.push('format:'+label);if(label==='endpoint'&&scenario==='endpoint-reenter')token=boxed('NEW','token');return s;}});
 const endpointValue=boxed('wss://\ud800','endpoint'),tokenValue=boxed('TK','token');
 const server={get endpoint(){events.push('get:server.endpoint');if(scenario==='endpoint-throws')throw failure;if(scenario==='missing-endpoint'){if(!spot)events.push('format:token');return undefined;}return endpointValue;},get pingInterval(){events.push('get:server.pingInterval');return scenario==='ping-zero'?0:7000;}};
 const servers={get length(){events.push('get:servers.length');return scenario==='zero-length'?0:1;},get 0(){events.push('get:servers.0');return server;}};
 const bucket={get token(){events.push('get:bucket.token');return tokenValue;},get instanceServers(){if(spot)events.push('get:bucket.instanceServers?.0');events.push('get:bucket.instanceServers');if(scenario==='no-servers'){if(spot)events.push('get:optional.endpoint');return null;}return servers;}};
 const data={get code(){events.push('get:data.code');return scenario==='numeric-code'?200000:['bad-code','emit-error-throws'].includes(scenario)?'bad':'200000';},get data(){events.push('get:data.data');return scenario==='missing-data'?undefined:bucket;},get msg(){events.push('get:data.msg');return 'venue message';}};
 const response={get data(){events.push('get:response.data');return data;}};
 const endpoints={...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts')).ENDPOINTS},spotEndpoints={...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts')).KUCOIN_SPOT_ENDPOINTS},liveUrl={tag:'endpoint-url'};
 const receiver=owner(connectionCodes[kind],{ENDPOINTS:endpoints,KUCOIN_SPOT_ENDPOINTS:spotEndpoints,Date:{now(){events.push('date');return {[Symbol.toPrimitive](){events.push('format:date');return 41;}};}}});
 Object.defineProperties(receiver,{isDisposed:{get(){events.push('disposed');if(scenario==='guard-error')throw failure;return disposed;}},token:{get(){events.push('state:token');return token;},set(v){assert.equal(v,tokenValue);events.push('set:token');token=v;}},socketEndpoint:{get(){events.push('state:socketEndpoint');return endpoint;},set(v){assert.equal(typeof v,'string');events.push('set:socketEndpoint');endpoint=v;}},pingInterval:{get(){events.push('state:pingInterval');return ping;},set(v){events.push('set:pingInterval');ping=v;}},ws:{get(){return socket;},set(v){assert.equal(v,newSocket);events.push('set:ws');socket=v;}},createWebsocket:{get(){events.push('create:get');return value=>{assert.equal(value,endpoint);events.push('create:call');return newSocket;};}}});
 const listener=(event,handler)=>{assert.equal(handler,receiver['on'+event[0].toUpperCase()+event.slice(1)]);events.push('listener:'+event);if(scenario==='listener-replace'&&event==='open')socket=replacement;};
 const newSocket={tag:'new',addEventListener:listener},replacement={tag:'replacement',addEventListener:listener};receiver.onOpen=()=>{};receiver.onMessage=()=>{};receiver.onClose=()=>{};
 receiver.parent={xhr:{post(url){if(scenario==='live-endpoint'){assert.equal(url,liveUrl);events.push('post:opaque');}else{assert.equal(url,privateSocket?'/api/v1/bullet-private':'/api/v1/bullet-public');events.push('post:'+url);}if(scenario==='post-sync-error')throw failure;return Promise.resolve().then(()=>{events.push('resolved');if(scenario==='dispose-await')disposed=true;if(['post-reject','retry-throws'].includes(scenario))throw failure;return response;});}},emitAndLogError(message){events.push('emit:'+message);if(scenario==='emit-error-throws')throw failure;}};
 if(scenario==='live-endpoint'){const actualPost=receiver.parent.xhr.post;Object.defineProperty(receiver.parent.xhr,'post',{get(){events.push('post:get');Object.defineProperty(spot?spotEndpoints:endpoints,privateSocket?'GET_PRIVATE_WS_TOKEN':'GET_PUBLIC_WS_TOKEN',{get(){events.push('endpoint:get');return liveUrl;},configurable:true});return actualPost;}});}
 receiver.retryFailedConnect=message=>{assert.equal(message,failure.message);events.push('retry:'+message);if(scenario==='retry-throws')throw failure;};
 let throws=false;try{await receiver.connectAndSubscribe();}catch(e){assert.equal(e,failure);assert.deepEqual(e.ids,[7,3]);assert.equal(e.code,'SETUP');throws=true;}
 return {kind,scenario,events,token:wire(token),endpoint:wire(endpoint),ping:wire(ping),socket:wire(socket),throw:throws};
}
async function reconnect(c){
 const {private:privateSocket,scenario}=c,events=[],failure=error(),old={tag:17};let socket=scenario==='null-socket'?null:old;
 const receiver=owner(reconnectCodes[privateSocket?1:0]);receiver.isDisposed=scenario==='disposed';
 Object.defineProperty(receiver,'ws',{get(){events.push('capture');return socket;},set(v){assert.equal(v,undefined);events.push('ws:undefined');socket=v;}});
 for(const [field,value] of [['token',null],['socketEndpoint',null],['subscribed',false]])Object.defineProperty(receiver,field,{set(v){assert.equal(v,value);events.push(field+':'+v);}});
 old.removeEventListener=(event,handler)=>{assert.equal(handler,receiver['on'+event[0].toUpperCase()+event.slice(1)]);events.push(event);if(scenario==='remove-error'&&event==='message')throw failure;if(scenario==='replace-socket'&&event==='open')socket={tag:9};};
 old.close=()=>{assert.equal(socket,undefined);events.push('close-socket');if(scenario==='close-error')throw failure;};receiver.onOpen=()=>{};receiver.onMessage=()=>{};receiver.onClose=()=>{};
 receiver.connectAndSubscribe=()=>{events.push('connect');if(scenario==='connect-sync-error')throw failure;return scenario==='connect-reject'?Promise.reject(failure):Promise.resolve();};
 let throws=false;try{await receiver.reconnect();}catch(e){assert.equal(e,failure);throws=true;}return {...c,events,throw:throws};
}
function constructor(c){
 const {scenario}=c,events=[],fields=[],failure=error(),parent={payload:17};let initialized=false;
 class BaseWebSocket {constructor(p){assert.equal(p,parent);events.push('super');if(scenario==='super-error')throw failure;return new Proxy(this,{defineProperty(target,key,descriptor){if(!initialized){initialized=true;events.push('initialize');}return Reflect.defineProperty(target,key,descriptor);},set(target,key,value){if(['role','shardIndex','shardCount'].includes(key)){if(['absent','null'].includes(scenario))events.push('get:'+key);events.push('set:'+key);fields.push([key,wire(value)]);}return Reflect.set(target,key,value);}});}}
 const code=selected(files[0],[],true),module={exports:{}};vm.runInNewContext(code,{BaseWebSocket,module,exports:module.exports});
 const opts=scenario==='absent'?undefined:scenario==='null'?null:new Proxy({},{get(_,field){events.push('get:'+field);if((scenario==='role-error'&&field==='role')||(scenario==='index-error'&&field==='shardIndex'))throw failure;if(scenario==='false-zero')return field==='role'?false:0;if(scenario==='values')return field==='role'?'snapshot':field==='shardIndex'?2:3;return undefined;}});
 let throws=false;try{const value=new module.exports.Selected(parent,opts);assert.equal(value.token,null);assert.equal(value.socketEndpoint,null);assert.deepEqual(Object.keys(value.messageHandlers),[]);}catch(e){assert.equal(e,failure);throws=true;}return {scenario,events,fields,throw:throws};
}
function pool(c){
 const {scenario}=c,events=[],failure=error(),parent={payload:17};const ast=ts.createSourceFile(files[0],fs.readFileSync(files[0],'utf8'),ts.ScriptTarget.Latest,true);
 const statements=['MAX_SNAPSHOT_SYMBOLS_PER_CONNECTION','createKucoinSnapshotWebsockets'].map(name=>{const s=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(s);return s.getText(ast);});
 class KucoinPublicWebsocket {constructor(p,opts){assert.equal(p,parent);assert.equal(opts.role,'snapshot');assert.equal(opts.shardCount,scenario==='761'||scenario==='construct-error'?3:scenario==='381'||scenario==='760'||scenario==='fraction'?2:1);events.push(opts.shardIndex);if(scenario==='construct-error'&&opts.shardIndex===1)throw failure;this.index=opts.shardIndex;}}
 const module={exports:{}};vm.runInNewContext(ts.transpileModule(statements.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{module,exports:module.exports,KucoinPublicWebsocket});
 const count={negative:-1,zero:0,'380':380,'381':381,'760':760,'761':761,fraction:380.5,nan:NaN,infinity:Infinity,'construct-error':761}[scenario];let throws=false;
 try{const sockets=module.exports.createKucoinSnapshotWebsockets(parent,count);assert.equal(sockets.length,{negative:1,zero:1,'380':1,'381':2,'760':2,'761':3,fraction:2,nan:0}[scenario]);sockets.forEach((v,i)=>assert.equal(v.index,i));}catch(e){if(scenario==='infinity'){assert.equal(e.name,'RangeError');assert.equal(e.message,'Invalid array length');assert.equal(e.code,undefined);assert.equal(e.ids,undefined);}else assert.equal(e,failure);throws=true;}
 return {scenario,events,throw:throws};
}
function subscription(c){
 const {scenario}=c,topics=[],failure=error(),receiver=owner(selected(files[2],['subscribe']));receiver.messageHandlers={'candle_BTC_1min':[],orderBook_BTC:[],candle_ETH:[],'candle_X__extra':[],['candle_\ud800_1']:[],other:[]};let count=0;
 receiver.subscribeTopic=topic=>{topics.push(units(topic));count++;if(scenario==='mutate-first'&&count===1)receiver.messageHandlers.orderBook_NEW=[];if(scenario==='mutate-later'&&count===2)receiver.messageHandlers={};if((scenario==='first-error'&&count===1)||(scenario==='candle-error'&&count===2))throw failure;if(scenario==='enumerate-error'&&count===1)receiver.messageHandlers=new Proxy({},{ownKeys(){throw failure;}});};
 let throws=false;try{receiver.subscribe();}catch(e){assert.equal(e,failure);throws=true;}return {scenario,topics,throw:throws};
}
(async()=>{const result={connects:[],reconnects:[],constructors:[],pools:[],subscriptions:[]};for(const c of input.connects)result.connects.push(await connection(c));for(const c of input.reconnects)result.reconnects.push(await reconnect(c));for(const c of input.constructors)result.constructors.push(constructor(c));for(const c of input.pools)result.pools.push(pool(c));for(const c of input.subscriptions)result.subscriptions.push(subscription(c));process.stdout.write(JSON.stringify(result));})().catch(e=>{console.error(e);process.exitCode=1;});
