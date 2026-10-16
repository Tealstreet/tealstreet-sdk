const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='WOOXExchange');
const constructor=klass.members.find(ts.isConstructorDeclaration);assert(constructor);assert.equal(constructor.parameters.length,3);
const code=ts.transpileModule(`this.C=class WOOXExchange extends BaseExchange {${constructor.getText(ast)}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const out=[];
for(const stop of JSON.parse(fs.readFileSync(0,'utf8'))){
 const trace=[],sentinel=Error('constructor sentinel');
 const record=key=>{trace.push(key);if(stop===key)throw sentinel;};
 const options={id:'original-options'},store={},first={id:'first-options-read'},second={id:'second-options-read'},proxyOptions={id:'proxy-options'};
 const innerResult={},outerResult={};const clients=[{},{},{}],proxy={},sockets=[{},{}],handler=()=>{};let receiver,reads=0;
 class BaseExchange {
  constructor(exchangeName,opts,s){assert.equal(exchangeName,'woo');assert.equal(opts,options);assert.equal(s,store);record('super');receiver=this;
   for(const key of ['xhr','_primaryXhr','_secondaryXhr','_proxyEnforcedXhr','publicWebsocket','privateWebsocket'])Object.defineProperty(this,key,{configurable:true,get(){return this['_value:'+key];},set(value){record('set:'+key);this['_value:'+key]=value;}});
   Object.defineProperty(this,'options',{get(){record('get:options');return reads++===0?first:second;}});
   Object.defineProperty(this,'on',{get(){record('get:on');return function(event,callback){assert.equal(this,receiver);assert.equal(event,'fill');assert.equal(callback,handler);record('on:fill');};}});
   Object.defineProperty(this,'handleFillForPositionSync',{get(){record('get:handleFillForPositionSync');return handler;}});
  }
 }
 const transport={get requireProxy(){record('get:requireProxy');return function(opts){assert.equal(this,transport);assert.equal(opts,second);record('requireProxy');return proxyOptions;};}};
 const context={BaseExchange,
  createAPI(opts,parent){assert.equal(opts,proxyOptions);assert.equal(parent,receiver);record('createAPI:proxy');return proxy;},
  getTransport(opts){assert.equal(opts,first);record('getTransport');return transport;},
  createApiWithSecondaryKey(opts,parent,api,extra){assert.equal(opts,options);assert.equal(parent,receiver);assert.equal(api,context.createAPI);assert.equal(extra.rateLimit.name,'rateLimit');assert.equal(extra.rateLimit.length,1);record('factory');assert.equal(extra.rateLimit(clients[0]),outerResult);
   return {[Symbol.iterator](){let i=0;return {next(){record('next:'+i);return {done:false,value:clients[i++]};},return(){record('iterator:return');return {done:true};}};}};
  },
  rateLimit(xhr,opts){assert([4,20].includes(opts.maxRequests));assert.deepEqual(Object.keys(opts),opts.maxRequests===4?['perMilliseconds','maxRequests']:['maxRequests','perMilliseconds']);assert.equal(opts.perMilliseconds,opts.maxRequests===4?1000:6000);record(`rate:${opts.maxRequests}:${opts.perMilliseconds}`);assert.equal(xhr,opts.maxRequests===4?clients[0]:innerResult);return opts.maxRequests===4?innerResult:outerResult;},
  WooPublicWebsocket:class{constructor(parent){assert.equal(parent,receiver);record('construct:public');return sockets[0];}},
  WooPrivateWebscoket:class{constructor(parent){assert.equal(parent,receiver);record('construct:private');return sockets[1];}},
 };
 vm.runInNewContext(code,context);let result,error;
 try{result=Reflect.construct(context.C,['woo',options,store,'ignored-extra']);}catch(e){error=e;}
 if(stop)assert.equal(error,sentinel);else{assert.equal(error,undefined);assert.equal(result,receiver);for(const [key,expected]of[['xhr',clients[0]],['_primaryXhr',clients[1]],['_secondaryXhr',clients[2]],['_proxyEnforcedXhr',proxy],['publicWebsocket',sockets[0]],['privateWebsocket',sockets[1]]])assert.equal(result[key],expected);}
 out.push({stop,trace});
}
assert.equal(out.length,13);process.stdout.write(JSON.stringify(out));
