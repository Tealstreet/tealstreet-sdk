'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const methodNames=['onOpen','startPingLoop','schedulePing','stopPingLoop','ping'];
let g,ids,tags,nextId,calls;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const tag=(v,t)=>(tags.set(v,t),v);
function native(v){
 if(v===undefined)return ['undefined'];if(v===null)return ['null'];
 if(typeof v==='boolean')return ['bool',v];if(typeof v==='string')return ['string',units(v)];
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',Number.isNaN(v)?'nan':b.toString('hex')];}
 if(!ids.has(v))ids.set(v,nextId++);return ['ref',ids.get(v),tags.get(v)||(v instanceof Error?'error:'+v.name:typeof v)];
}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(e0){e.throw=native(e0);throw e0;}}
const globals={};
const bind={
 __global:name=>op('global',{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;return Reflect.get(object,name);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;return undefined;}),
 __call:(fn,receiver,...args)=>op('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>{if(tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);}),
 __object:pairs=>{const object=op('object',{},()=>tag({},'literal'));for(const [name,value] of pairs)op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});});return object;},
 __callback:(socket,fn)=>op('callback',{socket:native(socket),kind:g.kind},()=>tag(fn,'pingCallback')),
};
const globalNames=new Set(['setTimeout','clearTimeout','performance','JSON','LogSeverity','HyperliquidPublicWebSocket','HyperliquidPrivateWebSocket']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);
 if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&['setTimeout','clearTimeout'].includes(n.expression.text)){
  const args=n.arguments.map(a=>ts.isArrowFunction(a)?call('__callback',[f.createThis(),ts.visitEachChild(a,visit,context)]):ts.visitNode(a,visit));
  return call('__call',[call('__global',[f.createStringLiteral(n.expression.text)]),f.createIdentifier('undefined'),...args]);
 }
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
  const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
  const vars=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,ts.visitNode(n.expression.expression,visit))],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,call('__get',[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const))];
  const body=f.createBlock([...vars,f.createReturnStatement(call('__call',[fn,recv,...n.arguments.map(a=>ts.visitNode(a,visit))]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 if(ts.isPropertyAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
 if(ts.isObjectLiteralExpression(n))return call('__object',[f.createArrayLiteralExpression(n.properties.map(p=>{assert(ts.isPropertyAssignment(p));return f.createArrayLiteralExpression([f.createStringLiteral(p.name.getText().replace(/^['"]|['"]$/g,'')),ts.visitNode(p.initializer,visit)]);} ))]);
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const classes={};
for(const kind of ['public','private']){
 const file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-${kind}.ts`),source=fs.readFileSync(file,'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration);
 const methods=methodNames.map(name=>{const node=cls.members.find(m=>m.name?.getText(ast)===name);assert(node,name);return node.getText(ast);}).join('\n');
 const interval=cls.members.find(m=>m.name?.getText(ast)==='PING_INTERVAL_MS');assert(interval);
 const className=cls.name.text;
 // Only selected member bodies are transformed. The class declaration/name and
 // static property retain actual source spelling and source constant value.
 const partial=ts.createSourceFile(file,`class ${className}{${interval.getText(ast)}\n${methods}}`,ts.ScriptTarget.Latest,true);
 const printer=ts.createPrinter();
 const members=partial.statements[0].members.map(m=>{
  if(m.name?.getText(partial)==='PING_INTERVAL_MS')return printer.printNode(ts.EmitHint.Unspecified,m,partial);
  const transformed=ts.transform(m.initializer,[transformer]);const init=printer.printNode(ts.EmitHint.Expression,transformed.transformed[0],partial);transformed.dispose();return `${m.name.getText(partial)} = ${init};`;
 }).join('\n');
 const logging=fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8');
 const context={...bind,exports:{}};
 vm.runInNewContext(ts.transpileModule(`${logging}\nclass ${className}{${members}}\nexports.Socket=${className};exports.LogSeverity=LogSeverity;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
 classes[kind]=context.exports;
}
const cases=[];
function add(kind,name,label,config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];
 const Socket=classes[kind].Socket,socket=tag(new Socket(),'socket'),error=tag({label},'error');
 const raw={hlPingTimeoutId:91.25,pingAt:17.5};let reads=0;
 const active=tag(new Set(['first','second']),'activeSet');
 g={kind,error,raw,active,...config};
 for(const key of ['hlPingTimeoutId','pingAt'])Object.defineProperty(socket,key,{configurable:true,get(){if(key==='hlPingTimeoutId'&&g.freshTimeout)return ++reads===1?91.25:17.5;return raw[key];},set(v){if(g.rejectSet===key)throw tag(new TypeError('setter rejected'),'error:TypeError');raw[key]=v;}});
 if(Object.hasOwn(config,'timeout'))raw.hlPingTimeoutId=config.timeout;
 socket.isDisposed=config.disposed??false;socket.isConnected=config.connected??true;socket.activeSubscriptions=active;
 const selected=socket[name];
 for(const m of ['onOpen','startPingLoop','schedulePing','stopPingLoop','ping','trackLastReceived','resetOrderbook','subscribePublicTopics','subscribe','sendWithDiagnostics'])socket[m]=tag(function(){return undefined;},m);
 socket.parent=tag({log:tag(function(){},'log')},'parent');
 const callbacks=[],timers=[];
 globals.setTimeout=tag(function(callback,delay){callbacks.push(callback);timers.push({id:g.timerId??53.5,delay,callback});return g.timerId??53.5;},'setTimeout');
 globals.clearTimeout=tag(function(id){const i=timers.findIndex(t=>Object.is(t.id,id));if(i>=0)timers.splice(i,1);},'clearTimeout');
 globals.performance=tag({now:tag(function(){return g.now??91.25;},'now')},'performance');
 globals.JSON=tag({stringify:tag(JSON.stringify,'stringify')},'JSON');
 globals.LogSeverity=tag(classes[kind].LogSeverity,'LogSeverity');
 globals.HyperliquidPublicWebSocket=tag(classes.public.Socket,'publicClass');globals.HyperliquidPrivateWebSocket=tag(classes.private.Socket,'privateClass');
 tag(active.clear,'clear');
 const input=native(socket),initialState=Object.fromEntries(Object.entries(raw).map(([k,v])=>[k,native(v)]));
 let result,thrown,callbackThrown;try{result=selected();}catch(e){thrown=e;}
 const mainStop=calls.length;
 if(config.fire&&callbacks.length){socket.isDisposed=config.callbackDisposed??false;socket.isConnected=config.callbackConnected??true;try{callbacks[0]();}catch(e){callbackThrown=e;}}
 const finalState=Object.fromEntries(Object.entries(raw).map(([k,v])=>[k,native(v)]));
 cases.push({kind,name,label,input,initialState,calls,mainStop,fire:!!config.fire,result:thrown?{throw:native(thrown)}:{value:native(result)},callbackResult:callbackThrown?{throw:native(callbackThrown)}:{value:native(undefined)},finalState,finalActive:[...active],finalTimers:timers.map(t=>({id:native(t.id),delay:native(t.delay),callback:native(t.callback)}))});
}
for(const kind of ['public','private']){
 for(const c of [{},{disposed:true},{disposed:0},{disposed:''},{disposed:{}},{throwGet:'isDisposed'},{throwCall:'trackLastReceived'},{throwCall:kind==='public'?'resetOrderbook':'subscribe'},{throwCall:'startPingLoop'}])add(kind,'onOpen','opening',c);
 if(kind==='public'){add(kind,'onOpen','log throws',{throwCall:'log'});add(kind,'onOpen','clear throws',{throwCall:'clear'});add(kind,'onOpen','subscribe throws',{throwCall:'subscribePublicTopics'});}
 for(const c of [{},{throwCall:'stopPingLoop'},{throwGet:'schedulePing'},{throwCall:'schedulePing'}])add(kind,'startPingLoop','deferred order',c);
 for(const c of [{},{fire:true},{fire:true,callbackDisposed:true},{fire:true,callbackConnected:false},{fire:true,callbackConnected:0},{fire:true,callbackConnected:''},{fire:true,callbackConnected:{}},{fire:true,throwCall:'ping'},{fire:true,throwCall:'schedulePing'},{throwGlobal:'setTimeout'},{throwGet:'PING_INTERVAL_MS'},{throwCall:'setTimeout'},{throwSet:'hlPingTimeoutId'},{rejectSet:'hlPingTimeoutId'},{timerId:0},{timerId:-0}])add(kind,'schedulePing','timer and callback',c);
 for(const timeout of [undefined,null,false,0,-0,NaN,'',91.25,'token',{}])add(kind,'stopPingLoop','timeout truthiness',{timeout});
 for(const c of [{freshTimeout:true},{throwGet:'hlPingTimeoutId'},{throwGlobal:'clearTimeout'},{throwCall:'clearTimeout'},{throwSet:'hlPingTimeoutId'},{rejectSet:'hlPingTimeoutId'}])add(kind,'stopPingLoop','fresh read and failure',c);
 for(const c of [{},{disposed:true},{disposed:0},{disposed:{}},{now:-0},{now:Infinity},{now:NaN},{throwGet:'isDisposed'},{throwGlobal:'performance'},{throwCall:'now'},{throwSet:'pingAt'},{rejectSet:'pingAt'},{throwGet:'sendWithDiagnostics'},{throwCall:'stringify'},{throwCall:'sendWithDiagnostics'}])add(kind,'ping','clock before payload',c);
}
assert.equal(cases.length,123,'fixed12+9+8+32+32+30');
console.log(JSON.stringify({names:['public','private'].flatMap(kind=>methodNames.map(name=>kind+'.'+name)),cases,iterationStop:122,scope:'ten source methods plus schedule callback body; virtual callees and timer/global/primitive adapter bindings; no full class/live socket claim'}));
