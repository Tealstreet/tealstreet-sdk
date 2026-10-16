// Execute the actual SDK cookie helper and its installed parser dependency.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),{createRequire}=require('node:module');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../thirdparty/grvt-ts-sdk/src/utils/cookie.ts');
const source=fs.readFileSync(file,'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const context={exports:{},require:createRequire(file),console:{warn(){},error(){}}};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const expires='; Expires=Wed, 15 Nov 2023 00:00:00 GMT';
const cases=[
 ['plain','gravity=token'+expires],['uri','gravity=a%2Fb%20c'+expires],['quotes','gravity="quoted"'+expires],['bad-uri','gravity=%E2'+expires],['empty','gravity='+expires],['upper','Gravity=x'+expires],['combined','other=x, gravity=token'+expires],['expires-comma','gravity=x'+expires+', other=y; Max-Age=42'],['attribute','other=x; gravity=wrong'+expires],['first-array',['other=x'+expires,'gravity=second'+expires]],['array',['gravity=first'+expires,'gravity=second'+expires]],['missing-date','gravity=x'],['invalid-date','gravity=x; Expires=bad'],['last-date','gravity=x; Expires=bad'+expires],['missing-header',null],['non-200','gravity=x'+expires,201],['upper-header','gravity=upper'+expires,200,true],['account-case','gravity=account'+expires,200,false,true]
];
(async()=>{
 const rows=[];
 for(const [name,raw,status=200,upper=false,accountUpper=false]of cases){
  const response={status,headers:{[upper?'Set-Cookie':'set-cookie']:raw,[accountUpper?'X-Grvt-Account-Id':'x-grvt-account-id']:'account-42'}};
  const output=await context.exports.getCookieWithExpiration('/login','key',{async post(){return response}});
  if(output&&Number.isNaN(output.expires))output.expires={$number:'NaN'};
  rows.push({name,response,output});
 }
 assert.equal(rows.length,18);process.stdout.write(JSON.stringify(rows));
})().catch(e=>{process.stderr.write(String(e.stack));process.exitCode=1;});
