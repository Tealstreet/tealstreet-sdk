// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Execute the live read, real omitUndefined and installed Axios/qs. Capture
// native query and output numbers before JSON can erase NaN or signed zero.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_FILLS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const qs=runtime('qs'),{root,load}=require('./tables.cjs');
const moduleOptions={target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true};
const omitModule={exports:{}};
new Function('module','exports','require',ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'),'utf8'),{compilerOptions:moduleOptions}).outputText)(omitModule,omitModule.exports,runtime);
const env={...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts')),...load(path.join(root,'packages/safe-cex/src/types.ts')),...omitModule.exports};
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const apiFile=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.api.ts');
const apiAst=ts.createSourceFile(apiFile,fs.readFileSync(apiFile,'utf8'),ts.ScriptTarget.Latest,true);
let serializer;
function findSerializer(node){if(ts.isPropertyAssignment(node)&&node.name.getText(apiAst)==='serialize')serializer=node.initializer.getText(apiAst);ts.forEachChild(node,findSerializer);}
findSerializer(apiAst);if(!serializer)throw Error('missing live Spot params serializer');
const serializerModule={exports:{}};
new Function('module','qs',ts.transpileModule('module.exports='+serializer,{compilerOptions:moduleOptions}).outputText)(serializerModule,qs);
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const member=ast.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(ast)==='fetchHistoricalFills');
if(!member?.initializer)throw Error('missing live fetchHistoricalFills');
let expr=member.initializer.getText(ast);
if(process.env.KUCOIN_FILLS_MUTATION==='number'){
 if(!expr.includes('parseFloat('))throw Error('missing parseFloat control');
 expr=expr.replaceAll('parseFloat(','Number(');
}
const context={},mod={exports:{}};let clockCalls=0;
new Function('module','Date',...Object.keys(env),ts.transpileModule('module.exports='+expr,{compilerOptions:moduleOptions}).outputText).call(context,mod,{now:()=>{clockCalls++;return 1700000000123;}},...Object.values(env));
const {bits}=require('./numeric-bits.cjs');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const opts=c.opts||{};for(const [k,v]of Object.entries(c.special||{}))opts[k]=Number(v);
  clockCalls=0;const calls=[],errors=[];
  context.xhr={get:async(url,{params})=>{
   calls.push({url,params,keys:Object.keys(params),query:serializerModule.exports(params),bits:Object.fromEntries(Object.entries(params).filter(([,v])=>typeof v==='number').map(([k,v])=>[k,bits(v)]))});
   if(c.failure){const e={...c.failure};if(e.response?.raw!==undefined)e.response={...e.response,data:transform(e.response.raw)};throw e;}
   return{data:c.raw===undefined?c.data:transform(c.raw)};
  }};
  context.emitAndLogError=e=>errors.push(e);
  const fills=await mod.exports(opts);
  out.push({fills,errors,calls,clockCalls,bits:fills.map(f=>['timestamp','price','qty','commission'].map(k=>bits(f[k]))),keys:fills.map(Object.keys)});
 }
 console.log(JSON.stringify(out.map(row=>({...row,captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0])}))));
})().catch(e=>{console.error(e);process.exitCode=1;});
