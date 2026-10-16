'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const sourceNames={public:['connectAndSubscribe'],private:['connectAndSubscribe','subscribe'],spot_public:['connectAndSubscribe','onOpen','ping'],spot_private:['connectAndSubscribe','onOpen','ping','subscribe']};
let g,ids,tags,nextId,calls;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const tag=(v,t)=>(tags.set(v,t),v);
function native(v){
 if(v===undefined)return ['undefined'];if(v===null)return ['null'];
 if(typeof v==='boolean')return ['bool',v];if(typeof v==='string')return ['string',units(v)];
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return ['number',Number.isNaN(v)?'nan':b.toString('hex')];}
 if(!ids.has(v))ids.set(v,nextId++);return ['ref',ids.get(v),tags.get(v)||(v?.name?.endsWith('Error')?'error:'+v.name:typeof v)];
}
function op(name,args,fn){const e={op:name,args};calls.push(e);try{const r=fn();e.result=native(r);return r;}catch(e0){e.throw=native(e0);throw e0;}}
const globals={};
const bind={
 __global:name=>op('global',name==='WSS_URLS'?{name,venue:g.kind.startsWith('spot_')?'spot':'perp'}:{name},()=>{if(g.throwGlobal===name)throw g.error;return globals[name];}),
 __get:(object,name)=>op('get',{object:native(object),name},()=>{if(g.throwGet===name)throw g.error;return Reflect.get(object,name);}),
 __assign:(object,name,value)=>op('set',{object:native(object),name,value:native(value)},()=>{if(g.throwSet===name)throw g.error;object[name]=value;return undefined;}),
 __call:(fn,receiver,...args)=>op('call',{function:native(fn),receiver:native(receiver),args:args.map(native)},()=>{if(tags.get(fn)===g.throwCall)throw g.error;return Reflect.apply(fn,receiver,args);}),
 __object:pairs=>{const object=op('object',{},()=>tag({},'literal'));for(const [name,value] of pairs)op('define',{object:native(object),name,value:native(value)},()=>{Object.defineProperty(object,name,{value,writable:true,enumerable:true,configurable:true});});return object;},
};
const globalNames=new Set(['performance','JSON','WSS_URLS']);
function transformer(context){const f=ts.factory,call=(name,args)=>f.createCallExpression(f.createIdentifier(name),undefined,args);const visit=n=>{
 if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.EqualsToken&&ts.isPropertyAccessExpression(n.left))return call('__assign',[ts.visitNode(n.left.expression,visit),f.createStringLiteral(n.left.name.text),ts.visitNode(n.right,visit)]);
 if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)){
  const recv=f.createIdentifier('__receiver'),fn=f.createIdentifier('__function');
  const vars=[f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(recv,undefined,undefined,ts.visitNode(n.expression.expression,visit))],ts.NodeFlags.Const)),f.createVariableStatement(undefined,f.createVariableDeclarationList([f.createVariableDeclaration(fn,undefined,undefined,call('__get',[recv,f.createStringLiteral(n.expression.name.text)]))],ts.NodeFlags.Const))];
  const body=f.createBlock([...vars,f.createReturnStatement(call('__call',[fn,recv,...n.arguments.map(a=>ts.visitNode(a,visit))]))],true);
  return f.createCallExpression(f.createParenthesizedExpression(f.createArrowFunction(undefined,undefined,[],undefined,f.createToken(ts.SyntaxKind.EqualsGreaterThanToken),body)),undefined,[]);
 }
 if(ts.isElementAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),ts.visitNode(n.argumentExpression,visit)]);
 if(ts.isPropertyAccessExpression(n))return call('__get',[ts.visitNode(n.expression,visit),f.createStringLiteral(n.name.text)]);
 if(ts.isIdentifier(n)&&globalNames.has(n.text))return call('__global',[f.createStringLiteral(n.text)]);
 if(ts.isObjectLiteralExpression(n))return call('__object',[f.createArrayLiteralExpression(n.properties.map(p=>{assert(ts.isPropertyAssignment(p));return f.createArrayLiteralExpression([f.createStringLiteral(p.name.getText().replace(/^['"]|['"]$/g,'')),ts.visitNode(p.initializer,visit)]);} ))]);
 return ts.visitEachChild(n,visit,context);
};return s=>ts.visitNode(s,visit);}
const classes={};
for(const [kind,names] of Object.entries(sourceNames)){
 const stem=kind.startsWith('spot_')?'hyperliquidspot':'hyperliquid',side=kind.endsWith('public')?'public':'private';
 const file=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.ws-${side}.ts`),source=fs.readFileSync(file,'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(ts.isClassDeclaration),printer=ts.createPrinter();
 const methods=names.map(name=>{const m=cls.members.find(m=>m.name?.getText(ast)===name);assert(m,name);const transformed=ts.transform(m.initializer,[transformer]);const init=printer.printNode(ts.EmitHint.Expression,transformed.transformed[0],ast);transformed.dispose();return `${name} = ${init};`;}).join('\n');
 const typesFile=path.join(root,`packages/safe-cex/src/exchanges/hyperliquid/${stem}.types.ts`),typesSource=fs.readFileSync(typesFile,'utf8'),typesAst=ts.createSourceFile(typesFile,typesSource,ts.ScriptTarget.Latest,true);
 const urls=typesAst.statements.filter(ts.isVariableStatement).flatMap(n=>n.declarationList.declarations).find(n=>n.name.getText(typesAst)==='WSS_URLS');assert(urls);
 const context={...bind,exports:{}};
 vm.runInNewContext(ts.transpileModule(`const WSS_URLS=${urls.initializer.getText(typesAst)};class ${cls.name.text}{${methods}}\nexports.Socket=${cls.name.text};exports.urls=WSS_URLS;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
 classes[kind]=context.exports;
}
const cases=[];
function add(kind,name,label,config={}){
 ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];
 const socket=tag(new classes[kind].Socket(),'socket'),error=tag({label},'error');
 const listeners=new Map();let listenerCalls=0,wsReads=0;
 function websocket(label){const ws=tag({addEventListener:tag(function(event,handler){if(++listenerCalls===g.throwAtListener)throw error;listeners.get(ws).push([event,handler]);},'addEventListener')},label);listeners.set(ws,[]);return ws;}
 const old=websocket('oldWs'),created=websocket('createdWs'),alternate=websocket('alternateWs');
 const raw={ws:old,pingAt:17.5};
 g={kind,error,raw,...config};
 for(const key of ['ws','pingAt'])Object.defineProperty(socket,key,{configurable:true,get(){if(key==='ws'&&g.freshWs&&++wsReads===2)return alternate;return raw[key];},set(v){if(g.rejectSet===key)throw tag(new TypeError('setter rejected'),'error:TypeError');raw[key]=v;}});
 socket.isDisposed=config.disposed??false;
 const selected=socket[name];
 for(const m of ['onOpen','onMessage','onClose','trackLastReceived','resetOrderbook','subscribePublicTopics','subscribe','ping','send','subscribeSystemTopics','subscribeAllActiveAssetData','subscribeCoins'])socket[m]=tag(function(){return undefined;},m);
 socket.createWebsocket=tag(function(){return Object.hasOwn(g,'createdValue')?g.createdValue:created;},'createWebsocket');
 const options=tag({testnet:Object.hasOwn(config,'testnet')?config.testnet:false},'options');
 socket.parent=tag({options,requestSpotBalanceRefresh:tag(function(){},'requestSpotBalanceRefresh')},'parent');
 globals.WSS_URLS=tag(classes[kind].urls,kind.startsWith('spot_')?'spotURLs':'perpURLs');
 globals.performance=tag({now:tag(function(){return Object.hasOwn(g,'now')?g.now:91.25;},'now')},'performance');
 globals.JSON=tag({stringify:tag(JSON.stringify,'stringify')},'JSON');
 const input=native(socket),initialState=Object.fromEntries(Object.entries(raw).map(([k,v])=>[k,native(v)]));
 const initialWebsockets=[...listeners].map(([ws])=>native(ws));
 let result,thrown;try{result=selected(config.reconnect);}catch(e){thrown=e;}
 const finalState=Object.fromEntries(Object.entries(raw).map(([k,v])=>[k,native(v)]));
 const finalListeners=[...listeners].map(([ws,values])=>({ws:native(ws),listeners:values.map(([event,handler])=>[units(event),native(handler)])}));
 cases.push({kind,name,label,input,initialState,initialWebsockets,calls,result:thrown?{throw:native(thrown)}:{value:native(result)},finalState,finalListeners});
}
for(const kind of Object.keys(sourceNames)){
 for(const testnet of [undefined,null,false,0,-0,NaN,'',true,17.5,'yes',{}])add(kind,'connectAndSubscribe','URL truthiness',{testnet});
 for(const c of [{disposed:true},{disposed:0},{disposed:{}},{reconnect:true},{reconnect:'opaque'},{freshWs:true},{throwGet:'isDisposed'},{throwGet:'createWebsocket'},{throwGlobal:'WSS_URLS'},{throwGet:'parent'},{throwGet:'options'},{throwGet:'testnet'},{throwGet:'PRODUCTION'},{throwCall:'createWebsocket'},{throwSet:'ws'},{rejectSet:'ws'},{throwGet:'addEventListener'},{throwGet:'onOpen'},{throwGet:'onMessage'},{throwGet:'onClose'},{throwAtListener:1},{throwAtListener:2},{throwAtListener:3},{createdValue:null},{createdValue:undefined}])add(kind,'connectAndSubscribe','connection effects',c);
}
for(const kind of ['spot_public','spot_private']){
 for(const c of [{},{disposed:true},{disposed:0},{disposed:{}},{throwGet:'isDisposed'},{throwCall:'trackLastReceived'},{throwCall:kind==='spot_public'?'resetOrderbook':'subscribe'},{throwCall:'ping'},{throwCall:kind==='spot_public'?'subscribePublicTopics':'requestSpotBalanceRefresh'}])add(kind,'onOpen','spot opening',c);
 for(const c of [{},{disposed:true},{disposed:0},{disposed:{}},{now:-0},{now:Infinity},{now:NaN},{throwGet:'isDisposed'},{throwGlobal:'performance'},{throwCall:'now'},{throwSet:'pingAt'},{rejectSet:'pingAt'},{throwGet:'send'},{throwCall:'stringify'},{throwCall:'send'}])add(kind,'ping','spot clock and payload',c);
}
for(const kind of ['private','spot_private'])for(const c of [{},{throwGet:'subscribeSystemTopics'},{throwCall:'subscribeSystemTopics'},{throwGet:kind==='private'?'subscribeAllActiveAssetData':'subscribeCoins'},{throwCall:kind==='private'?'subscribeAllActiveAssetData':'subscribeCoins'}])add(kind,'subscribe','private dispatcher',c);
assert.equal(cases.length,202,'independent4*36+2*9+2*15+2*5');
console.log(JSON.stringify({names:Object.entries(sourceNames).flatMap(([kind,names])=>names.map(name=>kind+'.'+name)),cases,iterationStop:201,scope:'ten connection/opening/ping/private dispatch leaves; virtual callees and browser/global/JSON primitives are host bindings; full classes/live sockets unproved'}));
