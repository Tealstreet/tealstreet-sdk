const fs=require('node:fs'),path=require('node:path');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript',ts=require(compiler);
const runtime=require('node:module').createRequire(require.resolve(compiler));
const axios=runtime(process.env.KUCOIN_READS_MISSING_RUNTIME?'missing-kucoin-axios-control':'axios');
const transform=raw=>axios.defaults.transformResponse[0].call(axios.defaults,raw);
const responseData=c=>c.raw!==undefined?transform(c.raw):c.phase==='untransformed'?transform(c.data):c.phase==='encoded'?transform(JSON.stringify(c.data)):c.data;
const {root,load}=require('./tables.cjs');
const types=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
(async()=>{
const result=[];
for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${c.venue}.exchange.ts`);
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const member=source.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(source)==='getAccount');if(!member)throw Error('missing getAccount');
 const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const calls=[],errors=[],context={xhr:{async get(url){calls.push({method:'GET',url});if(c.failure){const e={...c.failure};if(e.response)e.response={...e.response,data:responseData(e.response)};throw e;}return {data:responseData(c)};}},emitAndLogError(message){errors.push(message===undefined?{$undefined:true}:message);}};
 const module={exports:{}};new Function('module',...Object.keys(types),code).call(context,module,...Object.values(types));
 const account=await module.exports();const observed=c.observeUser?{userKind:typeof account.userId,userLength:account.userId.length,userUnits:Array.from({length:account.userId.length},(_,i)=>account.userId.charCodeAt(i))}:{};result.push(c.raw===undefined?{...observed,account,calls,errors}:{...observed,accountJSON:JSON.stringify(account),calls,errors});
}
process.stdout.write(JSON.stringify(result));
})().catch(e=>{console.error(e);process.exitCode=1});
