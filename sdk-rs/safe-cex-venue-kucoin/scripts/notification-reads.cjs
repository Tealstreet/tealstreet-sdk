// Live venue methods, BaseExchange reporting and installed TinyEmitter fanout.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const {TinyEmitter}=runtime(process.env.KUCOIN_NOTIFICATION_MISSING_RUNTIME?'missing-kucoin-tiny-emitter':'tiny-emitter');
const axios=runtime('axios'),{root,load}=require('./tables.cjs');
const {DefaultStore}=require('./spot-store-notifications.cjs');
const options={target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS};
function ast(relative){const file=path.join(root,'packages/safe-cex/src',relative);return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
const base=ast('exchanges/base.ts'),cls=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseExchange');
const method=cls.members.find(n=>n.name?.getText(base)==='emitAndLogError');
if(!method)throw Error('missing actual error emitter');
const enhancer=ast('utils/axiosErrorEnhancer.ts');
const formatter=enhancer.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='formatErrorForSentry');
if(!formatter)throw Error('missing actual error formatter');
const contextFns=load(path.join(root,'packages/safe-cex/src/utils/httpFailureContext.ts'));
const code=ts.transpileModule(`exports.Reporter=class {${method.getText(base)}};\n${formatter.getText(enhancer)}`,{compilerOptions:options}).outputText;
const exportsObject={};
new Function('exports','readHttpFailureContext','reportException','console','window',code)(exportsObject,contextFns.readHttpFailureContext,()=>{}, {error(){},trace(){},warn(){}},{});
const environments={},methods={};
for(const venue of ['kucoin','kucoinspot']) {
 const source=ast(`exchanges/kucoin/${venue}.exchange.ts`),venueClass=source.statements.find(ts.isClassDeclaration);
 environments[venue]={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.types.ts`)),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'))};
 methods[venue]=Object.fromEntries(venueClass.members.filter(n=>n.initializer&&n.name).map(n=>[n.name.getText(source),n.initializer.getText(source)]));
}
function bind(venue,name,context){const expression=methods[venue][name];if(!expression)throw Error('missing actual '+name);const module={exports:{}};const env=environments[venue];new Function('module',...Object.keys(env),ts.transpileModule('module.exports='+expression,{compilerOptions:options}).outputText).call(context,module,...Object.values(env));return module.exports;}
(async()=>{
 const output=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const events=[],emitter=new TinyEmitter();let emission=0,calls=0;
  const sentinel=new TypeError('listener failure');sentinel.response={data:{msg:'caught listener body'}};
  const catchSentinel=new RangeError('catch listener failure');catchSentinel.response={data:{msg:'second listener body'}};
  emitter.on('error',message=>{events.push(['first',message]);emission++;if(c.fail==='always'||(c.fail==='once'&&emission===1))throw emission===1?sentinel:catchSentinel;});
  emitter.on('error',message=>events.push(['second',message]));
  const context=new exportsObject.Reporter();Object.assign(context,{name:'kucoin',options:{testnet:false,publicDataOnly:false},isDisposed:false,emitter,store:new DefaultStore()});
  const transportFailure={message:'transport failure',code:'ECONNRESET'};
  const request=async()=>{calls++;if(c.transport)throw transportFailure;return{data:axios.defaults.transformResponse[0].call(axios.defaults,c.raw===undefined?JSON.stringify({code:'bad',msg:'venue rejection'}):c.raw)};};
  context.xhr={get:request};context.unlimitedXHR={delete:request};
  context.fetchBalance=async()=>{throw Error('unexpected balance read');};
  if(c.method==='fetchOrders')for(const name of ['fetchNormalOrders','fetchAlgoOrders','reconcileFetchedOrderRemainders'])context[name]=bind(c.venue,name,context);
  let rejected;
  try{await bind(c.venue,c.method,context)(c.method==='cancelOrders'?c.orders:c.symbol);}catch(error){rejected={message:error.message,name:error.name,same:error===sentinel||error===catchSentinel,sameOriginal:error===transportFailure,which:error===sentinel?1:error===catchSentinel?2:0};}
  output.push({events,calls,resolved:rejected===undefined,...(rejected?{rejected}:{})});
 }
 console.log(JSON.stringify(output));
})().catch(error=>{console.error(error);process.exitCode=1;});
