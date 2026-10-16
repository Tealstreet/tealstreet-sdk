// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
const {bits:captureBits}=require('./numeric-bits.cjs');
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_READS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const {root,load}=require('./tables.cjs');
const env=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const member=source.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(source)==='fetchActiveOrderSymbols');
if(!member?.initializer)throw Error('missing live fetchActiveOrderSymbols');
let expr=member.initializer.getText(source);
if(process.env.KUCOIN_SPOT_READ_MUTATION==='coerce-symbols'){
 const old='? data.data.symbols : []';if(!expr.includes(old))throw Error('missing symbols control');expr=expr.replace(old,'? data.data.symbols.map(String) : []');
}
const moduleValue={exports:{}};const context={};
const code=ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
new Function('module',...Object.keys(env),code).call(context,moduleValue,...Object.values(env));
(async()=>{
 const results=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const calls=[];const data=c.raw===undefined?c.data:transform(c.raw);
  context.xhr={get:async url=>{calls.push({method:'GET',url});if(c.failure)throw c.failure;return{data};}};
  const result=await moduleValue.exports();
  if(c.alias){const before=JSON.stringify(result),same=result===data.data.symbols;data.data.symbols[0]='later';data.data.symbols.push('added');results.push({calls,alias:{before,same,result,gateway:data.data.symbols}});}
  else if(c.bits)results.push({calls,bits:result.map(captureBits)});
  else if(c.rawOnly)results.push({calls,json:JSON.stringify(result)});
  else results.push({calls,result});
 }
 console.log(JSON.stringify(results.map(row=>({...row,captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0])})),(_key,value)=>typeof value==='number'&&!Number.isFinite(value)?{$number:String(value)}:value));
})().catch(error=>{console.error(error);process.exitCode=1;});
