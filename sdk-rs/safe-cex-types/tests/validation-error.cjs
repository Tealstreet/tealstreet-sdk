// Execute the actual method with an explicitly prepared xhr response.data value.
// This is not a raw HTTP/Axios transform oracle.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function ast(relative){const file=path.join(root,relative);return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
const source=ast('packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const member=source.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(source)==='validateAccount');
if(!member?.initializer)throw Error('actual validateAccount missing');
const tables=ast('packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts');
const endpoints=tables.statements.filter(ts.isVariableStatement).flatMap(n=>n.declarationList.declarations).find(n=>n.name.getText(tables)==='KUCOIN_SPOT_ENDPOINTS');
if(!endpoints?.initializer)throw Error('actual endpoints missing');
let expression=member.initializer.getText(source);
if(process.env.P4_VALIDATION_CONTROL==='coerce')expression=expression.replace('return { error: data.msg ||', 'return { error: String(data.msg) ||');
const inputs=['42','-3','true','{"message":"venue error","code":197}','[]','[1,"x"]','"venue error"','0','false','null','""','-0','1e400','-1e400','-1e-400','{"nested":[-0,1e400,-1e400]}'];
function tag(value){if(typeof value==='number'){const d=new DataView(new ArrayBuffer(8));d.setFloat64(0,value);return {$bits:d.getBigUint64(0).toString(16).padStart(16,'0')};}if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)]));return value;}
(async()=>{const rows=[];for(const input of inputs)for(const caught of [false,true]){
 const context=vm.createContext({});
 const code=`const KUCOIN_SPOT_ENDPOINTS=${endpoints.initializer.getText(tables)}; globalThis.validate=${expression};`;
 vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const msg=JSON.parse(input),errors=[];let balanceCalls=0;
 context.xhr={get:async()=>{if(caught)throw {response:{data:{msg}},message:'outer error'};return {data:{code:'400000',msg}};}};
 context.fetchBalance=async()=>{balanceCalls++;throw Error('unexpected balance fetch');};
 context.emitAndLogError=value=>errors.push(tag(value));
 const result=await context.validate();
 rows.push({input,caught,result:tag(result),type:typeof result.error,errors,balanceCalls,sameValue:result.error===msg});
}console.log(JSON.stringify(rows));})().catch(e=>{console.error(e);process.exitCode=1;});
