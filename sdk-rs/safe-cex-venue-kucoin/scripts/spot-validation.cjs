// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Execute validateAccount itself; fetchBalance is its explicit async dependency.
const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript';
const ts=require(compiler);
const runtimeRequire=require('node:module').createRequire(require.resolve(compiler));
const axios=runtimeRequire(process.env.KUCOIN_VALIDATION_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
// Preserve Axios's default permissive response transform, including raw strings.
const responseData=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const {root,load}=require('./tables.cjs');
const env=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const member=ast.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(ast)==='validateAccount');
if(!member?.initializer)throw Error('missing live validateAccount');
let expr=member.initializer.getText(ast);
if(process.env.KUCOIN_VALIDATION_MUTATION==='propagate-balance') {
 if(!expr.includes('.catch(() => undefined)'))throw Error('missing balance catch control');
 expr=expr.replace('.catch(() => undefined)','');
}
const context={},mod={exports:{}};
new Function('module',...Object.keys(env),ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText).call(context,mod,...Object.values(env));
const {bits}=require('./numeric-bits.cjs');
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  const calls=[],errors=[];let balanceCalls=0;
  const balance=c.balanceRaw===undefined?(c.balance||{used:1,free:2,total:3,upnl:0}):JSON.parse(c.balanceRaw);
  context.xhr={get:async(url,opts)=>{
   calls.push({method:'GET',url,params:opts.params});
   if(c.failure){const e={...c.failure};if(e.response?.raw!==undefined)e.response={...e.response,data:responseData(e.response.raw)};throw e;}
   return{data:c.raw===undefined?c.data:responseData(c.raw)};
  }};
  context.fetchBalance=async()=>{balanceCalls++;if(c.balanceFailure)throw Error('balance rejected');return balance;};
  context.emitAndLogError=error=>errors.push(error);
  const result=await mod.exports();
  const numericBits=result.balance?[result.balance.used,result.balance.free,result.balance.total,result.balance.upnl].map(bits):null;
  const captured={captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0]),result,errors,calls,balanceCalls,ownBalance:Object.hasOwn(result,'balance'),bits:numericBits};
  if(c.captureRejection){captured.rejectionJSON=JSON.stringify(result);captured.errorKind=typeof result.error;captured.errorBits=typeof result.error==='number'?bits(result.error):null;captured.errorKeys=result.error&&typeof result.error==='object'?Object.keys(result.error):null;captured.privateNumber=result.error?.['$serde_json::private::Number'];captured.result=null;}
  if(c.alias){captured.before=JSON.parse(JSON.stringify(result));balance.free=77;captured.fetched=balance;}
  if(c.surrogate){captured.resultJson=JSON.stringify(result);captured.errorsJson=JSON.stringify(errors);captured.result=null;captured.errors=[];}
  out.push(captured);
 }
 console.log(JSON.stringify(out));
})().catch(e=>{console.error(e);process.exitCode=1;});
