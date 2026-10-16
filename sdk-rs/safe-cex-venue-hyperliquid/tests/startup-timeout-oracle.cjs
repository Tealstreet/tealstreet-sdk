const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),99,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const member=cls.members.find(n=>n.name?.getText(ast)==='withStartupTimeout');
assert(member?.initializer,'actual withStartupTimeout initializer');
const code=ts.transpileModule('return ('+member.initializer.getText(ast)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const NativePromise=Promise;
const scenes=['operation','rejection','fallback','zero-id','clear-error','log-error','drop-operation','drop-fallback'];
assert.equal(scenes.length,8,'independent timeout source scene floor');
(async()=>{
 let completed=0,last;
 for(const scene of scenes){
  const trace=[],original={},logError={},clearError={},value={},fallback={};let timer,timeoutPromise,resolveOperation,rejectOperation;
  const operation=new NativePromise((resolve,reject)=>{resolveOperation=resolve;rejectOperation=reject});
  function TracePromise(executor){trace.push('construct');timeoutPromise=new NativePromise(executor);return timeoutPromise}
  TracePromise.race=function(entries){assert.equal(this,TracePromise,'source native race receiver');assert.equal(entries.length,2,'source independent race input count');assert.equal(entries[0],operation,'source native first original operation');assert.equal(entries[1],timeoutPromise,'source native second timeout');trace.push('race');return NativePromise.race(entries)};
  const set=(callback,ms)=>{assert.equal(ms,1500,'source native delay');trace.push('setTimeout');timer=callback;return scene==='zero-id'?0:99};
  const clear=id=>{assert.equal(id,99,'source native cleanup handle');trace.push('clearTimeout');if(scene==='clear-error')throw clearError;timer=undefined};
  const target={log(message,severity){assert.equal(this,target,'source logger receiver');assert.equal(message,'probe timed out after 2s; continuing with fallback','source actual warning');assert.equal(severity,'warning','source warning severity');trace.push('log');if(scene==='log-error')throw logError}};
  const body=new Function('Promise','setTimeout','clearTimeout','LogSeverity',code).call(target,TracePromise,set,clear,{Warning:'warning'});
  let result;
  body.call(target,operation,1500,'probe',fallback).then(value=>{result={ok:value}},error=>{result={error}});
  assert.deepEqual(trace,['construct','setTimeout','race'],'source eager prefix order');
  if(['fallback','zero-id','log-error','drop-fallback'].includes(scene)){
   if(scene==='log-error')assert.throws(()=>timer(),e=>e===logError,'source original logging error');else timer();
  }else if(scene==='rejection')rejectOperation(original);else resolveOperation(value);
  for(let turn=0;turn<12;turn++)await NativePromise.resolve();
  if(scene==='log-error')assert.equal(result,undefined,'source throwing timer log leaves race pending');
  else if(scene==='rejection')assert.equal(result.error,original,'source original operation error');
  else if(scene==='clear-error')assert.equal(result.error,clearError,'source original cleanup error overrides return');
  else assert.equal(result.ok,['fallback','zero-id','drop-fallback'].includes(scene)?fallback:value,'source original result alias');
  const expected=['construct','setTimeout','race'];
  if(['fallback','zero-id','log-error','drop-fallback'].includes(scene))expected.push('log');
  if(!['zero-id','log-error'].includes(scene))expected.push('clearTimeout');
  assert.deepEqual(trace,expected,'source independent final effect order '+scene);
  completed++;last=scene;
 }
 assert.equal(completed,8,'independent completed timeout source count');
 assert.equal(last,'drop-fallback','source last completed scene');
 console.log(JSON.stringify({cases:completed,last}));
})().catch(e=>{console.error(e);process.exitCode=1});
